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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Flow = 0,
    Layer = 1,
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

    pub line_start: usize,
    pub line_end: usize,
    pub stale: bool,
    pub sym: Option<usize>,
}

pub struct PathDef {
    pub name: String,
    pub kind: Kind,
    pub note: String,
    pub author: Author,
    pub group: String,
    pub anchors: Vec<Anchor>,
}

#[derive(Debug, PartialEq)]
pub enum Row {
    Group {
        group: String,
        depth: usize,
        paths: usize,
    },
    Path {
        pi: usize,
        depth: usize,
    },
}

pub fn normal_group(g: &str) -> String {
    g.split('/')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Default)]
pub struct Map {
    pub paths: Vec<PathDef>,
    disk: std::collections::HashMap<String, String>,
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

pub struct PathDiff {
    pub name: String,
    pub change: Change,
    pub note_changed: bool,
    pub steps: Vec<Option<StepChange>>,
    pub removed: Vec<Anchor>,
}

pub const MAP_DIR: &str = ".codemap";
const VERSION: &str = "codemap 8";

const CONFLICT_MARKS: [&str; 5] = ["<<<<<<<", "=======", ">>>>>>>", "%%%%%%%", "+++++++"];

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
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

pub fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.starts_with('.')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(format!(
            "'{name}' cannot name a path: use letters, digits, '.', '_' and '-', not starting with '.'"
        ));
    }
    Ok(())
}

pub fn name_from(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches(|c| c == '-' || c == '.');
    if s.is_empty() {
        "path".into()
    } else {
        s.to_owned()
    }
}

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

pub fn parse(text: &str, origin: &str) -> Result<Vec<PathDef>, String> {
    let mut paths: Vec<PathDef> = Vec::new();
    let mut parents: Vec<Vec<Option<(String, usize)>>> = Vec::new();
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
                return Err(at(&format!(
                    "'{line}' is not '{VERSION}'; regenerate this map"
                )));
            }
            paths.push(PathDef {
                name: String::new(),
                kind: Kind::Flow,
                note: String::new(),
                author: Author::Ai,
                group: String::new(),
                anchors: Vec::new(),
            });
            parents.push(Vec::new());
            in_step = false;
            continue;
        }
        let Some(p) = paths.last_mut() else {
            return Err(at(&format!("expected '{VERSION}' first")));
        };
        let author = |v: &str| Author::parse(v).ok_or_else(|| at(&format!("unknown author '{v}'")));
        if key == "step" {
            if p.anchors.iter().any(|a| a.id == value) {
                return Err(at(&format!("a second step {value}")));
            }
            let mut a = Anchor {
                id: value,
                order: 0,
                file: String::new(),
                symbol: String::new(),
                off_start: 0,
                off_end: 0,
                hash: 0,
                author: Author::Ai,
                note: String::new(),
                parent: None,
                link: String::new(),
                line_start: 0,
                line_end: 0,
                stale: true,
                sym: None,
            };
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
                "kind" => {
                    p.kind =
                        Kind::parse(&value).ok_or_else(|| at(&format!("unknown kind '{value}'")))?
                }
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
                let [s, e] = n[..] else {
                    return Err(at("lines takes two numbers"));
                };
                (a.off_start, a.off_end) = (s, e);
            }
            "hash" => {
                a.hash =
                    u64::from_str_radix(&value, 16).map_err(|_| at("hash takes 16 hex digits"))?
            }
            "link" => a.link = value,
            "note" => a.note = value,
            _ => return Err(at(&format!("unknown field '{key}' of a step"))),
        }
    }
    for (p, ps) in paths.iter_mut().zip(parents) {
        if p.name.is_empty() {
            return Err(format!("{origin}: a path with no 'path' line"));
        }
        let mut steps: Vec<(Anchor, Option<(String, usize)>)> =
            std::mem::take(&mut p.anchors).into_iter().zip(ps).collect();
        steps.sort_by(|(a, _), (b, _)| (a.order, &a.id).cmp(&(b.order, &b.id)));
        let ps: Vec<Option<(String, usize)>>;
        (p.anchors, ps) = steps.into_iter().unzip();
        for (ai, parent) in ps.iter().enumerate() {
            if let Some((id, line)) = parent {
                let pi = p.anchors.iter().position(|a| &a.id == id).ok_or_else(|| {
                    format!(
                        "{origin}:{line}: step {} has parent {id}, which is not a step of '{}'",
                        p.anchors[ai].id, p.name
                    )
                })?;
                p.anchors[ai].parent = Some(pi);
            }
        }
    }
    Ok(paths)
}

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
    pub fn load(dir: &Path) -> Result<Map, String> {
        if dir.is_file() {
            return Err(format!(
                "{} is a map in the old single-file format; regenerate it",
                dir.display()
            ));
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Ok(Map::default());
        };
        let mut files: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "cmap"))
            .collect();
        files.sort();
        let mut m = Map::default();
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let origin = f.display().to_string().replace('\\', "/");
            let mut ps = parse(&text, &origin)?;
            match (ps.pop(), ps.is_empty()) {
                (Some(p), true) if f.file_stem().is_some_and(|s| s == p.name.as_str()) => {
                    m.disk.insert(p.name.clone(), text);
                    m.paths.push(p);
                }
                (Some(p), true) => {
                    return Err(format!(
                        "{origin}: holds the path '{}', which belongs in {}.cmap",
                        p.name, p.name
                    ));
                }
                _ => return Err(format!("{origin}: a map file holds exactly one path")),
            }
        }
        Ok(m)
    }

    pub fn save(&mut self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let mut now = std::collections::HashMap::new();
        for p in &self.paths {
            let text = p.to_text();
            if self.disk.get(&p.name) != Some(&text) {
                write_retry(&dir.join(format!("{}.cmap", p.name)), text.as_bytes())?;
            }
            now.insert(p.name.clone(), text);
        }
        for gone in self.disk.keys().filter(|name| !now.contains_key(*name)) {
            let f = dir.join(format!("{gone}.cmap"));
            if f.exists() {
                std::fs::remove_file(&f)?;
            }
        }
        self.disk = now;
        Ok(())
    }

    pub fn base_from_vcs(root: &Path) -> Result<Map, String> {
        let vcs = Vcs::detect(root).ok_or("not in a jj or git repo")?;
        let text = vcs
            .show_dir(root, vcs.parent(), MAP_DIR)
            .ok_or_else(|| format!("no {MAP_DIR} in {} ({})", vcs.parent(), vcs.name()))?;
        let paths = parse(
            &String::from_utf8_lossy(&text),
            &format!("{MAP_DIR} at {}", vcs.parent()),
        )?;
        let mut m = Map {
            paths,
            ..Default::default()
        };
        m.paths.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(m)
    }

    pub fn file_from_vcs(root: &Path, rev: &str, path: &str) -> Option<Vec<String>> {
        let out = Vcs::detect(root)?.show(root, rev, path)?;
        Some(
            String::from_utf8_lossy(&out)
                .replace('\t', "    ")
                .lines()
                .map(str::to_owned)
                .collect(),
        )
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.paths.iter().position(|p| p.name == name)
    }

    fn free_name(&self, name: &str, pi: Option<usize>) -> Result<(), String> {
        check_name(name)?;
        match self
            .paths
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name))
        {
            Some(other) if Some(other) != pi && self.paths[other].name == name => {
                Err(format!("a path named '{name}' already exists"))
            }
            Some(other) if Some(other) != pi => Err(format!(
                "'{name}' differs from the path '{}' only in letter case",
                self.paths[other].name
            )),
            _ => Ok(()),
        }
    }

    pub fn add_path(&mut self, name: &str, kind: Kind, author: Author) -> Result<usize, String> {
        if let Some(pi) = self.find(name) {
            return Ok(pi);
        }
        self.free_name(name, None)?;
        self.paths.push(PathDef {
            name: name.to_owned(),
            kind,
            note: String::new(),
            author,
            group: String::new(),
            anchors: Vec::new(),
        });
        Ok(self.paths.len() - 1)
    }

    pub fn set_group(&mut self, pi: usize, group: &str) {
        self.paths[pi].group = normal_group(group);
    }

    pub fn rename_group(&mut self, old: &str, new: &str) -> Result<usize, String> {
        let (old, new) = (normal_group(old), normal_group(new));
        if old.is_empty() {
            return Err("no group given".into());
        }
        let mut moved = 0;
        for p in &mut self.paths {
            if let Some(rest) = p
                .group
                .strip_prefix(&old)
                .filter(|r| r.is_empty() || r.starts_with('/'))
            {
                p.group = normal_group(&format!("{new}{rest}"));
                moved += 1;
            }
        }
        if moved == 0 {
            return Err(format!("no such group: {old}"));
        }
        Ok(moved)
    }

    pub fn rows(&self) -> Vec<Row> {
        fn level(m: &Map, pis: &[usize], prefix: &str, depth: usize, out: &mut Vec<Row>) {
            let mut groups: std::collections::BTreeMap<&str, Vec<usize>> = Default::default();
            let mut here = Vec::new();
            for &pi in pis {
                let g = &m.paths[pi].group;
                match g
                    .strip_prefix(prefix)
                    .map(|r| r.trim_start_matches('/'))
                    .filter(|r| !r.is_empty())
                {
                    Some(rest) => groups
                        .entry(rest.split('/').next().unwrap_or(rest))
                        .or_default()
                        .push(pi),
                    None => here.push(pi),
                }
            }
            for (name, members) in groups {
                let group = if prefix.is_empty() {
                    name.to_owned()
                } else {
                    format!("{prefix}/{name}")
                };
                out.push(Row::Group {
                    group: group.clone(),
                    depth,
                    paths: members.len(),
                });
                level(m, &members, &group, depth + 1, out);
            }
            out.extend(here.into_iter().map(|pi| Row::Path { pi, depth }));
        }
        let mut out = Vec::new();
        level(
            self,
            &(0..self.paths.len()).collect::<Vec<_>>(),
            "",
            0,
            &mut out,
        );
        out
    }

    pub fn pin_anchor(
        &mut self,
        idx: &Index,
        pi: usize,
        ai: usize,
        fi: usize,
        ls: usize,
        le: usize,
        author: Author,
    ) {
        let old = &self.paths[pi].anchors[ai];
        let mut a = Anchor::new(&idx.files[fi], ls, le, author);
        a.id = old.id.clone();
        a.order = old.order;
        a.note = old.note.clone();
        a.parent = old.parent;
        a.link = old.link.clone();
        self.paths[pi].anchors[ai] = a;
    }

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

    pub fn links_to(&self, name: &str) -> Vec<(usize, usize)> {
        self.paths
            .iter()
            .enumerate()
            .flat_map(|(pi, p)| {
                p.anchors
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| a.link == name)
                    .map(move |(ai, _)| (pi, ai))
            })
            .collect()
    }

    pub fn dangling_links(&self) -> Vec<(usize, usize)> {
        self.paths
            .iter()
            .enumerate()
            .flat_map(|(pi, p)| {
                p.anchors
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| !a.link.is_empty() && self.find(&a.link).is_none())
                    .map(move |(ai, _)| (pi, ai))
            })
            .collect()
    }

    pub fn remove_path(&mut self, pi: usize) -> Result<PathDef, String> {
        let from: Vec<String> = self
            .links_to(&self.paths[pi].name)
            .into_iter()
            .filter(|&(p, _)| p != pi)
            .map(|(p, a)| format!("{}[{a}]", self.paths[p].name))
            .collect();
        if !from.is_empty() {
            return Err(format!(
                "'{}' is linked from {}; unlink those steps first",
                self.paths[pi].name,
                from.join(", ")
            ));
        }
        Ok(self.paths.remove(pi))
    }

    pub fn covers(&self, file: &str, start: usize, end: usize) -> bool {
        self.paths
            .iter()
            .flat_map(|p| &p.anchors)
            .any(|a| !a.stale && a.file == file && a.line_start <= end && start <= a.line_end)
    }

    pub fn add_anchor(
        &mut self,
        idx: &Index,
        pi: usize,
        fi: usize,
        ls: usize,
        le: usize,
        author: Author,
        parent: Option<usize>,
    ) -> usize {
        let mut a = Anchor::new(&idx.files[fi], ls, le, author);
        let parent_id = parent
            .and_then(|p| self.paths[pi].anchors.get(p))
            .map_or("", |p| p.id.as_str());
        a.id = fresh_id(
            &format!(
                "{}\n{}\n{}\n{}\n{}\n{parent_id}",
                self.paths[pi].name, a.file, a.symbol, a.off_start, a.off_end
            ),
            &self.paths[pi].anchors,
        );
        a.order = self.paths[pi]
            .anchors
            .iter()
            .map(|x| x.order + 1)
            .max()
            .unwrap_or(0);
        a.parent = parent;
        self.paths[pi].anchors.push(a);
        self.paths[pi].anchors.len() - 1
    }

    pub fn rename(&mut self, pi: usize, new: &str) -> Result<(), String> {
        self.free_name(new, Some(pi))?;
        let old = std::mem::replace(&mut self.paths[pi].name, new.to_owned());
        for a in self
            .paths
            .iter_mut()
            .flat_map(|p| &mut p.anchors)
            .filter(|a| a.link == old)
        {
            a.link = new.to_owned();
        }
        Ok(())
    }

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

    pub fn parent_name(&self, pi: usize, ai: usize) -> String {
        match self.paths[pi].anchors[ai]
            .parent
            .and_then(|p| self.paths[pi].anchors.get(p))
        {
            Some(a) if !a.symbol.is_empty() => a.symbol.clone(),
            Some(a) => format!("{}:{}", a.file, a.line_start + 1),
            None => "top level".into(),
        }
    }

    pub fn swap_anchors(&mut self, pi: usize, a: usize, b: usize) {
        let p = &mut self.paths[pi];
        p.anchors.swap(a, b);
        let (oa, ob) = (p.anchors[b].order, p.anchors[a].order);
        (p.anchors[a].order, p.anchors[b].order) = (oa, ob);
        for x in &mut p.anchors {
            x.parent = x.parent.map(|q| {
                if q == a {
                    b
                } else if q == b {
                    a
                } else {
                    q
                }
            });
        }
    }

    pub fn remove_anchor(&mut self, pi: usize, ai: usize) {
        let p = &mut self.paths[pi];
        let up = p.anchors[ai].parent.map(|u| if u > ai { u - 1 } else { u });
        p.anchors.remove(ai);
        for a in &mut p.anchors {
            match a.parent {
                Some(q) if q == ai => a.parent = up,
                Some(q) if q > ai => a.parent = Some(q - 1),
                _ => {}
            }
        }
    }

    pub fn tree_order(&self, pi: usize) -> Vec<(usize, usize)> {
        self.tree_order_by(pi, &|_, _| 0)
    }

    pub fn tree_order_by(
        &self,
        pi: usize,
        key: &dyn Fn(usize, usize) -> usize,
    ) -> Vec<(usize, usize)> {
        let anchors = &self.paths[pi].anchors;
        let n = anchors.len();
        let mut out = Vec::with_capacity(n);
        let mut seen = vec![false; n];
        fn visit(
            anchors: &[Anchor],
            i: usize,
            depth: usize,
            key: &dyn Fn(usize, usize) -> usize,
            seen: &mut [bool],
            out: &mut Vec<(usize, usize)>,
        ) {
            if seen[i] {
                return;
            }
            seen[i] = true;
            out.push((i, depth));
            let mut kids: Vec<usize> = (0..anchors.len())
                .filter(|&j| anchors[j].parent == Some(i))
                .collect();
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
                visit(anchors, i, 0, key, &mut seen, &mut out);
            }
        }
        out
    }

    pub fn numbered(&self, idx: &Index, pi: usize) -> Vec<(usize, usize, String)> {
        let anchors = &self.paths[pi].anchors;
        let call_line = |p: usize, c: usize| -> usize {
            let (parent, child) = (&anchors[p], &anchors[c]);
            if child.symbol.is_empty() {
                return usize::MAX;
            }
            let Some(f) = idx.find_file(&parent.file).map(|fi| &idx.files[fi]) else {
                return usize::MAX;
            };
            (parent.line_start..=parent.line_end)
                .find(|&li| crate::index::call_site(f, li, &child.symbol))
                .unwrap_or(usize::MAX)
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
                (
                    ai,
                    depth,
                    counters
                        .iter()
                        .map(usize::to_string)
                        .collect::<Vec<_>>()
                        .join("."),
                )
            })
            .collect()
    }

    pub fn descendants(&self, pi: usize, ai: usize) -> usize {
        let order = self.tree_order(pi);
        let Some(pos) = order.iter().position(|&(i, _)| i == ai) else {
            return 0;
        };
        let depth = order[pos].1;
        order[pos + 1..]
            .iter()
            .take_while(|&&(_, d)| d > depth)
            .count()
    }

    pub fn promote(
        &mut self,
        idx: &Index,
        root: SymRef,
        depth: usize,
        name: Option<&str>,
        author: Author,
    ) -> Result<usize, String> {
        let pi = self.add_path(
            &name.map_or_else(|| name_from(&idx.sym(root).name), str::to_owned),
            Kind::Flow,
            author,
        )?;
        let mut stack: Vec<usize> = Vec::new();
        for (r, d) in idx.call_tree(root, depth) {
            let s = idx.sym(r);
            let file = &idx.files[r.file].path;
            let parent = if d == 0 {
                None
            } else {
                stack.get(d - 1).copied()
            };
            let ai = match self.paths[pi]
                .anchors
                .iter()
                .position(|a| a.file == *file && a.sym == Some(r.sym) && a.off_start == 0)
            {
                Some(ai) => ai,
                None => self.add_anchor(idx, pi, r.file, s.start, s.end, author, parent),
            };
            stack.truncate(d);
            stack.push(ai);
        }
        Ok(pi)
    }

    pub fn diff(&self, base: &Map) -> Vec<PathDiff> {
        let mut out = Vec::new();
        for p in &self.paths {
            let Some(b) = base.paths.iter().find(|b| b.name == p.name) else {
                out.push(PathDiff {
                    name: p.name.clone(),
                    change: Change::Added,
                    note_changed: false,
                    steps: vec![Some(StepChange::Added); p.anchors.len()],
                    removed: Vec::new(),
                });
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
                    if (
                        x.file.as_str(),
                        x.symbol.as_str(),
                        x.off_start,
                        x.off_end,
                        x.hash,
                    ) != (
                        a.file.as_str(),
                        a.symbol.as_str(),
                        a.off_start,
                        a.off_end,
                        a.hash,
                    ) {
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
            let removed: Vec<Anchor> = b
                .anchors
                .iter()
                .zip(&used)
                .filter(|(_, u)| !**u)
                .map(|(a, _)| a.clone())
                .collect();
            let note_changed = p.note != b.note || p.kind != b.kind || p.group != b.group;
            let change = if note_changed || !removed.is_empty() || steps.iter().any(Option::is_some)
            {
                Change::Changed
            } else {
                Change::Same
            };
            out.push(PathDiff {
                name: p.name.clone(),
                change,
                note_changed,
                steps,
                removed,
            });
        }
        for b in base.paths.iter().filter(|b| self.find(&b.name).is_none()) {
            out.push(PathDiff {
                name: b.name.clone(),
                change: Change::Removed,
                note_changed: false,
                steps: Vec::new(),
                removed: b.anchors.clone(),
            });
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

pub fn slice_hash(lines: &[String], ls: usize, le: usize) -> u64 {
    fnv1a(
        lines[ls..=le]
            .iter()
            .flat_map(|l| l.bytes().chain(std::iter::once(b'\n'))),
    )
}

pub fn follow(
    old: &[String],
    a: usize,
    b: usize,
    new: &[String],
) -> Option<(usize, usize, usize, Vec<Option<usize>>)> {
    let mut m = vec![None; old.len()];
    patience(old, new, 0..old.len(), 0..new.len(), (false, false), &mut m);
    let kept = m[a..=b].iter().flatten().count();
    if kept == 0 {
        return None;
    }
    let start = m[a]
        .or_else(|| m[..a].iter().rev().find_map(|x| *x).map(|j| j + 1))
        .unwrap_or(0);
    let end = m[b]
        .or_else(|| m[b + 1..].iter().find_map(|x| *x).map(|j| j - 1))
        .unwrap_or(new.len() - 1);
    Some((start, end, kept, m))
}

fn patience(
    old: &[String],
    new: &[String],
    mut o: std::ops::Range<usize>,
    mut n: std::ops::Range<usize>,
    paired: (bool, bool),
    m: &mut [Option<usize>],
) {
    while paired.0
        && o.start < o.end
        && n.start < n.end
        && old[o.start].trim() == new[n.start].trim()
    {
        m[o.start] = Some(n.start);
        (o.start, n.start) = (o.start + 1, n.start + 1);
    }
    while paired.1
        && o.start < o.end
        && n.start < n.end
        && old[o.end - 1].trim() == new[n.end - 1].trim()
    {
        (o.end, n.end) = (o.end - 1, n.end - 1);
        m[o.end] = Some(n.end);
    }
    let mut seen: std::collections::HashMap<&str, (u32, usize, u32, usize)> =
        std::collections::HashMap::new();
    for i in o.clone() {
        let e = seen.entry(old[i].trim()).or_default();
        (e.0, e.1) = (e.0 + 1, i);
    }
    for j in n.clone() {
        let e = seen.entry(new[j].trim()).or_default();
        (e.2, e.3) = (e.2 + 1, j);
    }
    let mut unique: Vec<(usize, usize)> = seen
        .into_values()
        .filter(|e| e.0 == 1 && e.2 == 1)
        .map(|e| (e.1, e.3))
        .collect();
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

fn increasing(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut tails: Vec<usize> = Vec::new();
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
        if let Some((si, s)) = f
            .symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| s.start <= ls && le <= s.end)
            .max_by_key(|(_, s)| s.depth)
        {
            a.symbol = s.name.clone();
            a.sym = Some(si);
            a.off_start = (ls - s.start) as i32;
            a.off_end = (le - s.start) as i32;
        }
        a
    }

    pub fn resolve(&mut self, idx: &Index) {
        self.stale = true;
        self.sym = None;
        let Some(f) = idx.find_file(&self.file).map(|i| &idx.files[i]) else {
            return;
        };

        let cands: Vec<Option<usize>> = if self.symbol.is_empty() {
            vec![None]
        } else {
            f.symbols
                .iter()
                .enumerate()
                .filter(|(_, s)| s.name == self.symbol)
                .map(|(i, _)| Some(i))
                .collect()
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
        Symbol {
            name: name.into(),
            kind: "fn".into(),
            start,
            end,
            depth: 0,
            owner: None,
            calls: vec![],
            targets: vec![],
            refs: vec![],
            callees: vec![],
            callers: vec![],
        }
    }

    fn one_file() -> Index {
        let lines: Vec<String> = ["fn a() {", "  1", "}", "fn b() {", "  2", "}"]
            .map(String::from)
            .to_vec();
        let file = File {
            path: "a.rs".into(),
            hl: vec![Vec::new(); lines.len()],
            lines,
            symbols: vec![sym("a", 0, 2), sym("b", 3, 5)],
            imports: Default::default(),
            mtime: None,
            hash: 0,
            backend: crate::index::Backend::TreeSitter,
            pending: false,
        };
        Index {
            root: ".".into(),
            files: vec![file],
        }
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

        let mut idx = idx;
        idx.files[0]
            .lines
            .splice(0..0, ["// x".to_string(), "// y".to_string()]);
        for s in &mut idx.files[0].symbols {
            s.start += 2;
            s.end += 2;
        }
        loaded.resolve_all(&idx);
        assert_eq!(loaded.paths[0].anchors[0].line_start, 6);
        assert!(!loaded.paths[0].anchors[0].stale);

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
        let root = m.add_anchor(&idx, pi, 0, 0, 2, Author::Human, None);
        let mid = m.add_anchor(&idx, pi, 0, 3, 5, Author::Human, Some(root));
        let leaf = m.add_anchor(&idx, pi, 0, 1, 1, Author::Human, Some(mid));
        assert_eq!(m.tree_order(pi), [(0, 0), (1, 1), (2, 2)]);
        let numbers: Vec<String> = m
            .numbered(&idx, pi)
            .into_iter()
            .map(|(_, _, n)| n)
            .collect();
        assert_eq!(numbers, ["1", "1.1", "1.1.1"]);
        assert_eq!(m.descendants(pi, 0), 2);
        assert_eq!(m.descendants(pi, 2), 0);

        m.remove_anchor(pi, mid);
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
        for (name, group) in [
            ("top", ""),
            ("a", "flows/http"),
            ("b", "areas"),
            ("c", "flows"),
            ("d", " /flows//http/ "),
        ] {
            let pi = m.add_path(name, Kind::Flow, Author::Ai).unwrap();
            m.set_group(pi, group);
        }
        assert_eq!(m.paths[4].group, "flows/http");
        let g = |group: &str, depth, paths| Row::Group {
            group: group.into(),
            depth,
            paths,
        };
        let p = |pi, depth| Row::Path { pi, depth };
        assert_eq!(
            m.rows(),
            [
                g("areas", 0, 1),
                p(2, 1),
                g("flows", 0, 3),
                g("flows/http", 1, 2),
                p(1, 2),
                p(4, 2),
                p(3, 1),
                p(0, 0)
            ]
        );

        let tmp = std::env::temp_dir().join("codemap_test_groups");
        let _ = std::fs::remove_dir_all(&tmp);
        m.save(&tmp).unwrap();
        let mut m = Map::load(&tmp).unwrap();
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
    fn saves_of_two_processes_keep_each_others_paths() {
        let idx = one_file();
        let dir = std::env::temp_dir().join("codemap_test_two_writers");
        let _ = std::fs::remove_dir_all(&dir);
        let mut m = Map::default();
        for name in ["one", "two"] {
            let pi = m.add_path(name, Kind::Flow, Author::Ai).unwrap();
            m.add_anchor(&idx, pi, 0, 0, 2, Author::Ai, None);
        }
        m.save(&dir).unwrap();

        let (mut a, mut b) = (Map::load(&dir).unwrap(), Map::load(&dir).unwrap());
        let pi = a.find("one").unwrap();
        a.paths[pi].note = "from a".into();
        a.add_path("three", Kind::Layer, Author::Ai).unwrap();
        a.save(&dir).unwrap();
        let pi = b.find("two").unwrap();
        b.paths[pi].note = "from b".into();
        b.save(&dir).unwrap();

        let m = Map::load(&dir).unwrap();
        let note = |name: &str| m.paths[m.find(name).unwrap()].note.clone();
        assert_eq!(
            (note("one"), note("two")),
            ("from a".to_owned(), "from b".to_owned())
        );
        assert!(m.find("three").is_some());

        let mut m = m;
        let pi = m.find("three").unwrap();
        m.remove_path(pi).unwrap();
        let pi = m.find("two").unwrap();
        m.rename(pi, "deux").unwrap();
        m.save(&dir).unwrap();
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["deux.cmap", "one.cmap"]);
    }

    #[test]
    fn slices_follow_the_diff() {
        let lines = |s: &[&str]| s.iter().map(|l| l.to_string()).collect::<Vec<_>>();
        let old = lines(&[
            "fn a() {",
            "    let x = 1;",
            "    let y = 2;",
            "    x + y",
            "}",
            "fn b() {",
            "    0",
            "}",
        ]);
        let new = lines(&[
            "mod m {",
            "  fn b() {",
            "      0",
            "  }",
            "}",
            "fn a() {",
            "    let x = 1;",
            "    log();",
            "    let y = 3;",
            "    x + y",
            "}",
        ]);
        let at = |a: usize, b: usize, lo: usize, hi: usize| {
            follow(&old, a, b, &new[lo..=hi]).map(|(s, e, kept, _)| (lo + s, lo + e, kept))
        };
        assert_eq!(at(5, 7, 1, 3), Some((1, 3, 3)));
        assert_eq!(at(0, 4, 5, 10), Some((5, 10, 4)));
        assert_eq!(at(1, 3, 5, 10), Some((6, 9, 2)));
        assert_eq!(at(0, 2, 5, 10), Some((5, 8, 2)));
        assert_eq!(at(2, 2, 5, 10), None);
    }
}
