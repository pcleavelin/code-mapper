use crate::index::{File, Index, Reader, SymRef, w_str};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Author {
    Human = 0,
    Ai = 1,
}

impl Author {
    pub fn tag(self) -> &'static str {
        match self {
            Author::Human => "",
            Author::Ai => " (ai)",
        }
    }
    fn from_u8(b: u8) -> Option<Author> {
        match b {
            0 => Some(Author::Human),
            1 => Some(Author::Ai),
            _ => None,
        }
    }
}

/// What a path describes. A tag only: listed and filterable, no rendering difference.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A workflow: what happens when X.
    Flow = 0,
    /// An abstraction boundary: the functions that form its surface.
    Layer = 1,
    /// A data structure and what mutates it.
    Type = 2,
}

impl Kind {
    pub const NAMES: [&'static str; 3] = ["flow", "layer", "type"];
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "flow" => Some(Kind::Flow),
            "layer" => Some(Kind::Layer),
            "type" => Some(Kind::Type),
            _ => None,
        }
    }
    fn from_u8(b: u8) -> Option<Kind> {
        Self::parse(Self::NAMES.get(b as usize)?)
    }
}

/// Pins a slice of lines. Offsets are relative to the start of the enclosing symbol so the anchor
/// survives edits elsewhere in the file. `symbol == ""` means absolute lines. `hash` detects when
/// the anchored text itself changed (-> stale). `parent` makes a path a tree: the step this one
/// is reached from, or -1 for a root.
#[derive(Clone)]
pub struct Anchor {
    pub file: String,
    pub symbol: String,
    pub off_start: i32,
    pub off_end: i32,
    pub hash: u64,
    pub author: Author,
    pub note: String,
    pub parent: i32,

    // resolved per session, not stored
    pub line_start: usize,
    pub line_end: usize,
    pub stale: bool,
}

pub struct PathDef {
    pub name: String,
    pub kind: Kind,
    pub note: String,
    pub author: Author,
    pub anchors: Vec<Anchor>,
}

#[derive(Default)]
pub struct Map {
    pub paths: Vec<PathDef>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    Same,
    Added,
    Removed,
    Changed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepChange {
    Added,
    Repinned,
    NoteEdited,
}

impl StepChange {
    pub fn tag(self) -> &'static str {
        match self {
            StepChange::Added => "new",
            StepChange::Repinned => "re-pinned",
            StepChange::NoteEdited => "note edited",
        }
    }
}

/// One path's difference against the parent revision's map.
pub struct PathDiff {
    pub name: String,
    pub change: Change,
    pub note_changed: bool,         // the path note or kind
    pub steps: Vec<Option<StepChange>>, // per step of the working path
    pub removed: Vec<Anchor>,       // base steps no longer present (unresolved)
}

// ---- binary file format ----------------------------------------------------------
// "CMAP" u32 version
// u32 npaths { str name, u8 kind, str note, u8 author, u32 nanchors {
//     str file, str symbol, i32 off_start, i32 off_end, u64 hash, u8 author, str note, i32 parent } }
// str = u32 len + utf8 bytes. All little-endian. Any other version is rejected: an old map is
// regenerated, never migrated.

const MAGIC: &[u8; 4] = b"CMAP";
const VERSION: u32 = 5;

impl Map {
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut b = Vec::with_capacity(4096);
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&VERSION.to_le_bytes());
        b.extend_from_slice(&(self.paths.len() as u32).to_le_bytes());
        for p in &self.paths {
            w_str(&mut b, &p.name);
            b.push(p.kind as u8);
            w_str(&mut b, &p.note);
            b.push(p.author as u8);
            b.extend_from_slice(&(p.anchors.len() as u32).to_le_bytes());
            for a in &p.anchors {
                w_str(&mut b, &a.file);
                w_str(&mut b, &a.symbol);
                b.extend_from_slice(&a.off_start.to_le_bytes());
                b.extend_from_slice(&a.off_end.to_le_bytes());
                b.extend_from_slice(&a.hash.to_le_bytes());
                b.push(a.author as u8);
                w_str(&mut b, &a.note);
                b.extend_from_slice(&a.parent.to_le_bytes());
            }
        }
        std::fs::write(path, b)
    }

    pub fn load(path: &Path) -> Option<Map> {
        Map::from_bytes(&std::fs::read(path).ok()?)
    }

    /// The map as committed in the parent revision, by shelling out to jj. None when there is
    /// no jj repo, no committed map, or it cannot be read.
    pub fn base_from_vcs(root: &Path) -> Option<Map> {
        let out = std::process::Command::new("jj").args(["file", "show", "-r", "@-", ".codemap"]).current_dir(root).output().ok()?;
        if !out.status.success() {
            return None;
        }
        Map::from_bytes(&out.stdout)
    }

    pub fn from_bytes(data: &[u8]) -> Option<Map> {
        let mut r = Reader { data, off: 0 };
        if r.bytes(4)? != MAGIC {
            return None;
        }
        if r.u32()? != VERSION {
            return None;
        }
        let mut m = Map::default();
        for _ in 0..r.u32()? {
            let mut p = PathDef {
                name: r.str()?,
                kind: Kind::from_u8(r.u8()?)?,
                note: r.str()?,
                author: Author::from_u8(r.u8()?)?,
                anchors: Vec::new(),
            };
            for _ in 0..r.u32()? {
                p.anchors.push(Anchor {
                    file: r.str()?,
                    symbol: r.str()?,
                    off_start: r.i32()?,
                    off_end: r.i32()?,
                    hash: r.u64()?,
                    author: Author::from_u8(r.u8()?)?,
                    note: r.str()?,
                    parent: r.i32()?,
                    line_start: 0,
                    line_end: 0,
                    stale: true,
                });
            }
            m.paths.push(p);
        }
        Some(m)
    }

    // ---- mutations shared by the GUI and the CLI ----

    pub fn find(&self, name: &str) -> Option<usize> {
        self.paths.iter().position(|p| p.name == name)
    }

    /// Returns the existing path of that name, or creates it.
    pub fn add_path(&mut self, name: &str, kind: Kind, author: Author) -> usize {
        if let Some(pi) = self.find(name) {
            return pi;
        }
        self.paths.push(PathDef { name: name.to_owned(), kind, note: String::new(), author, anchors: Vec::new() });
        self.paths.len() - 1
    }

    /// Re-anchor an existing step to new lines. Note and parent stay; the pinning author is
    /// recorded.
    pub fn pin_anchor(&mut self, idx: &Index, pi: usize, ai: usize, fi: usize, ls: usize, le: usize, author: Author) {
        let old = &self.paths[pi].anchors[ai];
        let mut a = Anchor::new(&idx.files[fi], ls, le);
        a.author = author;
        a.note = old.note.clone();
        a.parent = old.parent;
        self.paths[pi].anchors[ai] = a;
    }

    /// Whether any current (non-stale) step overlaps lines `[start, end]` of `file`. Coverage is
    /// derived here and never stored.
    pub fn covers(&self, file: &str, start: usize, end: usize) -> bool {
        self.paths.iter().flat_map(|p| &p.anchors).any(|a| !a.stale && a.file == file && a.line_start <= end && start <= a.line_end)
    }

    /// Append a step under `parent` (-1 = root). Returns its index.
    pub fn add_anchor(&mut self, idx: &Index, pi: usize, fi: usize, ls: usize, le: usize, author: Author, parent: i32) -> usize {
        let mut a = Anchor::new(&idx.files[fi], ls, le);
        a.author = author;
        a.parent = if parent >= 0 && (parent as usize) < self.paths[pi].anchors.len() { parent } else { -1 };
        self.paths[pi].anchors.push(a);
        self.paths[pi].anchors.len() - 1
    }

    /// Index of the step for `symbol` in `file`, if the path already has one.
    pub fn step_for(&self, pi: usize, file: &str, symbol: &str) -> Option<usize> {
        self.paths[pi].anchors.iter().position(|a| a.file == file && a.symbol == symbol)
    }

    /// Remove a step; its children move up to its parent so the tree stays connected.
    pub fn remove_anchor(&mut self, pi: usize, ai: usize) {
        let p = &mut self.paths[pi];
        let up = p.anchors[ai].parent;
        let up = if up > ai as i32 { up - 1 } else { up }; // the parent's index after the removal shifts everything above `ai`
        p.anchors.remove(ai);
        for a in &mut p.anchors {
            if a.parent == ai as i32 {
                a.parent = up;
            } else if a.parent > ai as i32 {
                a.parent -= 1;
            }
        }
    }

    /// Pre-order walk of a path's tree: (anchor index, depth). Roots in list order, children in
    /// list order. A dangling parent counts as a root.
    pub fn tree_order(&self, pi: usize) -> Vec<(usize, usize)> {
        let anchors = &self.paths[pi].anchors;
        let n = anchors.len();
        let mut out = Vec::with_capacity(n);
        let mut seen = vec![false; n];
        fn visit(anchors: &[Anchor], i: usize, depth: usize, seen: &mut [bool], out: &mut Vec<(usize, usize)>) {
            if seen[i] {
                return; // cycle guard
            }
            seen[i] = true;
            out.push((i, depth));
            for (j, a) in anchors.iter().enumerate() {
                if a.parent == i as i32 {
                    visit(anchors, j, depth + 1, seen, out);
                }
            }
        }
        for i in 0..n {
            let par = anchors[i].parent;
            if par < 0 || par as usize >= n || par as usize == i {
                visit(anchors, i, 0, &mut seen, &mut out);
            }
        }
        for i in 0..n {
            if !seen[i] {
                visit(anchors, i, 0, &mut seen, &mut out); // orphaned cycles
            }
        }
        out
    }

    /// A path named after `root` shaped like its call tree: one step per symbol, each under the
    /// step it is called from. Re-promoting adds only symbols the path lacks.
    pub fn promote(&mut self, idx: &Index, root: SymRef, depth: usize, author: Author) -> usize {
        let pi = self.add_path(&idx.sym(root).name.clone(), Kind::Flow, author);
        let mut stack: Vec<i32> = Vec::new(); // step index at each depth
        for (r, d) in idx.call_tree(root, depth) {
            let s = idx.sym(r);
            let file = &idx.files[r.file].path;
            let parent = if d == 0 { -1 } else { stack.get(d - 1).copied().unwrap_or(-1) };
            let ai = match self.step_for(pi, file, &s.name) {
                Some(ai) => ai,
                None => self.add_anchor(idx, pi, r.file, s.start, s.end, author, parent),
            };
            stack.truncate(d);
            stack.push(ai as i32);
        }
        pi
    }

    /// This map against `base`: every path in either, in this map's order then the removed
    /// ones. Steps match by (file, symbol); with no symbol, by start line or by note. A match
    /// with different lines or text is re-pinned, one with a different note is edited.
    pub fn diff(&self, base: &Map) -> Vec<PathDiff> {
        let mut out = Vec::new();
        for p in &self.paths {
            let Some(b) = base.paths.iter().find(|b| b.name == p.name) else {
                out.push(PathDiff { name: p.name.clone(), change: Change::Added, note_changed: false, steps: vec![Some(StepChange::Added); p.anchors.len()], removed: Vec::new() });
                continue;
            };
            let mut used = vec![false; b.anchors.len()];
            let same_place = |a: &Anchor, x: &Anchor| a.file == x.file && a.symbol == x.symbol && (!a.symbol.is_empty() || a.off_start == x.off_start || (!a.note.is_empty() && a.note == x.note));
            let steps: Vec<Option<StepChange>> = p
                .anchors
                .iter()
                .map(|a| {
                    let exact = b.anchors.iter().enumerate().position(|(i, x)| !used[i] && same_place(a, x) && (x.off_start, x.off_end, x.hash) == (a.off_start, a.off_end, a.hash));
                    let Some(bi) = exact.or_else(|| b.anchors.iter().enumerate().position(|(i, x)| !used[i] && same_place(a, x))) else {
                        return Some(StepChange::Added);
                    };
                    used[bi] = true;
                    let x = &b.anchors[bi];
                    if exact.is_none() {
                        Some(StepChange::Repinned)
                    } else if x.note != a.note {
                        Some(StepChange::NoteEdited)
                    } else {
                        None
                    }
                })
                .collect();
            let removed: Vec<Anchor> = b.anchors.iter().zip(&used).filter(|(_, u)| !**u).map(|(a, _)| a.clone()).collect();
            let note_changed = p.note != b.note || p.kind != b.kind;
            let change = if note_changed || !removed.is_empty() || steps.iter().any(Option::is_some) { Change::Changed } else { Change::Same };
            out.push(PathDiff { name: p.name.clone(), change, note_changed, steps, removed });
        }
        for b in base.paths.iter().filter(|b| self.find(&b.name).is_none()) {
            out.push(PathDiff { name: b.name.clone(), change: Change::Removed, note_changed: false, steps: Vec::new(), removed: b.anchors.clone() });
        }
        out
    }

    pub fn resolve_all(&mut self, idx: &Index) {
        for p in &mut self.paths {
            for a in &mut p.anchors {
                a.resolve(idx);
            }
        }
    }
}

// ---- anchoring --------------------------------------------------------------------

pub fn slice_hash(lines: &[String], ls: usize, le: usize) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for line in &lines[ls..=le] {
        for b in line.bytes().chain(std::iter::once(b'\n')) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

impl Anchor {
    pub fn new(f: &File, ls: usize, le: usize) -> Anchor {
        let mut a = Anchor {
            file: f.path.clone(),
            symbol: String::new(),
            off_start: ls as i32,
            off_end: le as i32,
            hash: slice_hash(&f.lines, ls, le),
            author: Author::Human,
            note: String::new(),
            parent: -1,
            line_start: ls,
            line_end: le,
            stale: false,
        };
        // innermost symbol containing the slice
        if let Some(s) = f.symbols.iter().filter(|s| s.start <= ls && le <= s.end).max_by_key(|s| s.depth) {
            a.symbol = s.name.clone();
            a.off_start = (ls - s.start) as i32;
            a.off_end = (le - s.start) as i32;
        }
        a
    }

    pub fn resolve(&mut self, idx: &Index) {
        self.stale = true;
        let Some(f) = idx.find_file(&self.file).map(|i| &idx.files[i]) else { return };

        let base = if self.symbol.is_empty() {
            0
        } else {
            match f.symbols.iter().find(|s| s.name == self.symbol) {
                Some(s) => s.start as i64,
                None => return,
            }
        };
        let ls = base + self.off_start as i64;
        let le = base + self.off_end as i64;
        if ls < 0 || le >= f.lines.len() as i64 || ls > le {
            return;
        }
        self.line_start = ls as usize;
        self.line_end = le as usize;
        self.stale = slice_hash(&f.lines, self.line_start, self.line_end) != self.hash;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Symbol;

    fn sym(name: &str, start: usize, end: usize) -> Symbol {
        Symbol { name: name.into(), kind: "fn".into(), start, end, depth: 0, owner: None, calls: vec![], targets: vec![], refs: vec![], callees: vec![], callers: vec![] }
    }

    fn one_file() -> Index {
        let lines: Vec<String> = ["fn a() {", "  1", "}", "fn b() {", "  2", "}"].map(String::from).to_vec();
        let file = File { path: "a.rs".into(), hl: vec![Vec::new(); lines.len()], lines, symbols: vec![sym("a", 0, 2), sym("b", 3, 5)], imports: Default::default(), mtime: None, hash: 0, backend: crate::index::Backend::TreeSitter, pending: false };
        Index { root: ".".into(), files: vec![file] }
    }

    #[test]
    fn round_trip_and_stale() {
        let idx = one_file();
        let mut m = Map::default();
        let pi = m.add_path("p", Kind::Type, Author::Ai);
        m.add_anchor(&idx, pi, 0, 4, 4, Author::Ai, -1);
        m.paths[0].anchors[0].note = "the middle".into();
        assert_eq!(m.paths[0].anchors[0].symbol, "b");
        assert_eq!(m.paths[0].anchors[0].off_start, 1);

        let tmp = std::env::temp_dir().join("codemap_test.cmap");
        m.save(&tmp).unwrap();
        let mut loaded = Map::load(&tmp).unwrap();
        assert_eq!(loaded.paths[0].author, Author::Ai);
        assert_eq!(loaded.paths[0].kind, Kind::Type);
        assert_eq!(loaded.paths[0].anchors[0].author, Author::Ai);
        assert_eq!(loaded.paths[0].anchors[0].note, "the middle");
        assert_eq!(loaded.paths[0].anchors[0].parent, -1);

        // symbol `b` moved down two lines: anchor follows it and is not stale
        let mut idx = idx;
        idx.files[0].lines.splice(0..0, ["// x".to_string(), "// y".to_string()]);
        for s in &mut idx.files[0].symbols {
            s.start += 2;
            s.end += 2;
        }
        loaded.resolve_all(&idx);
        assert_eq!(loaded.paths[0].anchors[0].line_start, 6);
        assert!(!loaded.paths[0].anchors[0].stale);

        // text changed: stale, and no longer covers; re-pin restores both and keeps the note
        idx.files[0].lines[6] = "  3".into();
        loaded.resolve_all(&idx);
        assert!(loaded.paths[0].anchors[0].stale);
        assert!(!loaded.covers("a.rs", 6, 6));
        loaded.pin_anchor(&idx, 0, 0, 0, 6, 6, Author::Ai);
        assert!(!loaded.paths[0].anchors[0].stale);
        assert_eq!(loaded.paths[0].anchors[0].note, "the middle");
        assert!(loaded.covers("a.rs", 5, 7));
        assert!(!loaded.covers("a.rs", 0, 2));
    }

    #[test]
    fn tree_edits_keep_parents() {
        let idx = one_file();
        let mut m = Map::default();
        let pi = m.add_path("t", Kind::Flow, Author::Human);
        let root = m.add_anchor(&idx, pi, 0, 0, 2, Author::Human, -1) as i32; // a
        let mid = m.add_anchor(&idx, pi, 0, 3, 5, Author::Human, root) as i32; // b under a
        let leaf = m.add_anchor(&idx, pi, 0, 1, 1, Author::Human, mid); // line in a, under b
        assert_eq!(m.tree_order(pi), [(0, 0), (1, 1), (2, 2)]);

        m.remove_anchor(pi, mid as usize); // remove the middle: leaf moves up under the root
        assert_eq!(m.paths[pi].anchors.len(), 2);
        assert_eq!(m.paths[pi].anchors[leaf - 1].parent, root);
        assert_eq!(m.tree_order(pi), [(0, 0), (1, 1)]);
    }
}
