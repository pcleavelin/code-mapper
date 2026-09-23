use crate::codec::{Reader, fnv1a, w_str, write_retry};
use crate::index::{File, Index, SymRef};
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
/// is reached from, None for a root (-1 on disk). `link` names another path that documents
/// what the step's lines call, `""` for none.
#[derive(Clone)]
pub struct Anchor {
    pub file: String,
    pub symbol: String,
    pub off_start: i32,
    pub off_end: i32,
    pub hash: u64,
    pub author: Author,
    pub note: String,
    pub parent: Option<usize>,
    pub link: String,

    // resolved per session, not stored
    pub line_start: usize,
    pub line_end: usize,
    pub stale: bool,
    pub sym: Option<usize>, // index of the enclosing symbol in its file, when it was found
}

/// `group` places the path in the paths list: `/` nests one group in another
/// (`flows/http`), `""` leaves the path at the top level.
pub struct PathDef {
    pub name: String,
    pub kind: Kind,
    pub note: String,
    pub author: Author,
    pub group: String,
    pub anchors: Vec<Anchor>,
}

/// One row of the paths list in group order: a group with the number of paths under it at
/// any depth, or a path. `depth` counts the groups around the row.
#[derive(Debug, PartialEq)]
pub enum Row {
    Group { group: String, depth: usize, paths: usize },
    Path { pi: usize, depth: usize },
}

/// A group written the one way the map stores it: no empty, leading or trailing parts.
pub fn normal_group(g: &str) -> String {
    g.split('/').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("/")
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
    Relinked,
}

impl StepChange {
    pub fn tag(self) -> &'static str {
        match self {
            StepChange::Added => "new",
            StepChange::Repinned => "re-pinned",
            StepChange::NoteEdited => "note edited",
            StepChange::Relinked => "link changed",
        }
    }
}

/// One path's difference against the parent revision's map.
pub struct PathDiff {
    pub name: String,
    pub change: Change,
    pub note_changed: bool,         // the path note, kind or group
    pub steps: Vec<Option<StepChange>>, // per step of the working path
    pub removed: Vec<Anchor>,       // base steps no longer present (unresolved)
}

// ---- binary file format ----------------------------------------------------------
// "CMAP" u32 version
// u32 npaths { str name, u8 kind, str note, u8 author, str group, u32 nanchors {
//     str file, str symbol, i32 off_start, i32 off_end, u64 hash, u8 author, str note, i32 parent,
//     str link } }
// str = u32 len + utf8 bytes. All little-endian. Any other version is rejected: an old map is
// regenerated, never migrated.

const MAGIC: &[u8; 4] = b"CMAP";
const VERSION: u32 = 7;

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
            w_str(&mut b, &p.group);
            b.extend_from_slice(&(p.anchors.len() as u32).to_le_bytes());
            for a in &p.anchors {
                w_str(&mut b, &a.file);
                w_str(&mut b, &a.symbol);
                b.extend_from_slice(&a.off_start.to_le_bytes());
                b.extend_from_slice(&a.off_end.to_le_bytes());
                b.extend_from_slice(&a.hash.to_le_bytes());
                b.push(a.author as u8);
                w_str(&mut b, &a.note);
                b.extend_from_slice(&a.parent.map_or(-1, |p| p as i32).to_le_bytes());
                w_str(&mut b, &a.link);
            }
        }
        write_retry(path, &b)
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

    /// The lines of `path` in revision `rev`, with tabs expanded as the index expands them so
    /// slice hashes compare. None when there is no jj repo or the file is not in `rev`.
    pub fn file_from_vcs(root: &Path, rev: &str, path: &str) -> Option<Vec<String>> {
        let out = std::process::Command::new("jj").args(["file", "show", "-r", rev, path]).current_dir(root).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).replace('\t', "    ").lines().map(str::to_owned).collect())
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
                group: r.str()?,
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
                    parent: usize::try_from(r.i32()?).ok(),
                    link: r.str()?,
                    line_start: 0,
                    line_end: 0,
                    stale: true,
                    sym: None,
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
        self.paths.push(PathDef { name: name.to_owned(), kind, note: String::new(), author, group: String::new(), anchors: Vec::new() });
        self.paths.len() - 1
    }

    /// Put path `pi` in `group`, or at the top level with `""`.
    pub fn set_group(&mut self, pi: usize, group: &str) {
        self.paths[pi].group = normal_group(group);
    }

    /// Rename group `old` to `new`, with every group nested in it, which moves their paths.
    /// Returns how many paths moved.
    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<usize, String> {
        let (old, new) = (normal_group(old), normal_group(new));
        if old.is_empty() {
            return Err("no group given".into());
        }
        let mut moved = 0;
        for p in &mut self.paths {
            if let Some(rest) = p.group.strip_prefix(&old).filter(|r| r.is_empty() || r.starts_with('/')) {
                p.group = normal_group(&format!("{new}{rest}"));
                moved += 1;
            }
        }
        if moved == 0 {
            return Err(format!("no such group: {old}"));
        }
        Ok(moved)
    }

    /// The paths list in group order. At each level the groups come first, sorted by name,
    /// then the paths of that level in map order.
    pub fn rows(&self) -> Vec<Row> {
        fn level(m: &Map, pis: &[usize], prefix: &str, depth: usize, out: &mut Vec<Row>) {
            let mut groups: std::collections::BTreeMap<&str, Vec<usize>> = Default::default();
            let mut here = Vec::new();
            for &pi in pis {
                let g = &m.paths[pi].group;
                match g.strip_prefix(prefix).map(|r| r.trim_start_matches('/')).filter(|r| !r.is_empty()) {
                    Some(rest) => groups.entry(rest.split('/').next().unwrap_or(rest)).or_default().push(pi),
                    None => here.push(pi),
                }
            }
            for (name, members) in groups {
                let group = if prefix.is_empty() { name.to_owned() } else { format!("{prefix}/{name}") };
                out.push(Row::Group { group: group.clone(), depth, paths: members.len() });
                level(m, &members, &group, depth + 1, out);
            }
            out.extend(here.into_iter().map(|pi| Row::Path { pi, depth }));
        }
        let mut out = Vec::new();
        level(self, &(0..self.paths.len()).collect::<Vec<_>>(), "", 0, &mut out);
        out
    }

    /// Re-anchor an existing step to new lines. Note, parent and link stay; the pinning author
    /// is recorded.
    pub fn pin_anchor(&mut self, idx: &Index, pi: usize, ai: usize, fi: usize, ls: usize, le: usize, author: Author) {
        let old = &self.paths[pi].anchors[ai];
        let mut a = Anchor::new(&idx.files[fi], ls, le, author);
        a.note = old.note.clone();
        a.parent = old.parent;
        a.link = old.link.clone();
        self.paths[pi].anchors[ai] = a;
    }

    /// Link step `ai` of path `pi` to the path named `target`, or unlink it with `""`. A path
    /// cannot link to itself, and the target must exist.
    pub fn set_link(&mut self, pi: usize, ai: usize, target: &str) -> Result<(), String> {
        if !target.is_empty() {
            if self.find(target).is_none() {
                return Err(format!("no such path: {target}"));
            }
            if self.paths[pi].name == target {
                return Err("a step cannot link to its own path".into());
            }
        }
        self.paths[pi].anchors[ai].link = target.to_owned();
        Ok(())
    }

    /// Every step that links to the path named `name`: (path, step), in map order.
    pub fn links_to(&self, name: &str) -> Vec<(usize, usize)> {
        self.paths.iter().enumerate().flat_map(|(pi, p)| p.anchors.iter().enumerate().filter(|(_, a)| a.link == name).map(move |(ai, _)| (pi, ai))).collect()
    }

    /// Every step whose link names a path the map does not have: (path, step).
    pub fn dangling_links(&self) -> Vec<(usize, usize)> {
        self.paths.iter().enumerate().flat_map(|(pi, p)| p.anchors.iter().enumerate().filter(|(_, a)| !a.link.is_empty() && self.find(&a.link).is_none()).map(move |(ai, _)| (pi, ai))).collect()
    }

    /// Remove a path. Refuses while another path's step links to it, and lists those steps.
    pub fn remove_path(&mut self, pi: usize) -> Result<PathDef, String> {
        let from: Vec<String> = self.links_to(&self.paths[pi].name).into_iter().filter(|&(p, _)| p != pi).map(|(p, a)| format!("{}[{a}]", self.paths[p].name)).collect();
        if !from.is_empty() {
            return Err(format!("'{}' is linked from {}; unlink those steps first", self.paths[pi].name, from.join(", ")));
        }
        Ok(self.paths.remove(pi))
    }

    /// Whether any current (non-stale) step overlaps lines `[start, end]` of `file`. Coverage is
    /// derived here and never stored.
    pub fn covers(&self, file: &str, start: usize, end: usize) -> bool {
        self.paths.iter().flat_map(|p| &p.anchors).any(|a| !a.stale && a.file == file && a.line_start <= end && start <= a.line_end)
    }

    /// Append a step under `parent`: None for a root, else an index the path has, which every
    /// caller checks before calling this. Returns the new index.
    pub fn add_anchor(&mut self, idx: &Index, pi: usize, fi: usize, ls: usize, le: usize, author: Author, parent: Option<usize>) -> usize {
        let mut a = Anchor::new(&idx.files[fi], ls, le, author);
        a.parent = parent;
        self.paths[pi].anchors.push(a);
        self.paths[pi].anchors.len() - 1
    }

    /// Rename a path, and every link to it with it.
    pub fn rename(&mut self, pi: usize, new: &str) -> Result<(), String> {
        if self.find(new).is_some_and(|other| other != pi) {
            return Err(format!("a path named '{new}' already exists"));
        }
        let old = std::mem::replace(&mut self.paths[pi].name, new.to_owned());
        for a in self.paths.iter_mut().flat_map(|p| &mut p.anchors).filter(|a| a.link == old) {
            a.link = new.to_owned();
        }
        Ok(())
    }

    /// Put step `ai` under `parent` (None = root). Refuses a parent that is the step itself or
    /// one of its descendants.
    pub fn reparent(&mut self, pi: usize, ai: usize, parent: Option<usize>) -> Result<(), String> {
        let anchors = &self.paths[pi].anchors;
        let mut p = parent;
        while let Some(q) = p {
            if q == ai {
                return Err("a step cannot go under itself or its own descendants".into());
            }
            p = anchors.get(q).ok_or("no such parent step")?.parent;
        }
        self.paths[pi].anchors[ai].parent = parent;
        Ok(())
    }

    /// Name of the step `ai` is under, for a reader coming back up the tree.
    pub fn parent_name(&self, pi: usize, ai: usize) -> String {
        match self.paths[pi].anchors[ai].parent.and_then(|p| self.paths[pi].anchors.get(p)) {
            Some(a) if !a.symbol.is_empty() => a.symbol.clone(),
            Some(a) => format!("{}:{}", a.file, a.line_start + 1),
            None => "top level".into(),
        }
    }

    /// Swap two steps in list order, which is what orders siblings that nothing else orders,
    /// keeping every parent link pointing at the same step.
    pub fn swap_anchors(&mut self, pi: usize, a: usize, b: usize) {
        let p = &mut self.paths[pi];
        p.anchors.swap(a, b);
        for x in &mut p.anchors {
            x.parent = x.parent.map(|q| if q == a { b } else if q == b { a } else { q });
        }
    }

    /// Remove a step; its children move up to its parent so the tree stays connected.
    pub fn remove_anchor(&mut self, pi: usize, ai: usize) {
        let p = &mut self.paths[pi];
        let up = p.anchors[ai].parent.map(|u| if u > ai { u - 1 } else { u }); // the parent's index after the removal shifts everything above `ai`
        p.anchors.remove(ai);
        for a in &mut p.anchors {
            match a.parent {
                Some(q) if q == ai => a.parent = up,
                Some(q) if q > ai => a.parent = Some(q - 1),
                _ => {}
            }
        }
    }

    /// Pre-order walk of a path's tree: (anchor index, depth). Roots in list order, children in
    /// list order. A dangling parent counts as a root.
    pub fn tree_order(&self, pi: usize) -> Vec<(usize, usize)> {
        self.tree_order_by(pi, &|_, _| 0)
    }

    /// Tree order with the children of each step sorted by `key(parent, child)`, list order
    /// breaking ties. The key the views use is the line of the parent's slice that names the
    /// child, so siblings read in the order the code calls them.
    pub fn tree_order_by(&self, pi: usize, key: &dyn Fn(usize, usize) -> usize) -> Vec<(usize, usize)> {
        let anchors = &self.paths[pi].anchors;
        let n = anchors.len();
        let mut out = Vec::with_capacity(n);
        let mut seen = vec![false; n];
        fn visit(anchors: &[Anchor], i: usize, depth: usize, key: &dyn Fn(usize, usize) -> usize, seen: &mut [bool], out: &mut Vec<(usize, usize)>) {
            if seen[i] {
                return; // cycle guard
            }
            seen[i] = true;
            out.push((i, depth));
            let mut kids: Vec<usize> = (0..anchors.len()).filter(|&j| anchors[j].parent == Some(i)).collect();
            kids.sort_by_key(|&j| (key(i, j), j));
            for j in kids {
                visit(anchors, j, depth + 1, key, seen, out);
            }
        }
        for i in 0..n {
            if anchors[i].parent.is_none_or(|p| p >= n || p == i) {
                visit(anchors, i, 0, key, &mut seen, &mut out);
            }
        }
        for i in 0..n {
            if !seen[i] {
                visit(anchors, i, 0, key, &mut seen, &mut out); // orphaned cycles
            }
        }
        out
    }

    /// Tree order with each step's hierarchical number: roots 1, 2, ...; the children of 1 are
    /// 1.1, 1.2, ...
    pub fn numbered(&self, idx: &Index, pi: usize) -> Vec<(usize, usize, String)> {
        let anchors = &self.paths[pi].anchors;
        // the line of the parent's slice that names the child's symbol, or last
        let call_line = |p: usize, c: usize| -> usize {
            let (parent, child) = (&anchors[p], &anchors[c]);
            if child.symbol.is_empty() {
                return usize::MAX;
            }
            let Some(f) = idx.find_file(&parent.file).map(|fi| &idx.files[fi]) else { return usize::MAX };
            (parent.line_start..=parent.line_end).find(|&li| crate::index::call_site(f, li, &child.symbol)).unwrap_or(usize::MAX)
        };
        let mut counters: Vec<usize> = Vec::new();
        self.tree_order_by(pi, &call_line)
            .into_iter()
            .map(|(ai, depth)| {
                counters.truncate(depth + 1);
                match counters.get_mut(depth) {
                    Some(c) => *c += 1,
                    None => counters.push(1),
                }
                (ai, depth, counters.iter().map(usize::to_string).collect::<Vec<_>>().join("."))
            })
            .collect()
    }

    /// How many steps sit below `ai` in the tree.
    pub fn descendants(&self, pi: usize, ai: usize) -> usize {
        let order = self.tree_order(pi);
        let Some(pos) = order.iter().position(|&(i, _)| i == ai) else { return 0 };
        let depth = order[pos].1;
        order[pos + 1..].iter().take_while(|&&(_, d)| d > depth).count()
    }

    /// A path named `name` (default: after `root`) shaped like the root's call tree: one step per
    /// symbol, each under the step it is called from. Re-promoting adds only symbols the path
    /// lacks.
    pub fn promote(&mut self, idx: &Index, root: SymRef, depth: usize, name: Option<&str>, author: Author) -> usize {
        let pi = self.add_path(name.unwrap_or(&idx.sym(root).name.clone()), Kind::Flow, author);
        let mut stack: Vec<usize> = Vec::new(); // step index at each depth
        for (r, d) in idx.call_tree(root, depth) {
            let s = idx.sym(r);
            let file = &idx.files[r.file].path;
            let parent = if d == 0 { None } else { stack.get(d - 1).copied() };
            // a symbol the path already pins whole keeps its step
            let ai = match self.paths[pi].anchors.iter().position(|a| a.file == *file && a.sym == Some(r.sym) && a.off_start == 0) {
                Some(ai) => ai,
                None => self.add_anchor(idx, pi, r.file, s.start, s.end, author, parent),
            };
            stack.truncate(d);
            stack.push(ai);
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
                    } else if x.link != a.link {
                        Some(StepChange::Relinked)
                    } else {
                        None
                    }
                })
                .collect();
            let removed: Vec<Anchor> = b.anchors.iter().zip(&used).filter(|(_, u)| !**u).map(|(a, _)| a.clone()).collect();
            let note_changed = p.note != b.note || p.kind != b.kind || p.group != b.group;
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
    fnv1a(lines[ls..=le].iter().flat_map(|l| l.bytes().chain(std::iter::once(b'\n'))))
}

/// Where lines `[a, b]` of `old` (a step's slice as it was, with a few lines of context around
/// it) sit within `new` (a candidate symbol's lines as they are): the new range, how many of the
/// slice's lines survive, and each old line's partner. None when no slice line pairs up.
///
/// A patience alignment: lines that occur once on each side, taken in an order both sides agree
/// on, pair first; pairing then grows outward from them into equal neighbouring lines and recurses
/// into the gaps between. Growth only runs from a paired line, never in from an open edge, so a
/// slice's closing brace is not paired with the last line of a longer function. Lines compare
/// with surrounding whitespace ignored, so re-indented code still pairs.
///
/// A changed edge line of the slice takes the range to just inside the nearest paired context
/// line on that side, or to that edge of `new` when none pairs: nothing before a slice pairing
/// inside the symbol means the slice began where the symbol begins.
pub fn follow(old: &[String], a: usize, b: usize, new: &[String]) -> Option<(usize, usize, usize, Vec<Option<usize>>)> {
    let mut m = vec![None; old.len()];
    patience(old, new, 0..old.len(), 0..new.len(), (false, false), &mut m);
    let kept = m[a..=b].iter().flatten().count();
    if kept == 0 {
        return None;
    }
    let start = m[a].or_else(|| m[..a].iter().rev().find_map(|x| *x).map(|j| j + 1)).unwrap_or(0);
    let end = m[b].or_else(|| m[b + 1..].iter().find_map(|x| *x).map(|j| j - 1)).unwrap_or(new.len() - 1);
    Some((start, end, kept, m))
}

/// Aligns `old[o]` with `new[n]` into `m`. `paired` says whether the line just before and the
/// line just after the gap are paired, which is what lets equal lines grow from that side.
fn patience(old: &[String], new: &[String], mut o: std::ops::Range<usize>, mut n: std::ops::Range<usize>, paired: (bool, bool), m: &mut [Option<usize>]) {
    while paired.0 && o.start < o.end && n.start < n.end && old[o.start].trim() == new[n.start].trim() {
        m[o.start] = Some(n.start);
        (o.start, n.start) = (o.start + 1, n.start + 1);
    }
    while paired.1 && o.start < o.end && n.start < n.end && old[o.end - 1].trim() == new[n.end - 1].trim() {
        (o.end, n.end) = (o.end - 1, n.end - 1);
        m[o.end] = Some(n.end);
    }
    // per line text: (count in old, last old index, count in new, last new index)
    let mut seen: std::collections::HashMap<&str, (u32, usize, u32, usize)> = std::collections::HashMap::new();
    for i in o.clone() {
        let e = seen.entry(old[i].trim()).or_default();
        (e.0, e.1) = (e.0 + 1, i);
    }
    for j in n.clone() {
        let e = seen.entry(new[j].trim()).or_default();
        (e.2, e.3) = (e.2 + 1, j);
    }
    let mut unique: Vec<(usize, usize)> = seen.into_values().filter(|e| e.0 == 1 && e.2 == 1).map(|e| (e.1, e.3)).collect();
    unique.sort_unstable();
    let run = increasing(&unique);
    if run.is_empty() {
        return;
    }
    let (mut po, mut pn, mut before) = (o.start, n.start, paired.0);
    for (i, j) in run {
        m[i] = Some(j);
        patience(old, new, po..i, pn..j, (before, true), m);
        (po, pn, before) = (i + 1, j + 1, true);
    }
    patience(old, new, po..o.end, pn..n.end, (true, paired.1), m);
}

/// The longest run of `pairs` (sorted by first element) whose second elements also increase.
fn increasing(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut tails: Vec<usize> = Vec::new(); // tails[k]: the pair ending the best run of length k + 1
    let mut prev = vec![usize::MAX; pairs.len()];
    for (k, &(_, j)) in pairs.iter().enumerate() {
        let at = tails.partition_point(|&t| pairs[t].1 < j);
        if at > 0 {
            prev[k] = tails[at - 1];
        }
        if at == tails.len() {
            tails.push(k);
        } else {
            tails[at] = k;
        }
    }
    let mut run = Vec::new();
    let mut k = tails.last().copied().unwrap_or(usize::MAX);
    while k != usize::MAX {
        run.push(pairs[k]);
        k = prev[k];
    }
    run.reverse();
    run
}

impl Anchor {
    pub fn new(f: &File, ls: usize, le: usize, author: Author) -> Anchor {
        let mut a = Anchor {
            file: f.path.clone(),
            symbol: String::new(),
            off_start: ls as i32,
            off_end: le as i32,
            hash: slice_hash(&f.lines, ls, le),
            author,
            note: String::new(),
            parent: None,
            link: String::new(),
            line_start: ls,
            line_end: le,
            stale: false,
            sym: None,
        };
        // innermost symbol containing the slice
        if let Some((si, s)) = f.symbols.iter().enumerate().filter(|(_, s)| s.start <= ls && le <= s.end).max_by_key(|(_, s)| s.depth) {
            a.symbol = s.name.clone();
            a.sym = Some(si);
            a.off_start = (ls - s.start) as i32;
            a.off_end = (le - s.start) as i32;
        }
        a
    }

    /// Several symbols in one file can share a name (`Author::tag`, `Kind::tag`): the one whose
    /// text still hashes right wins, else the first, so a re-pin is only needed when the text
    /// itself changed.
    pub fn resolve(&mut self, idx: &Index) {
        self.stale = true;
        self.sym = None;
        let Some(f) = idx.find_file(&self.file).map(|i| &idx.files[i]) else { return };

        let cands: Vec<Option<usize>> = if self.symbol.is_empty() {
            vec![None]
        } else {
            f.symbols.iter().enumerate().filter(|(_, s)| s.name == self.symbol).map(|(i, _)| Some(i)).collect()
        };
        let mut best: Option<(Option<usize>, usize, usize, bool)> = None;
        for si in cands {
            let base = si.map_or(0, |i| f.symbols[i].start as i64);
            let ls = base + self.off_start as i64;
            let le = base + self.off_end as i64;
            if ls < 0 || le >= f.lines.len() as i64 || ls > le {
                continue;
            }
            let (ls, le) = (ls as usize, le as usize);
            let current = slice_hash(&f.lines, ls, le) == self.hash;
            if best.is_none() || current {
                best = Some((si, ls, le, current));
            }
            if current {
                break;
            }
        }
        if let Some((si, ls, le, current)) = best {
            self.sym = si;
            self.line_start = ls;
            self.line_end = le;
            self.stale = !current;
        }
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
        m.add_anchor(&idx, pi, 0, 4, 4, Author::Ai, None);
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
        assert_eq!(loaded.paths[0].anchors[0].parent, None);

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
        let root = m.add_anchor(&idx, pi, 0, 0, 2, Author::Human, None); // a
        let mid = m.add_anchor(&idx, pi, 0, 3, 5, Author::Human, Some(root)); // b under a
        let leaf = m.add_anchor(&idx, pi, 0, 1, 1, Author::Human, Some(mid)); // line in a, under b
        assert_eq!(m.tree_order(pi), [(0, 0), (1, 1), (2, 2)]);
        let numbers: Vec<String> = m.numbered(&idx, pi).into_iter().map(|(_, _, n)| n).collect();
        assert_eq!(numbers, ["1", "1.1", "1.1.1"]);
        assert_eq!(m.descendants(pi, 0), 2);
        assert_eq!(m.descendants(pi, 2), 0);

        m.remove_anchor(pi, mid); // remove the middle: leaf moves up under the root
        assert_eq!(m.paths[pi].anchors.len(), 2);
        assert_eq!(m.paths[pi].anchors[leaf - 1].parent, Some(root));
        assert_eq!(m.tree_order(pi), [(0, 0), (1, 1)]);
    }

    #[test]
    fn links_follow_renames_and_hold_removal() {
        let idx = one_file();
        let mut m = Map::default();
        let flow = m.add_path("flow", Kind::Flow, Author::Ai);
        let shared = m.add_path("shared", Kind::Layer, Author::Ai);
        m.add_anchor(&idx, flow, 0, 0, 2, Author::Ai, None);
        m.add_anchor(&idx, shared, 0, 3, 5, Author::Ai, None);
        assert!(m.set_link(flow, 0, "nope").is_err());
        assert!(m.set_link(flow, 0, "flow").is_err());
        m.set_link(flow, 0, "shared").unwrap();
        assert_eq!(m.links_to("shared"), [(flow, 0)]);

        let tmp = std::env::temp_dir().join("codemap_test_links.cmap");
        m.save(&tmp).unwrap();
        let mut m = Map::load(&tmp).unwrap();
        assert_eq!(m.paths[flow].anchors[0].link, "shared");

        m.rename(shared, "common").unwrap();
        assert_eq!(m.paths[flow].anchors[0].link, "common");
        assert!(m.remove_path(shared).is_err());
        m.pin_anchor(&idx, flow, 0, 0, 1, 1, Author::Ai);
        assert_eq!(m.paths[flow].anchors[0].link, "common");

        m.paths[flow].anchors[0].link = "gone".into();
        assert_eq!(m.dangling_links(), [(flow, 0)]);
        m.set_link(flow, 0, "").unwrap();
        assert!(m.remove_path(shared).is_ok());
    }

    #[test]
    fn groups_nest_and_rename() {
        let mut m = Map::default();
        for (name, group) in [("top", ""), ("a", "flows/http"), ("b", "areas"), ("c", "flows"), ("d", " /flows//http/ ")] {
            let pi = m.add_path(name, Kind::Flow, Author::Ai);
            m.set_group(pi, group);
        }
        assert_eq!(m.paths[4].group, "flows/http");
        let g = |group: &str, depth, paths| Row::Group { group: group.into(), depth, paths };
        let p = |pi, depth| Row::Path { pi, depth };
        assert_eq!(m.rows(), [g("areas", 0, 1), p(2, 1), g("flows", 0, 3), g("flows/http", 1, 2), p(1, 2), p(4, 2), p(3, 1), p(0, 0)]);

        let tmp = std::env::temp_dir().join("codemap_test_groups.cmap");
        m.save(&tmp).unwrap();
        let mut m = Map::load(&tmp).unwrap();
        assert_eq!(m.paths[1].group, "flows/http");

        assert_eq!(m.rename_group("flows", "work/flows"), Ok(3));
        assert_eq!(m.paths[1].group, "work/flows/http");
        assert_eq!(m.paths[3].group, "work/flows");
        assert!(m.rename_group("flow", "x").is_err());
        assert_eq!(m.rename_group("work", ""), Ok(3));
        assert_eq!(m.paths[3].group, "flows");
    }

    #[test]
    fn slices_follow_the_diff() {
        let lines = |s: &[&str]| s.iter().map(|l| l.to_string()).collect::<Vec<_>>();
        let old = lines(&["fn a() {", "    let x = 1;", "    let y = 2;", "    x + y", "}", "fn b() {", "    0", "}"]);
        // b moved above a and re-indented; a gained a line and changed one
        let new = lines(&["mod m {", "  fn b() {", "      0", "  }", "}", "fn a() {", "    let x = 1;", "    log();", "    let y = 3;", "    x + y", "}"]);
        let at = |a: usize, b: usize, lo: usize, hi: usize| follow(&old, a, b, &new[lo..=hi]).map(|(s, e, kept, _)| (lo + s, lo + e, kept));
        assert_eq!(at(5, 7, 1, 3), Some((1, 3, 3))); // b whole: moved and re-indented
        assert_eq!(at(0, 4, 5, 10), Some((5, 10, 4))); // a whole: 4 of 5 lines survive
        assert_eq!(at(1, 3, 5, 10), Some((6, 9, 2))); // part of a: the closing brace stays out
        assert_eq!(at(0, 2, 5, 10), Some((5, 8, 2))); // a changed last line ends before `x + y`
        assert_eq!(at(2, 2, 5, 10), None); // the changed line alone has nothing to follow
    }
}
