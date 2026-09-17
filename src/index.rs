use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::{Language, Node, Parser};

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

pub struct File {
    pub path: String, // relative to root, forward slashes
    pub lines: Vec<String>,
    pub symbols: Vec<Symbol>,
}

pub struct Index {
    pub files: Vec<File>,
}

const MAX_FILE: usize = 4 << 20;
const CONTAINERS: [&str; 4] = ["impl", "mod", "trait", "class"];

fn language_for(ext: &str) -> Option<Language> {
    Some(match ext {
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        "odin" => tree_sitter_odin::LANGUAGE.into(),
        "c" | "h" => tree_sitter_c::LANGUAGE.into(),
        "py" => tree_sitter_python::LANGUAGE.into(),
        "js" | "mjs" | "cjs" => tree_sitter_javascript::LANGUAGE.into(),
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        _ => return None,
    })
}

// ponytail: reads the whole tree into memory up front on the main thread; move to a background
// thread with a progress bar when startup on a big repo becomes annoying.
pub fn build(root: &Path) -> Index {
    let mut parser = Parser::new();
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
        files.push(parse_file(&mut parser, rel, &text, language_for(ext)));
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut idx = Index { files };
    idx.link();
    idx
}

pub fn parse_file(parser: &mut Parser, path: String, text: &str, lang: Option<Language>) -> File {
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut symbols = Vec::new();
    if let Some(lang) = lang {
        if parser.set_language(&lang).is_ok() {
            if let Some(tree) = parser.parse(text, None) {
                collect(tree.root_node(), text.as_bytes(), &lines, 0, &mut symbols);
            }
        }
    }
    File { path, lines, symbols }
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
        let mut p = Parser::new();
        let f = parse_file(&mut p, "x.rs".into(), src, language_for("rs"));
        let mut idx = Index { files: vec![f] };
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
}
