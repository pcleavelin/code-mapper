use crate::codec::{fnv1a, write_retry};
use crate::index::{File, Index, SymRef};
use crate::vcs::Vcs;
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
    /// The word the map file writes.
    pub fn name(self) -> &'static str {
        match self {
            Author::Human => "human",
            Author::Ai => "ai",
        }
    }
    pub fn parse(s: &str) -> Option<Author> {
        match s {
            "human" => Some(Author::Human),
            "ai" => Some(Author::Ai),
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
}

/// Pins a slice of lines. Offsets are relative to the start of the enclosing symbol so the anchor
/// survives edits elsewhere in the file. `symbol == ""` means absolute lines. `hash` detects when
/// the anchored text itself changed (-> stale). `parent` makes a path a tree: the step this one
/// is reached from, None for a root; on disk it is the parent's `id`, which stays the same for
/// the life of the step. `link` names another path that documents what the step's lines call,
/// `""` for none. `order` places the step in the step list, which orders siblings the code
/// does not: it is set once, one past the highest in the path, and only a swap changes it.
#[derive(Clone)]
pub struct Anchor {
    pub id: String,
    pub order: u32,
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

// ---- file format -------------------------------------------------------------------
// `.codemap/` at the root holds one text file per path, `<name>.cmap`, so two branches that
// change different paths never touch the same file, and two that change one path merge line
// by line. One field per line, `key value`, the value running to the end of the line with
// `\` escaping a backslash, a newline (`\n`) and a carriage return (`\r`). A blank line ends
// the path's block and each step's. Optional fields are left out when empty; nothing counts
// anything. A step names its parent by id and holds its place in the step list in `order`, so
// adding a step changes no other. Steps are written in id order, and ids are random, so steps
// that two branches add to one path land apart in the file and merge without a conflict.
//
//   codemap 8
//   path <name>
//   kind flow | layer | type
//   author ai | human
//   group <group>          optional
//   note <text>            optional
//
//   step <id>
//   order <n>              the place in the step list; ties go by id
//   parent <id>            optional: a root has none
//   author ai | human
//   file <path>
//   symbol <name>          optional: none means absolute lines
//   lines <start> <end>    from the symbol's first line, or absolute
//   hash <16 hex digits>
//   link <path name>       optional
//   note <text>            optional
//
// Any other version is rejected: an old map is regenerated, never migrated.

pub const MAP_DIR: &str = ".codemap";
const VERSION: &str = "codemap 8";

/// Lines jj and git write into a file with a conflict in it.
const CONFLICT_MARKS: [&str; 5] = ["<<<<<<<", "=======", ">>>>>>>", "%%%%%%%", "+++++++"];

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n").replace('\r', "\\r")
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (c, c == '\\') {
            (_, true) => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some(c) => out.push(c),
                None => out.push('\\'),
            },
            (c, false) => out.push(c),
        }
    }
    out
}

/// A path name as the map stores it: it is also the file name, so letters, digits, `.`, `_`
/// and `-` only, not starting with a dot.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.starts_with('.') || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return Err(format!("'{name}' cannot name a path: use letters, digits, '.', '_' and '-', not starting with '.'"));
    }
    Ok(())
}

/// `name` with every character a path name cannot hold turned into `-`, for names taken from
/// symbols.
pub fn name_from(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' }).collect();
    let s = s.trim_matches(|c| c == '-' || c == '.');
    if s.is_empty() { "path".into() } else { s.to_owned() }
}

/// A new step id: six base36 digits of a hash of `seed`, which says what the step is, and a
/// counter that moves past ids the path already holds. The digits spread new steps across the
/// file, and two branches that add the same step add the same id.
fn fresh_id(seed: &str, taken: &[Anchor]) -> String {
    (0u32..)
        .map(|n| {
            let mut h = fnv1a(seed.bytes().chain(n.to_le_bytes()));
            (0..6)
                .map(|_| {
                    let d = (h % 36) as u32;
                    h /= 36;
                    char::from_digit(d, 36).unwrap_or('0')
                })
                .collect::<String>()
        })
        .find(|id| !taken.iter().any(|a| &a.id == id))
        .unwrap_or_default()
}

impl PathDef {
    /// The path's file, written the same way every time so saving an unchanged path changes
    /// no byte.
    pub fn to_text(&self) -> String {
        let mut t = String::new();
        let mut field = |k: &str, v: &str| {
            t.push_str(k);
            t.push(' ');
            t.push_str(&escape(v));
            t.push('\n');
        };
        field("codemap", "8");
        field("path", &self.name);
        field("kind", self.kind.name());
        field("author", self.author.name());
        if !self.group.is_empty() {
            field("group", &self.group);
        }
        if !self.note.is_empty() {
            field("note", &self.note);
        }
        let mut by_id: Vec<&Anchor> = self.anchors.iter().collect();
        by_id.sort_by(|a, b| a.id.cmp(&b.id));
        for a in by_id {
            t.push('\n');
            let mut field = |k: &str, v: &str| {
                t.push_str(k);
                t.push(' ');
                t.push_str(&escape(v));
                t.push('\n');
            };
            field("step", &a.id);
            field("order", &a.order.to_string());
            if let Some(p) = a.parent.and_then(|p| self.anchors.get(p)) {
                field("parent", &p.id);
            }
            field("author", a.author.name());
            field("file", &a.file);
            if !a.symbol.is_empty() {
                field("symbol", &a.symbol);
            }
            field("lines", &format!("{} {}", a.off_start, a.off_end));
            field("hash", &format!("{:016x}", a.hash));
            if !a.link.is_empty() {
                field("link", &a.link);
            }
            if !a.note.is_empty() {
                field("note", &a.note);
            }
        }
        t
    }
}

/// Every path in `text`: one path's file, or several files read one after another (what jj
/// and git print for a directory at a revision). An error names `origin` and the line.
pub fn parse(text: &str, origin: &str) -> Result<Vec<PathDef>, String> {
    let mut paths: Vec<PathDef> = Vec::new();
    let mut parents: Vec<Vec<Option<(String, usize)>>> = Vec::new(); // per path, per step: (parent id, line)
    let mut in_step = false;
    for (i, line) in text.lines().enumerate() {
        let at = |msg: &str| format!("{origin}:{}: {msg}", i + 1);
        if CONFLICT_MARKS.iter().any(|m| line.starts_with(m)) {
            return Err(at("an unresolved merge conflict"));
        }
        if line.is_empty() {
            continue;
        }
        let (key, raw) = line.split_once(' ').unwrap_or((line, ""));
        let value = unescape(raw);
        if key == "codemap" {
            if line != VERSION {
                return Err(at(&format!("'{line}' is not '{VERSION}'; regenerate this map")));
            }
            paths.push(PathDef { name: String::new(), kind: Kind::Flow, note: String::new(), author: Author::Ai, group: String::new(), anchors: Vec::new() });
            parents.push(Vec::new());
            in_step = false;
            continue;
        }
        let Some(p) = paths.last_mut() else { return Err(at(&format!("expected '{VERSION}' first"))) };
        let author = |v: &str| Author::parse(v).ok_or_else(|| at(&format!("unknown author '{v}'")));
        if key == "step" {
            if p.anchors.iter().any(|a| a.id == value) {
                return Err(at(&format!("a second step {value}")));
            }
            let mut a = Anchor { id: value, order: 0, file: String::new(), symbol: String::new(), off_start: 0, off_end: 0, hash: 0, author: Author::Ai, note: String::new(), parent: None, link: String::new(), line_start: 0, line_end: 0, stale: true, sym: None };
            a.author = p.author;
            p.anchors.push(a);
            parents.last_mut().unwrap().push(None);
            in_step = true;
            continue;
        }
        if !in_step {
            match key {
                "path" => {
                    check_name(&value).map_err(|e| at(&e))?;
                    p.name = value;
                }
                "kind" => p.kind = Kind::parse(&value).ok_or_else(|| at(&format!("unknown kind '{value}'")))?,
                "author" => p.author = author(&value)?,
                "group" => p.group = normal_group(&value),
                "note" => p.note = value,
                _ => return Err(at(&format!("unknown field '{key}' of a path"))),
            }
            continue;
        }
        let a = p.anchors.last_mut().unwrap();
        match key {
            "parent" => *parents.last_mut().unwrap().last_mut().unwrap() = Some((value, i + 1)),
            "order" => a.order = value.parse().map_err(|_| at("order takes a number"))?,
            "author" => a.author = author(&value)?,
            "file" => a.file = value,
            "symbol" => a.symbol = value,
            "lines" => {
                let n: Vec<i32> = value.split(' ').filter_map(|v| v.parse().ok()).collect();
                let [s, e] = n[..] else { return Err(at("lines takes two numbers")) };
                (a.off_start, a.off_end) = (s, e);
            }
            "hash" => a.hash = u64::from_str_radix(&value, 16).map_err(|_| at("hash takes 16 hex digits"))?,
            "link" => a.link = value,
            "note" => a.note = value,
            _ => return Err(at(&format!("unknown field '{key}' of a step"))),
        }
    }
    for (p, ps) in paths.iter_mut().zip(parents) {
        if p.name.is_empty() {
            return Err(format!("{origin}: a path with no 'path' line"));
        }
        // the step list in order, each step's parent id travelling with it
        let mut steps: Vec<(Anchor, Option<(String, usize)>)> = std::mem::take(&mut p.anchors).into_iter().zip(ps).collect();
        steps.sort_by(|(a, _), (b, _)| (a.order, &a.id).cmp(&(b.order, &b.id)));
        let ps: Vec<Option<(String, usize)>>;
        (p.anchors, ps) = steps.into_iter().unzip();
        for (ai, parent) in ps.iter().enumerate() {
            if let Some((id, line)) = parent {
                let pi = p.anchors.iter().position(|a| &a.id == id).ok_or_else(|| format!("{origin}:{line}: step {} has parent {id}, which is not a step of '{}'", p.anchors[ai].id, p.name))?;
                p.anchors[ai].parent = Some(pi);
            }
        }
    }
    Ok(paths)
}

/// The newest change time of the map's files and how many there are: what the GUI polls to
/// know the map changed on disk.
pub fn stamp(dir: &Path) -> Option<(std::time::SystemTime, usize)> {
    let mut newest = std::fs::metadata(dir).ok()?.modified().ok()?;
    let mut n = 0;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        if e.path().extension().is_some_and(|x| x == "cmap") {
            n += 1;
            if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                newest = newest.max(t);
            }
        }
    }
    Some((newest, n))
}

impl Map {
    /// The map in `dir`, or an empty one when there is no such directory. Paths come in file
    /// name order. Every file that does not read is an error, and so is a map in the old
    /// single-file format.
    pub fn load(dir: &Path) -> Result<Map, String> {
        if dir.is_file() {
            return Err(format!("{} is a map in the old single-file format; regenerate it", dir.display()));
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return Ok(Map::default()) };
        let mut files: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "cmap")).collect();
        files.sort();
        let mut m = Map::default();
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let origin = f.display().to_string().replace('\\', "/");
            let mut ps = parse(&text, &origin)?;
            match (ps.pop(), ps.is_empty()) {
                (Some(p), true) if f.file_stem().is_some_and(|s| s == p.name.as_str()) => m.paths.push(p),
                (Some(p), true) => return Err(format!("{origin}: holds the path '{}', which belongs in {}.cmap", p.name, p.name)),
                _ => return Err(format!("{origin}: a map file holds exactly one path")),
            }
        }
        Ok(m)
    }

    /// Write every path to its file in `dir` and remove the files of paths the map no longer
    /// has. A file whose text is already what it would be is not written.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut keep = std::collections::HashSet::new();
        for p in &self.paths {
            let file = dir.join(format!("{}.cmap", p.name));
            let text = p.to_text();
            if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
                write_retry(&file, text.as_bytes())?;
            }
            keep.insert(file);
        }
        for e in std::fs::read_dir(dir)?.flatten() {
            let f = e.path();
            if f.extension().is_some_and(|x| x == "cmap") && !keep.contains(&f) {
                std::fs::remove_file(&f)?;
            }
        }
        Ok(())
    }

    /// The map as committed in the parent revision. Err says why there is none: no repo, no
    /// committed map, or a map that does not read.
    pub fn base_from_vcs(root: &Path) -> Result<Map, String> {
        let vcs = Vcs::detect(root).ok_or("not in a jj or git repo")?;
        let text = vcs.show_dir(root, vcs.parent(), MAP_DIR).ok_or_else(|| format!("no {MAP_DIR} in {} ({})", vcs.parent(), vcs.name()))?;
        let paths = parse(&String::from_utf8_lossy(&text), &format!("{MAP_DIR} at {}", vcs.parent()))?;
        let mut m = Map { paths };
        m.paths.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(m)
    }

    /// The lines of `path` in revision `rev`, with tabs expanded as the index expands them so
    /// slice hashes compare. None when there is no repo or the file is not in `rev`.
    pub fn file_from_vcs(root: &Path, rev: &str, path: &str) -> Option<Vec<String>> {
        let out = Vcs::detect(root)?.show(root, rev, path)?;
        Some(String::from_utf8_lossy(&out).replace('\t', "    ").lines().map(str::to_owned).collect())
    }

    // ---- mutations shared by the GUI and the CLI ----

    pub fn find(&self, name: &str) -> Option<usize> {
        self.paths.iter().position(|p| p.name == name)
    }

    /// A name a new or renamed path can take: one `check_name` accepts, and not the name of
    /// another path in other letter case, since the two files would be one on a
    /// case-insensitive file system.
    fn free_name(&self, name: &str, pi: Option<usize>) -> Result<(), String> {
        check_name(name)?;
        match self.paths.iter().position(|p| p.name.eq_ignore_ascii_case(name)) {
            Some(other) if Some(other) != pi && self.paths[other].name == name => Err(format!("a path named '{name}' already exists")),
            Some(other) if Some(other) != pi => Err(format!("'{name}' differs from the path '{}' only in letter case", self.paths[other].name)),
            _ => Ok(()),
        }
    }

    /// Returns the existing path of that name, or creates it.
    pub fn add_path(&mut self, name: &str, kind: Kind, author: Author) -> Result<usize, String> {
        if let Some(pi) = self.find(name) {
            return Ok(pi);
        }
        self.free_name(name, None)?;
        self.paths.push(PathDef { name: name.to_owned(), kind, note: String::new(), author, group: String::new(), anchors: Vec::new() });
        Ok(self.paths.len() - 1)
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
        a.id = old.id.clone();
        a.order = old.order;
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
        let parent_id = parent.and_then(|p| self.paths[pi].anchors.get(p)).map_or("", |p| p.id.as_str());
        a.id = fresh_id(&format!("{}\n{}\n{}\n{}\n{}\n{parent_id}", self.paths[pi].name, a.file, a.symbol, a.off_start, a.off_end), &self.paths[pi].anchors);
        a.order = self.paths[pi].anchors.iter().map(|x| x.order + 1).max().unwrap_or(0);
        a.parent = parent;
        self.paths[pi].anchors.push(a);
        self.paths[pi].anchors.len() - 1
    }

    /// Rename a path, and every link to it with it.
    pub fn rename(&mut self, pi: usize, new: &str) -> Result<(), String> {
        self.free_name(new, Some(pi))?;
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
        // the places stay where they were, so the two steps trade them
        let (oa, ob) = (p.anchors[b].order, p.anchors[a].order);
        (p.anchors[a].order, p.anchors[b].order) = (oa, ob);
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
    pub fn promote(&mut self, idx: &Index, root: SymRef, depth: usize, name: Option<&str>, author: Author) -> Result<usize, String> {
        let pi = self.add_path(&name.map_or_else(|| name_from(&idx.sym(root).name), str::to_owned), Kind::Flow, author)?;
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
        Ok(pi)
    }

    /// This map against `base`: every path in either, in this map's order then the removed
    /// ones. Steps match by id. A match with different lines or text is re-pinned, one with a
    /// different note is edited, one with a different link is relinked.
    pub fn diff(&self, base: &Map) -> Vec<PathDiff> {
        let mut out = Vec::new();
        for p in &self.paths {
            let Some(b) = base.paths.iter().find(|b| b.name == p.name) else {
                out.push(PathDiff { name: p.name.clone(), change: Change::Added, note_changed: false, steps: vec![Some(StepChange::Added); p.anchors.len()], removed: Vec::new() });
                continue;
            };
            let mut used = vec![false; b.anchors.len()];
            let steps: Vec<Option<StepChange>> = p
                .anchors
                .iter()
                .map(|a| {
                    let Some(bi) = b.anchors.iter().position(|x| x.id == a.id) else {
                        return Some(StepChange::Added);
                    };
                    used[bi] = true;
                    let x = &b.anchors[bi];
                    if (x.file.as_str(), x.symbol.as_str(), x.off_start, x.off_end, x.hash) != (a.file.as_str(), a.symbol.as_str(), a.off_start, a.off_end, a.hash) {
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
            id: String::new(),
            order: 0,
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
        let pi = m.add_path("p", Kind::Type, Author::Ai).unwrap();
        m.add_anchor(&idx, pi, 0, 4, 4, Author::Ai, None);
        m.paths[0].anchors[0].note = "the middle".into();
        assert_eq!(m.paths[0].anchors[0].symbol, "b");
        assert_eq!(m.paths[0].anchors[0].off_start, 1);

        let tmp = std::env::temp_dir().join("codemap_test_round_trip");
        let _ = std::fs::remove_dir_all(&tmp);
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
        let pi = m.add_path("t", Kind::Flow, Author::Human).unwrap();
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
        let flow = m.add_path("flow", Kind::Flow, Author::Ai).unwrap();
        let shared = m.add_path("shared", Kind::Layer, Author::Ai).unwrap();
        m.add_anchor(&idx, flow, 0, 0, 2, Author::Ai, None);
        m.add_anchor(&idx, shared, 0, 3, 5, Author::Ai, None);
        assert!(m.set_link(flow, 0, "nope").is_err());
        assert!(m.set_link(flow, 0, "flow").is_err());
        m.set_link(flow, 0, "shared").unwrap();
        assert_eq!(m.links_to("shared"), [(flow, 0)]);

        let tmp = std::env::temp_dir().join("codemap_test_links");
        let _ = std::fs::remove_dir_all(&tmp);
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
            let pi = m.add_path(name, Kind::Flow, Author::Ai).unwrap();
            m.set_group(pi, group);
        }
        assert_eq!(m.paths[4].group, "flows/http");
        let g = |group: &str, depth, paths| Row::Group { group: group.into(), depth, paths };
        let p = |pi, depth| Row::Path { pi, depth };
        assert_eq!(m.rows(), [g("areas", 0, 1), p(2, 1), g("flows", 0, 3), g("flows/http", 1, 2), p(1, 2), p(4, 2), p(3, 1), p(0, 0)]);

        let tmp = std::env::temp_dir().join("codemap_test_groups");
        let _ = std::fs::remove_dir_all(&tmp);
        m.save(&tmp).unwrap();
        let mut m = Map::load(&tmp).unwrap();
        // a map reads back in file name order
        let group = |m: &Map, name: &str| m.paths[m.find(name).unwrap()].group.clone();
        assert_eq!(group(&m, "a"), "flows/http");

        assert_eq!(m.rename_group("flows", "work/flows"), Ok(3));
        assert_eq!(group(&m, "a"), "work/flows/http");
        assert_eq!(group(&m, "c"), "work/flows");
        assert!(m.rename_group("flow", "x").is_err());
        assert_eq!(m.rename_group("work", ""), Ok(3));
        assert_eq!(group(&m, "c"), "flows");
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
