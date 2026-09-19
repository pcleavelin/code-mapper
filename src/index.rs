use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SymRef {
    pub file: usize,
    pub sym: usize,
}

/// How a call names its target. `a.b.c()` and `A::c()` both give name `c` with qualifier `b`
/// or `A`; `self.c()`, `Self::c()`, `this.c()` are `SelfRef`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Qual {
    None,
    SelfRef,
    Some(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub name: String,
    pub qual: Qual,
}

pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub start: usize, // inclusive 0-based line rows
    pub end: usize,
    pub depth: u8,
    pub owner: Option<String>, // the type / class a member belongs to (impl Foo -> "Foo")
    pub calls: Vec<Call>,      // raw call sites found in the body
    pub callees: Vec<SymRef>,
    pub callers: Vec<SymRef>,
}

/// Syntax colour span within one line: byte start, byte end, class (see `HL_*`).
pub type Span = (u32, u32, u8);

pub const HL_PLAIN: u8 = 0;
pub const HL_KEYWORD: u8 = 1;
pub const HL_STRING: u8 = 2;
pub const HL_COMMENT: u8 = 3;
pub const HL_FUNCTION: u8 = 4;
pub const HL_TYPE: u8 = 5;
pub const HL_CONSTANT: u8 = 6;
pub const HL_PROPERTY: u8 = 7;

pub struct File {
    pub path: String, // relative to root, forward slashes
    pub lines: Vec<String>,
    pub hl: Vec<Vec<Span>>, // per line, sorted, non-overlapping
    pub symbols: Vec<Symbol>,
    pub imports: HashMap<String, String>, // imported name or alias -> module stem it comes from
    pub mtime: Option<SystemTime>,
}

impl File {
    /// `src/core/file_buffer.odin` -> `file_buffer`
    pub fn stem(&self) -> &str {
        let base = self.path.rsplit('/').next().unwrap_or(&self.path);
        base.split('.').next().unwrap_or(base)
    }
    /// `src/core/file_buffer.odin` -> `core` (the package / module directory)
    pub fn dir(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => self.path[..i].rsplit('/').next().unwrap_or(""),
            None => "",
        }
    }
}

pub struct Index {
    pub root: PathBuf,
    pub files: Vec<File>,
}

const MAX_FILE: usize = 4 << 20;
const CONTAINERS: [&str; 4] = ["impl", "mod", "trait", "class"];

fn language_for(ext: &str) -> Option<(Language, &'static str)> {
    Some(match ext {
        "rs" => (tree_sitter_rust::LANGUAGE.into(), tree_sitter_rust::HIGHLIGHTS_QUERY),
        "odin" => (tree_sitter_odin::LANGUAGE.into(), tree_sitter_odin::HIGHLIGHTS_QUERY),
        "c" | "h" => (tree_sitter_c::LANGUAGE.into(), tree_sitter_c::HIGHLIGHT_QUERY),
        "py" => (tree_sitter_python::LANGUAGE.into(), tree_sitter_python::HIGHLIGHTS_QUERY),
        "js" | "mjs" | "cjs" => (tree_sitter_javascript::LANGUAGE.into(), tree_sitter_javascript::HIGHLIGHT_QUERY),
        "ts" => (tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), tree_sitter_typescript::HIGHLIGHTS_QUERY),
        "tsx" => (tree_sitter_typescript::LANGUAGE_TSX.into(), tree_sitter_typescript::HIGHLIGHTS_QUERY),
        _ => return None,
    })
}

/// Parser plus compiled highlight queries, one per extension, reused across files.
pub struct Parsers {
    parser: Parser,
    queries: HashMap<&'static str, Option<(Language, Query)>>,
}

impl Parsers {
    pub fn new() -> Parsers {
        Parsers { parser: Parser::new(), queries: HashMap::new() }
    }

    /// A free function over the map (not `&mut self`) so the caller can still use `parser`.
    fn query_for<'a>(queries: &'a mut HashMap<&'static str, Option<(Language, Query)>>, ext: &str) -> Option<&'a (Language, Query)> {
        let key: &'static str = match ext {
            "rs" => "rs",
            "odin" => "odin",
            "c" | "h" => "c",
            "py" => "py",
            "js" | "mjs" | "cjs" => "js",
            "ts" => "ts",
            "tsx" => "tsx",
            _ => return None,
        };
        queries
            .entry(key)
            .or_insert_with(|| {
                let (lang, q) = language_for(key)?;
                // ponytail: a grammar whose bundled query fails to compile just gets no colours
                let query = Query::new(&lang, q).ok()?;
                Some((lang, query))
            })
            .as_ref()
    }
}

// ponytail: reads the whole tree into memory up front on the main thread; move to a background
// thread with a progress bar when startup on a big repo becomes annoying.
pub fn build(root: &Path) -> Index {
    let mut parsers = Parsers::new();
    let mut files = Vec::new();

    // require_git(false): honour .gitignore even when the root is not a git repo (target/ etc.)
    for entry in ignore::WalkBuilder::new(root).require_git(false).build().flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(raw) = std::fs::read(entry.path()) else { continue };
        if raw.len() > MAX_FILE || raw[..raw.len().min(1024)].contains(&0) {
            continue; // too big or binary
        }

        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        // ponytail: tabs become 4 spaces once, for display and hashing alike
        let text = String::from_utf8_lossy(&raw).replace('\t', "    ");
        let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
        let mut f = parse_file(&mut parsers, rel, &text, ext);
        f.mtime = mtime;
        files.push(f);
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut idx = Index { root: root.to_path_buf(), files };
    idx.link();
    idx
}

pub fn parse_file(parsers: &mut Parsers, path: String, text: &str, ext: &str) -> File {
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut symbols = Vec::new();
    let mut hl = vec![Vec::new(); lines.len()];
    let mut imports = HashMap::new();
    let Parsers { parser, queries } = parsers;
    if let Some((lang, query)) = Parsers::query_for(queries, ext) {
        if parser.set_language(lang).is_ok() {
            if let Some(tree) = parser.parse(text, None) {
                collect(tree.root_node(), text.as_bytes(), &lines, 0, None, &mut symbols, &mut imports);
                hl = highlight(tree.root_node(), query, text, &lines);
            }
        }
    }
    File { path, lines, hl, symbols, imports, mtime: None }
}

fn class_of(capture: &str) -> u8 {
    match capture.split('.').next().unwrap_or("") {
        "keyword" | "include" | "repeat" | "conditional" | "storageclass" | "storage" | "exception" => HL_KEYWORD,
        "string" | "character" | "escape" => HL_STRING,
        "comment" => HL_COMMENT,
        "function" | "method" | "constructor" | "macro" => HL_FUNCTION,
        "type" | "namespace" | "module" => HL_TYPE,
        "number" | "constant" | "boolean" | "float" => HL_CONSTANT,
        "property" | "field" | "attribute" | "label" | "tag" => HL_PROPERTY,
        "variable" if capture.contains("builtin") => HL_CONSTANT,
        _ => HL_PLAIN,
    }
}

/// Run the grammar's highlight query and bucket the captures per line. First capture wins where
/// they overlap, which is what tree-sitter highlight queries are written for.
fn highlight(root: Node, query: &Query, text: &str, lines: &[String]) -> Vec<Vec<Span>> {
    let mut line_starts: Vec<usize> = vec![0];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }
    let names = query.capture_names();
    let mut hl: Vec<Vec<Span>> = vec![Vec::new(); lines.len()];
    let mut cursor = QueryCursor::new();
    let mut caps = cursor.captures(query, root, text.as_bytes());
    while let Some((m, ci)) = caps.next() {
        let cap = m.captures[*ci];
        let class = class_of(names[cap.index as usize]);
        if class == HL_PLAIN {
            continue;
        }
        let (s, e) = (cap.node.start_byte(), cap.node.end_byte());
        let first = line_starts.partition_point(|&ls| ls <= s).saturating_sub(1);
        for li in first..lines.len() {
            let ls = line_starts[li];
            if ls >= e {
                break;
            }
            let le = ls + lines[li].len();
            let (a, b) = (s.max(ls), e.min(le));
            if a < b {
                hl[li].push(((a - ls) as u32, (b - ls) as u32, class));
            }
        }
    }
    for spans in &mut hl {
        spans.sort_by_key(|s| s.0);
        let mut end = 0;
        spans.retain(|s| {
            let keep = s.0 >= end;
            if keep {
                end = s.1;
            }
            keep
        });
    }
    hl
}

/// `Foo<T>` -> `Foo`, `&mut Foo` -> `Foo`, `crate::a::Foo` -> `Foo`.
fn bare_type(t: &str) -> String {
    let t = t.split('<').next().unwrap_or(t).trim_start_matches(['&', '*', ' ']).trim_start_matches("mut ").trim();
    t.rsplit("::").next().unwrap_or(t).trim().to_owned()
}

/// Record what an import statement brings into scope: every identifier segment maps to the
/// module it came from (the last path segment before it), so later `name` or `alias.name`
/// calls can be sent to that module's file. Loose by design: it is string work over the
/// statement's text, the same for every language.
fn record_import(text: &str, imports: &mut HashMap<String, String>) {
    let text = text.trim().trim_end_matches(';');
    // Odin: import "core:fmt" / import alias "../pkg"; JS: import {a, b} from './mod'
    if let Some(q) = text.find(['"', '\'']) {
        let quoted: String = text[q + 1..].chars().take_while(|c| *c != '"' && *c != '\'').collect();
        let module = quoted.rsplit(['/', ':', '\\']).next().unwrap_or(&quoted).trim_end_matches(".js").trim_end_matches(".ts").to_owned();
        let head = &text[..q];
        let mut named = false;
        for tok in head.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|t| !t.is_empty()) {
            if ["import", "from", "as", "type", "default"].contains(&tok) {
                continue;
            }
            imports.insert(tok.to_owned(), module.clone());
            named = true;
        }
        if !named {
            imports.insert(module.clone(), module);
        }
        return;
    }
    // Rust `use a::b::{c, d as e}` / Python `from a.b import c, d` / `import a.b`
    let body = text.trim_start_matches("pub ").trim_start_matches("use ").trim_start_matches("from ").trim_start_matches("import ");
    let (path_part, items) = match body.split_once(" import ") {
        Some((p, i)) => (p.trim(), i.trim()),
        None => match body.find('{') {
            Some(b) => (body[..b].trim().trim_end_matches("::"), body[b + 1..].trim_end_matches('}')),
            None => match body.rsplit_once("::").or_else(|| body.rsplit_once('.')) {
                Some((p, last)) => (p, last),
                None => (body, body),
            },
        },
    };
    let module = path_part.rsplit(['.', ':']).find(|s| !s.is_empty()).unwrap_or(path_part).to_owned();
    for item in items.split(',') {
        let item = item.trim();
        if item.is_empty() || item == "*" {
            continue;
        }
        let (name, alias) = match item.split_once(" as ") {
            Some((n, a)) => (n.trim(), Some(a.trim())),
            None => (item, None),
        };
        let name = name.rsplit("::").next().unwrap_or(name).trim();
        if name.is_empty() || name == "self" {
            continue;
        }
        imports.insert(alias.unwrap_or(name).to_owned(), module.clone());
    }
}

// Named children of `parent` become symbols. Name comes from the grammar's `name` field when
// there is one, else the first line (Odin's `foo :: proc` splits on `::`). One level of recursion
// into impl/mod/trait/class bodies so methods show up, tagged with their owner type.
fn collect(parent: Node, src: &[u8], lines: &[String], depth: u8, owner: Option<&str>, out: &mut Vec<Symbol>, imports: &mut HashMap<String, String>) {
    let mut cursor = parent.walk();
    for node in parent.named_children(&mut cursor) {
        let kind = node.kind();
        if kind.contains("import") || kind == "use_declaration" {
            if let Ok(t) = node.utf8_text(src) {
                record_import(t, imports);
            }
            continue;
        }
        if kind.contains("comment") || kind.contains("package") || kind.contains("attribute") {
            continue;
        }
        // `mod foo;` (no body) is a reference, not a definition; otherwise every `.map()` links to `mod map;`
        if kind == "mod_item" && node.child_by_field_name("body").is_none() {
            continue;
        }

        let start = node.start_position().row;
        let mut end = node.end_position().row;
        if node.end_position().column == 0 && end > start {
            end -= 1;
        }

        // Odin attributes (`@(test)`, `@(private)`) are part of the declaration node; name from the line after them.
        let mut name_row = start;
        while name_row < end && lines.get(name_row).is_some_and(|l| l.trim_start().starts_with('@')) {
            name_row += 1;
        }
        let first = lines.get(name_row).map(|l| l.trim()).unwrap_or("");
        let name = match node.child_by_field_name("name") {
            Some(n) => n.utf8_text(src).unwrap_or("").to_owned(),
            None => first.split('{').next().unwrap_or(first).split("::").next().unwrap_or(first).trim().to_owned(),
        };
        if name.is_empty() || name.len() > 80 {
            continue;
        }

        let container = depth == 0 && CONTAINERS.iter().any(|k| kind.contains(k));
        // the type an impl/class is about: Rust `impl X for Y` -> Y (field "type"), class -> its name
        let own_type: Option<String> = if container {
            node.child_by_field_name("type").and_then(|t| t.utf8_text(src).ok()).map(bare_type).or_else(|| if kind.contains("class") { Some(name.clone()) } else { None })
        } else {
            None
        };
        let mut calls = Vec::new();
        if !container {
            find_calls(node, src, &mut calls);
            calls.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| format!("{:?}", a.qual).cmp(&format!("{:?}", b.qual))));
            calls.dedup();
        }

        out.push(Symbol {
            name,
            kind,
            start,
            end,
            depth,
            owner: if container { own_type.clone() } else { owner.map(str::to_owned) },
            calls,
            callees: Vec::new(),
            callers: Vec::new(),
        });

        if container {
            if let Some(body) = node.child_by_field_name("body") {
                collect(body, src, lines, 1, own_type.as_deref(), out, imports);
            }
        }
    }
}

// Every `*call*` node below `node`; the callee is its `function` field (or first named child),
// split into the called name and the segment before it: `a.b.c(...)` -> c via b,
// `Foo::new(...)` -> new via Foo, `self.f()` -> f via self, `f!(...)` -> f.
fn find_calls(node: Node, src: &[u8], out: &mut Vec<Call>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind().contains("call") {
            let callee = child.child_by_field_name("function").or_else(|| child.named_child(0));
            if let Some(text) = callee.and_then(|c| c.utf8_text(src).ok()) {
                if let Some(mut call) = parse_callee(text) {
                    // Odin (and some others) parse `pkg.proc(x)` as member(pkg, call(proc, x)):
                    // the qualifier is the sibling before the call in the parent expression.
                    if call.qual == Qual::None {
                        let pk = node.kind();
                        if ["member", "selector", "scoped"].iter().any(|k| pk.contains(k)) {
                            if let Some(first) = node.named_child(0).filter(|f| f.id() != child.id()) {
                                if let Ok(t) = first.utf8_text(src) {
                                    if let Some(q) = t.rsplit(['.', ':']).next().filter(|q| !q.is_empty()) {
                                        call.qual = match q {
                                            "self" | "Self" | "this" | "super" => Qual::SelfRef,
                                            q => Qual::Some(bare_type(q)),
                                        };
                                    }
                                }
                            }
                        }
                    }
                    out.push(call);
                }
            }
        }
        find_calls(child, src, out);
    }
}

fn parse_callee(text: &str) -> Option<Call> {
    // drop generics and argument lists, then split on the last separator
    let mut clean = String::with_capacity(text.len());
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            _ if depth == 0 => clean.push(c),
            _ => {}
        }
    }
    let clean = clean.trim().trim_end_matches('!').trim();
    let segs: Vec<&str> = clean.split(['.', ':']).map(str::trim).filter(|s| !s.is_empty()).collect();
    let name = *segs.last()?;
    if name.len() > 64 || !name.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
        return None;
    }
    let qual = match segs.len() {
        0 | 1 => Qual::None,
        n => match segs[n - 2] {
            "self" | "Self" | "this" | "super" => Qual::SelfRef,
            q => Qual::Some(bare_type(q)),
        },
    };
    Some(Call { name: name.to_owned(), qual })
}

impl Index {
    pub fn find_file(&self, path: &str) -> Option<usize> {
        self.files.iter().position(|f| f.path == path)
    }

    pub fn sym(&self, r: SymRef) -> &Symbol {
        &self.files[r.file].symbols[r.sym]
    }

    /// Stable identity across re-indexes: (file path, symbol name).
    pub fn key(&self, r: SymRef) -> (String, String) {
        (self.files[r.file].path.clone(), self.sym(r).name.clone())
    }

    pub fn by_key(&self, key: &(String, String)) -> Option<SymRef> {
        let file = self.find_file(&key.0)?;
        let sym = self.files[file].symbols.iter().position(|s| s.name == key.1)?;
        Some(SymRef { file, sym })
    }

    /// All symbols with this name.
    /// Every symbol called `name`. `Owner::name` narrows to that owner or file stem,
    /// `path/file.rs:name` to that file.
    pub fn find_symbols(&self, name: &str) -> Vec<SymRef> {
        let (qual, name) = match name.rsplit_once("::").or_else(|| name.rsplit_once(':')) {
            Some((q, n)) => (Some(q.replace('\\', "/")), n),
            None => (None, name),
        };
        let mut out = Vec::new();
        for (file, f) in self.files.iter().enumerate() {
            for (sym, s) in f.symbols.iter().enumerate() {
                if s.name == name && qual.as_deref().is_none_or(|q| s.owner.as_deref() == Some(q) || f.stem() == q || f.path.ends_with(q)) {
                    out.push(SymRef { file, sym });
                }
            }
        }
        out
    }

    /// True if any indexed file's modification time differs from when it was read.
    /// ponytail: stats every file; new or deleted files are only noticed on a manual reindex.
    pub fn changed(&self) -> bool {
        self.files.iter().any(|f| std::fs::metadata(self.root.join(&f.path)).ok().and_then(|m| m.modified().ok()) != f.mtime)
    }

    fn owner_is(&self, r: SymRef, t: &str) -> bool {
        self.sym(r).owner.as_deref() == Some(t)
    }

    fn in_module(&self, r: SymRef, m: &str) -> bool {
        let f = &self.files[r.file];
        f.stem() == m || f.dir() == m
    }

    /// Where a call goes. Qualifier-aware, no types: `self.m` / `Self::m` bind to the caller's
    /// owner; `T::m` / `t.m` to a member of type `T`, else to module (file stem or directory)
    /// `T`, else through the file's imports; a lowercase receiver picks a member somewhere;
    /// an unqualified name picks a free function in the same file, then an imported one, then
    /// one in the same directory, then anywhere. Wrong when two types share a method name and
    /// the receiver is a variable; a real resolver needs per-language scope and type rules.
    fn resolve(&self, from: SymRef, call: &Call, by_name: &HashMap<String, Vec<SymRef>>) -> Option<SymRef> {
        // a struct/enum/union is data, not a call target, even where `Foo{...}` parses as a call
        let cands: Vec<SymRef> = by_name.get(&call.name)?.iter().copied().filter(|r| !["struct", "enum", "union"].iter().any(|k| self.sym(*r).kind.contains(k))).collect();
        let cands = &cands;
        let f = &self.files[from.file];
        let owner = self.sym(from).owner.as_deref();
        let same_file = |r: &&SymRef| r.file == from.file;
        let member = |r: &&SymRef| self.sym(**r).owner.is_some();
        let free = |r: &&SymRef| self.sym(**r).owner.is_none();
        let pick = |it: &mut dyn Iterator<Item = &SymRef>| -> Option<SymRef> {
            let v: Vec<SymRef> = it.copied().collect();
            v.iter().find(|r| r.file == from.file).or(v.first()).copied()
        };

        match &call.qual {
            Qual::SelfRef => {
                if let Some(o) = owner {
                    if let Some(r) = pick(&mut cands.iter().filter(|r| self.owner_is(**r, o))) {
                        return Some(r);
                    }
                }
                pick(&mut cands.iter().filter(same_file)).or_else(|| pick(&mut cands.iter()))
            }
            Qual::Some(q) => {
                if let Some(r) = pick(&mut cands.iter().filter(|r| self.owner_is(**r, q))) {
                    return Some(r);
                }
                if let Some(r) = pick(&mut cands.iter().filter(|r| self.in_module(**r, q))) {
                    return Some(r);
                }
                if let Some(m) = f.imports.get(q) {
                    // a known import that matched no file here is an external module (`log.error`)
                    return pick(&mut cands.iter().filter(|r| self.in_module(**r, m)));
                }
                let lowercase = q.chars().next().is_some_and(|c| c.is_lowercase() || c == '_');
                if !lowercase {
                    return None; // `Regex::new`: a type this repo does not define
                }
                // a variable receiver: some type's member, the caller's own type first; a
                // variable never calls a free function
                if let Some(o) = owner {
                    if let Some(r) = pick(&mut cands.iter().filter(|r| self.owner_is(**r, o))) {
                        return Some(r);
                    }
                }
                pick(&mut cands.iter().filter(member))
            }
            Qual::None => {
                if let Some(r) = pick(&mut cands.iter().filter(same_file).filter(free)) {
                    return Some(r);
                }
                if let Some(m) = f.imports.get(&call.name) {
                    if let Some(r) = pick(&mut cands.iter().filter(|r| self.in_module(**r, m))) {
                        return Some(r);
                    }
                }
                let dir = f.dir().to_owned();
                if let Some(r) = pick(&mut cands.iter().filter(free).filter(|r| self.files[r.file].dir() == dir)) {
                    return Some(r);
                }
                // only C reaches other files without naming them (headers); elsewhere an
                // unqualified name that is not local or imported is a builtin or a std call
                if f.path.ends_with(".c") || f.path.ends_with(".h") {
                    return pick(&mut cands.iter().filter(free)).or_else(|| pick(&mut cands.iter()));
                }
                None
            }
        }
    }

    pub fn link(&mut self) {
        let mut by_name: HashMap<String, Vec<SymRef>> = HashMap::new();
        for (file, f) in self.files.iter().enumerate() {
            for (sym, s) in f.symbols.iter().enumerate() {
                by_name.entry(s.name.clone()).or_default().push(SymRef { file, sym });
            }
        }

        let mut edges = Vec::new();
        for (file, f) in self.files.iter().enumerate() {
            for (sym, s) in f.symbols.iter().enumerate() {
                let from = SymRef { file, sym };
                for call in &s.calls {
                    if let Some(to) = self.resolve(from, call, &by_name) {
                        edges.push((from, to));
                    }
                }
            }
        }
        edges.sort_by_key(|(a, b)| (a.file, a.sym, b.file, b.sym));
        edges.dedup();

        for (from, to) in edges {
            self.files[from.file].symbols[from.sym].callees.push(to);
            self.files[to.file].symbols[to.sym].callers.push(from);
        }
    }

    /// Symbols that call something but are called by nothing: entry points of auto-mapped paths.
    pub fn roots(&self) -> Vec<SymRef> {
        let mut out = Vec::new();
        for (file, f) in self.files.iter().enumerate() {
            for (sym, s) in f.symbols.iter().enumerate() {
                if !s.callees.is_empty() && s.callers.is_empty() {
                    out.push(SymRef { file, sym });
                }
            }
        }
        out.sort_by_key(|r| std::cmp::Reverse(self.sym(*r).callees.len()));
        out
    }

    /// Pre-order call tree from `root`, depth-limited, each symbol at most once.
    pub fn call_tree(&self, root: SymRef, max_depth: usize) -> Vec<(SymRef, usize)> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        self.walk(root, 0, max_depth, &mut seen, &mut out);
        out
    }

    fn walk(&self, r: SymRef, depth: usize, max: usize, seen: &mut HashSet<SymRef>, out: &mut Vec<(SymRef, usize)>) {
        if !seen.insert(r) {
            return;
        }
        out.push((r, depth));
        if depth < max {
            for &c in &self.sym(r).callees {
                self.walk(c, depth + 1, max, seen, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(files: &[(&str, &str)]) -> Index {
        let mut p = Parsers::new();
        let files = files.iter().map(|(path, src)| parse_file(&mut p, path.to_string(), src, path.rsplit('.').next().unwrap())).collect();
        let mut idx = Index { root: ".".into(), files };
        idx.link();
        idx
    }

    fn callee_names(idx: &Index, name: &str, file: &str) -> Vec<String> {
        let fi = idx.find_file(file).unwrap();
        let si = idx.files[fi].symbols.iter().position(|s| s.name == name).unwrap();
        idx.sym(SymRef { file: fi, sym: si }).callees.iter().map(|r| format!("{}:{}", idx.files[r.file].path, idx.sym(*r).name)).collect()
    }

    #[test]
    fn links_calls_by_name() {
        let src = "fn a() { b(); c::d(); }\nfn b() {}\nstruct C;\nimpl C { fn d() { b() } }\n";
        let idx = index(&[("x.rs", src)]);
        let names: Vec<&str> = idx.files[0].symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "C", "impl C", "d"]);
        assert_eq!(idx.files[0].symbols[4].owner.as_deref(), Some("C"));

        let a = idx.find_symbols("a")[0];
        let callees: Vec<&str> = idx.sym(a).callees.iter().map(|r| idx.sym(*r).name.as_str()).collect();
        assert_eq!(callees, ["b", "d"]);
        assert_eq!(idx.sym(idx.find_symbols("b")[0]).callers.len(), 2);
        assert_eq!(idx.roots(), [a]);
        assert_eq!(idx.call_tree(a, 5).len(), 3);
    }

    #[test]
    fn resolves_by_owner_module_and_import() {
        let a = "pub struct A;\nimpl A { pub fn new() -> A { A } fn go(&self) { self.step(); Self::new(); } fn step(&self) {} }\npub fn helper() {}\n";
        let b = "use crate::a::helper;\npub struct B;\nimpl B { pub fn new() -> B { B } fn step(&self) {} }\nfn run(x: &B) { A::new(); B::new(); x.step(); helper(); a::helper(); }\nfn helper() {}\n";
        let idx = index(&[("src/a.rs", a), ("src/b.rs", b)]);

        // self.step / Self::new bind to A's members, not B's
        assert_eq!(callee_names(&idx, "go", "src/a.rs"), ["src/a.rs:new", "src/a.rs:step"]);
        // A::new -> A's new, B::new -> B's new, x.step -> a member (same file wins), helper -> same-file free fn,
        // a::helper -> module a
        let run = callee_names(&idx, "run", "src/b.rs");
        assert!(run.contains(&"src/a.rs:new".to_string()), "{run:?}");
        assert!(run.contains(&"src/b.rs:new".to_string()), "{run:?}");
        assert!(run.contains(&"src/b.rs:step".to_string()), "{run:?}");
        assert!(run.contains(&"src/b.rs:helper".to_string()), "{run:?}");
        assert!(run.contains(&"src/a.rs:helper".to_string()), "{run:?}");
        assert_eq!(idx.files[1].imports.get("helper").map(String::as_str), Some("a"));
    }

    #[test]
    fn odin_package_calls() {
        let main = "package main\nimport \"core\"\nimport \"util\"\nimport \"core:log\"\nS :: struct { commands: int }\nmain :: proc() {\n    core.init_bookmarks(nil)\n    util.make_static_list(int, 4)\n    helper()\n    make([]int, 4)\n    s := S{ commands = make(int) }\n    log.error(\"x\")\n}\nhelper :: proc() {}\n";
        let core = "package core\ninit_bookmarks :: proc(b: rawptr) {}\nerror :: proc(msg: string) {}\n";
        let util = "package util\nmake_static_list :: proc($T: typeid, n: int) {}\nmake :: proc() {}\n";
        let idx = index(&[("src/main.odin", main), ("src/core/bookmarks.odin", core), ("src/util/list.odin", util)]);
        let m = idx.find_symbols("main")[0];
        let calls: Vec<String> = idx.sym(m).calls.iter().map(|c| format!("{}/{:?}", c.name, c.qual)).collect();
        assert!(calls.iter().any(|c| c == "init_bookmarks/Some(\"core\")"), "{calls:?}");
        let mut got = callee_names(&idx, "main", "src/main.odin");
        got.sort();
        assert_eq!(got, ["src/core/bookmarks.odin:init_bookmarks", "src/main.odin:helper", "src/util/list.odin:make_static_list"], "{got:?}");
    }

    #[test]
    fn parses_callees() {
        assert_eq!(parse_callee("foo"), Some(Call { name: "foo".into(), qual: Qual::None }));
        assert_eq!(parse_callee("self.graph.focus"), Some(Call { name: "focus".into(), qual: Qual::Some("graph".into()) }));
        assert_eq!(parse_callee("Self::new"), Some(Call { name: "new".into(), qual: Qual::SelfRef }));
        assert_eq!(parse_callee("Vec::<u8>::with_capacity"), Some(Call { name: "with_capacity".into(), qual: Qual::Some("Vec".into()) }));
        assert_eq!(parse_callee("println!"), Some(Call { name: "println".into(), qual: Qual::None }));
        assert_eq!(parse_callee("pkg.proc"), Some(Call { name: "proc".into(), qual: Qual::Some("pkg".into()) }));
        assert_eq!(parse_callee("(a)"), None);
    }

    #[test]
    fn records_imports() {
        let mut m = HashMap::new();
        record_import("use crate::index::{build, Index as Idx};", &mut m);
        record_import("use std::collections::HashMap;", &mut m);
        record_import("import \"../util\"", &mut m);
        record_import("import fmt \"core:fmt\"", &mut m);
        record_import("from index import build, link", &mut m);
        record_import("import { thing } from './mod.js'", &mut m);
        assert_eq!(m["build"], "index");
        assert_eq!(m["Idx"], "index");
        assert_eq!(m["HashMap"], "collections");
        assert_eq!(m["util"], "util");
        assert_eq!(m["fmt"], "fmt");
        assert_eq!(m["link"], "index");
        assert_eq!(m["thing"], "mod");
    }

    #[test]
    fn highlights_keywords_and_strings() {
        let src = "fn a() { let s = \"hi\"; } // c\n";
        let mut p = Parsers::new();
        let f = parse_file(&mut p, "x.rs".into(), src, "rs");
        let classes: Vec<u8> = f.hl[0].iter().map(|s| s.2).collect();
        assert!(classes.contains(&HL_KEYWORD), "{classes:?}");
        assert!(classes.contains(&HL_STRING), "{classes:?}");
        assert!(classes.contains(&HL_COMMENT), "{classes:?}");
        assert!(f.hl[0].windows(2).all(|w| w[0].1 <= w[1].0), "overlap: {:?}", f.hl[0]);
    }
}
