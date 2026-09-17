use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SymRef {
    pub file: usize,
    pub sym: usize,
}

pub struct Symbol {
    pub name: String,
    pub kind: &'static str,
    pub start: usize, // inclusive 0-based line rows
    pub end: usize,
    pub depth: u8,
    pub calls: Vec<String>, // raw callee names found in the body
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
    pub mtime: Option<SystemTime>,
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
    let Parsers { parser, queries } = parsers;
    if let Some((lang, query)) = Parsers::query_for(queries, ext) {
        if parser.set_language(lang).is_ok() {
            if let Some(tree) = parser.parse(text, None) {
                collect(tree.root_node(), text.as_bytes(), &lines, 0, &mut symbols);
                hl = highlight(tree.root_node(), query, text, &lines);
            }
        }
    }
    File { path, lines, hl, symbols, mtime: None }
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

// Named children of `parent` become symbols. Name comes from the grammar's `name` field when
// there is one, else the first line (Odin's `foo :: proc` splits on `::`). One level of recursion
// into impl/mod/trait/class bodies so methods show up.
fn collect(parent: Node, src: &[u8], lines: &[String], depth: u8, out: &mut Vec<Symbol>) {
    let mut cursor = parent.walk();
    for node in parent.named_children(&mut cursor) {
        let kind = node.kind();
        if kind.contains("comment") || kind.contains("import") || kind.contains("package") || kind.contains("attribute") || kind == "use_declaration" {
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
        let mut calls = Vec::new();
        if !container {
            find_calls(node, src, &mut calls);
            calls.sort();
            calls.dedup();
        }

        out.push(Symbol { name, kind, start, end, depth, calls, callees: Vec::new(), callers: Vec::new() });

        if container {
            if let Some(body) = node.child_by_field_name("body") {
                collect(body, src, lines, 1, out);
            }
        }
    }
}

// Every `*call*` node below `node`; the callee is its `function` field (or first named child),
// reduced to the last path segment: `a.b.c(...)` -> c, `Foo::new(...)` -> new, `f!(...)` -> f.
fn find_calls(node: Node, src: &[u8], out: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind().contains("call") {
            let callee = child.child_by_field_name("function").or_else(|| child.named_child(0));
            if let Some(text) = callee.and_then(|c| c.utf8_text(src).ok()) {
                let seg = text.rsplit(['.', ':']).next().unwrap_or(text);
                let seg = seg.split(['<', '(', '!', ' ']).next().unwrap_or(seg).trim();
                if !seg.is_empty() && seg.len() <= 64 && seg.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
                    out.push(seg.to_owned());
                }
            }
        }
        find_calls(child, src, out);
    }
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

    /// All symbols with this name (same-file matches are not preferred here; see `link`).
    pub fn find_symbols(&self, name: &str) -> Vec<SymRef> {
        let mut out = Vec::new();
        for (file, f) in self.files.iter().enumerate() {
            for (sym, s) in f.symbols.iter().enumerate() {
                if s.name == name {
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

    // ponytail: name-based resolution, no types. Same-file match wins, else the first definition
    // in path order. Wrong for overloaded names like `new`; a real resolver needs per-language
    // scope rules.
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
                for name in &s.calls {
                    let Some(cands) = by_name.get(name) else { continue };
                    let target = cands.iter().find(|r| r.file == file).or(cands.first()).copied().unwrap();
                    edges.push((SymRef { file, sym }, target));
                }
            }
        }

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

    #[test]
    fn links_calls_by_name() {
        let src = "fn a() { b(); c::d(); }\nfn b() {}\nstruct C;\nimpl C { fn d() { b() } }\n";
        let mut p = Parsers::new();
        let f = parse_file(&mut p, "x.rs".into(), src, "rs");
        let mut idx = Index { root: ".".into(), files: vec![f] };
        idx.link();

        let names: Vec<&str> = idx.files[0].symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "C", "impl C", "d"]);

        let a = idx.find_symbols("a")[0];
        let callees: Vec<&str> = idx.sym(a).callees.iter().map(|r| idx.sym(*r).name.as_str()).collect();
        assert_eq!(callees, ["b", "d"]);
        assert_eq!(idx.sym(idx.find_symbols("b")[0]).callers.len(), 2);
        assert_eq!(idx.roots(), [a]);
        assert_eq!(idx.call_tree(a, 5).len(), 3);
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
