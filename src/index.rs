use crate::lsp;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
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
    pub kind: String,
    pub start: usize, // inclusive 0-based line rows
    pub end: usize,
    pub depth: u8,
    pub owner: Option<String>,       // the type / class a member belongs to (impl Foo -> "Foo")
    pub calls: Vec<Call>,            // tree-sitter backend: raw call sites, resolved by name in `link`
    pub targets: Vec<(String, u32)>, // server backend: (file, line) of each callee's definition, resolved in `link`
    pub refs: Vec<(String, u32)>,    // server backend: (file, line) of every reference to this symbol
    pub callees: Vec<SymRef>,
    pub callers: Vec<SymRef>,
}

/// Where a file's symbols and xrefs came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    TreeSitter = 0,
    Server = 1,
}

/// A language and its server, looked up on PATH by name.
pub struct Lang {
    pub exts: &'static [&'static str],
    pub server: &'static str,
    pub args: &'static [&'static str],
}

pub const LANGS: [Lang; 5] = [
    Lang { exts: &["rs"], server: "rust-analyzer", args: &[] },
    Lang { exts: &["odin"], server: "ols", args: &[] },
    Lang { exts: &["c", "h"], server: "clangd", args: &[] },
    Lang { exts: &["py"], server: "pyright-langserver", args: &["--stdio"] },
    Lang { exts: &["js", "mjs", "cjs", "ts", "tsx"], server: "typescript-language-server", args: &["--stdio"] },
];

pub fn lang_for(path: &str) -> Option<&'static Lang> {
    let ext = path.rsplit('.').next().unwrap_or("");
    LANGS.iter().find(|l| l.exts.contains(&ext))
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
    pub hash: u64, // FNV-1a of the text as indexed (tabs expanded)
    pub backend: Backend,
    pub pending: bool, // a server exists for this language and has not answered for this text yet
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

pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Walks the root. A file whose text matches its cache entry is taken from the cache without
/// parsing; anything else is parsed with tree-sitter now and, if its language has a server,
/// marked pending for it. A server answer names lines in other files, so a cached answer is
/// also pending again when any file it points into changed.
// ponytail: reads the whole tree into memory up front on the main thread; move to a background
// thread with a progress bar when startup on a big repo becomes annoying.
pub fn build(root: &Path) -> Index {
    let mut parsers = Parsers::new();
    let mut cache = load_cache(&root.join(CACHE)).unwrap_or_default();
    let mut changed: HashSet<String> = HashSet::new();
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
        let hash = fnv1a(text.as_bytes());
        let mut f = match cache.remove(&rel) {
            Some(mut c) if c.hash == hash => {
                c.lines = text.lines().map(str::to_owned).collect();
                c.hl.resize(c.lines.len(), Vec::new());
                c
            }
            _ => {
                changed.insert(rel.clone());
                parse_file(&mut parsers, rel, &text, ext)
            }
        };
        f.pending = lang_for(&f.path).is_some() && f.backend != Backend::Server;
        f.mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
        files.push(f);
    }
    for f in &mut files {
        if f.backend == Backend::Server && f.symbols.iter().any(|s| s.targets.iter().chain(&s.refs).any(|(p, _)| changed.contains(p))) {
            f.pending = true;
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut idx = Index { root: root.to_path_buf(), files };
    idx.link();
    if !changed.is_empty() || !cache.is_empty() {
        idx.save_cache(); // parsed something, or a cached file is gone
    }
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
    File { path, lines, hl, symbols, imports, mtime: None, hash: fnv1a(text.as_bytes()), backend: Backend::TreeSitter, pending: false }
}

// ---- language servers ---------------------------------------------------------------

/// One file's symbols as a server reported them, with the text hash they were asked for.
pub struct ServerFile {
    pub path: String,
    pub hash: u64,
    pub symbols: Vec<Symbol>,
}

fn symbol_kind_name(k: u64, name: &str) -> &'static str {
    match k {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 if name.starts_with("impl") => "impl",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum-member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type",
        _ => "symbol",
    }
}

/// `impl<T> Trait for Foo<T>` -> `Foo`; `impl Foo` -> `Foo`.
fn impl_type(name: &str) -> String {
    let t = name.split(" for ").last().unwrap_or(name).trim_start_matches("impl");
    let t = match t.strip_prefix('<') {
        Some(rest) => {
            let mut depth = 1;
            let end = rest.char_indices().find(|&(_, c)| {
                depth += match c {
                    '<' => 1,
                    '>' => -1,
                    _ => 0,
                };
                depth == 0
            });
            end.map(|(i, _)| &rest[i + 1..]).unwrap_or(rest)
        }
        None => t,
    };
    bare_type(t.trim())
}

/// One DocumentSymbol and, for a container, its direct children. Records where each symbol's
/// name sits so call-hierarchy and reference requests can point at it.
fn push_symbol(v: &Value, depth: u8, owner: Option<&str>, out: &mut Vec<Symbol>, sel: &mut Vec<(u64, u64)>) {
    let name = v["name"].as_str().unwrap_or("").to_owned();
    if name.is_empty() {
        return;
    }
    let k = v["kind"].as_u64().unwrap_or(0);
    let r = &v["range"];
    let p = &v["selectionRange"]["start"];
    // Servers start a symbol at its doc comment or attributes; tree-sitter at the declaration.
    // Anchors are offsets from the start, so both backends use the line the name is on.
    let start = p["line"].as_u64().unwrap_or(0) as usize;
    let mut end = (r["end"]["line"].as_u64().unwrap_or(0) as usize).max(start);
    if r["end"]["character"] == 0 && end > start {
        end -= 1;
    }
    let container = depth == 0 && matches!(k, 2 | 3 | 5 | 11 | 19);
    let own: Option<String> = if container { Some(if k == 19 { impl_type(&name) } else { name.clone() }) } else { owner.map(str::to_owned) };
    sel.push((p["line"].as_u64().unwrap_or(0), p["character"].as_u64().unwrap_or(0)));
    out.push(Symbol { kind: symbol_kind_name(k, &name).to_owned(), name, start, end, depth, owner: own.clone(), calls: Vec::new(), targets: Vec::new(), refs: Vec::new(), callees: Vec::new(), callers: Vec::new() });
    if container {
        for c in v["children"].as_array().into_iter().flatten() {
            push_symbol(c, 1, own.as_deref(), out, sel);
        }
    }
}

/// (file, line) for each element of a Location-like array, files outside the root dropped.
fn locations(v: &Value, root: &Path, item: impl Fn(&Value) -> (&Value, &Value)) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            let (uri, pos) = item(l);
            Some((lsp::from_uri(uri.as_str()?, root)?, pos["line"].as_u64()? as u32))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Starts the language's server on the root and waits for it to finish its own indexing.
/// Err when it is not on PATH or will not start. The returned root is absolute, which is
/// what the server's URIs are compared against.
pub fn start_server(root: &Path, lang: &Lang) -> Result<(lsp::Client, PathBuf), String> {
    let exe = lsp::find_on_path(lang.server).ok_or_else(|| format!("{} not on PATH", lang.server))?;
    let root = std::path::absolute(root).map_err(|e| e.to_string())?;
    let mut c = lsp::Client::start(&exe, lang.args, &root).ok_or_else(|| format!("{} would not start", lang.server))?;
    c.wait_ready(Duration::from_secs(300));
    Ok((c, root))
}

/// One file's symbols, outgoing calls and references from a running server.
pub fn index_file(c: &mut lsp::Client, root: &Path, path: &str, hash: u64) -> ServerFile {
    let doc = json!({"uri": lsp::to_uri(&root.join(path))});
    let syms = c.request("textDocument/documentSymbol", json!({"textDocument": doc})).unwrap_or(Value::Null);
    let mut symbols = Vec::new();
    let mut sel = Vec::new();
    for s in syms.as_array().into_iter().flatten() {
        push_symbol(s, 0, None, &mut symbols, &mut sel);
    }
    for (s, (line, ch)) in symbols.iter_mut().zip(&sel) {
        let at = json!({"textDocument": doc, "position": {"line": line, "character": ch}});
        for item in c.request("textDocument/prepareCallHierarchy", at.clone()).unwrap_or(Value::Null).as_array().into_iter().flatten() {
            let calls = c.request("callHierarchy/outgoingCalls", json!({"item": item})).unwrap_or(Value::Null);
            s.targets.extend(locations(&calls, root, |call| (&call["to"]["uri"], &call["to"]["selectionRange"]["start"])));
        }
        let mut at = at;
        at["context"] = json!({"includeDeclaration": false});
        let refs = c.request("textDocument/references", at).unwrap_or(Value::Null);
        s.refs = locations(&refs, root, |l| (&l["uri"], &l["range"]["start"]));
        s.targets.sort();
        s.targets.dedup();
    }
    ServerFile { path: path.to_owned(), hash, symbols }
}

/// Asks the language's server for symbols, outgoing calls and references of each file, handing
/// each file over as it completes, then stops it. Err when the server is not on PATH or will
/// not start.
pub fn query_server(root: &Path, lang: &Lang, files: &[(String, u64)], mut each: impl FnMut(ServerFile)) -> Result<(), String> {
    let (mut c, root) = start_server(root, lang)?;
    for (path, hash) in files {
        each(index_file(&mut c, &root, path, *hash));
    }
    c.shutdown();
    Ok(())
}

/// A bundled grammar exists for the file, so it has symbols even without a server.
pub fn has_grammar(path: &str) -> bool {
    language_for(path.rsplit('.').next().unwrap_or("")).is_some()
}

// ---- cache ----------------------------------------------------------------------------
// ".codemap-cache" at the root: every file's symbols, xrefs, imports and highlight spans, keyed
// by the file's text hash. Derived from the backends, never committed, and rebuilt whenever
// it cannot be read.
//
// "CMCH" u32 version
// u32 nfiles { str path, u64 hash, u8 backend,
//   u32 nsyms { str name, str kind, u32 start, u32 end, u8 depth, str owner ("" = none),
//     u32 ncalls { str name, u8 qual (0 none, 1 self, 2 named), str qualifier (only when 2) },
//     u32 ntargets { str path, u32 line }, u32 nrefs { str path, u32 line } }
//   u32 nimports { str name, str module }
//   u32 nlines { u32 nspans { u32 start, u32 end, u8 class } } }
// str = u32 len + utf8 bytes. All little-endian.

pub const CACHE: &str = ".codemap-cache";
const CACHE_MAGIC: &[u8; 4] = b"CMCH";
const CACHE_VERSION: u32 = 1;

pub(crate) fn w_str(b: &mut Vec<u8>, s: &str) {
    w_u32(b, s.len() as u32);
    b.extend_from_slice(s.as_bytes());
}

pub(crate) fn w_u32(b: &mut Vec<u8>, n: u32) {
    b.extend_from_slice(&n.to_le_bytes());
}

fn w_locs(b: &mut Vec<u8>, locs: &[(String, u32)]) {
    w_u32(b, locs.len() as u32);
    for (p, l) in locs {
        w_str(b, p);
        w_u32(b, *l);
    }
}

pub(crate) struct Reader<'a> {
    pub data: &'a [u8],
    pub off: usize,
}

impl Reader<'_> {
    pub fn bytes(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.data.get(self.off..self.off + n)?;
        self.off += n;
        Some(s)
    }
    pub fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }
    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    pub fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.bytes(8)?.try_into().ok()?))
    }
    pub fn str(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        Some(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }
    fn locs(&mut self) -> Option<Vec<(String, u32)>> {
        (0..self.u32()?).map(|_| Some((self.str()?, self.u32()?))).collect()
    }
}

/// Cached files by path; their `lines` are empty until `build` fills them from disk.
fn load_cache(path: &Path) -> Option<HashMap<String, File>> {
    let data = std::fs::read(path).ok()?;
    let mut r = Reader { data: &data, off: 0 };
    if r.bytes(4)? != CACHE_MAGIC || r.u32()? != CACHE_VERSION {
        return None;
    }
    let mut out = HashMap::new();
    for _ in 0..r.u32()? {
        let path = r.str()?;
        let hash = r.u64()?;
        let backend = if r.u8()? == 1 { Backend::Server } else { Backend::TreeSitter };
        let mut symbols = Vec::new();
        for _ in 0..r.u32()? {
            let (name, kind) = (r.str()?, r.str()?);
            let (start, end, depth) = (r.u32()? as usize, r.u32()? as usize, r.u8()?);
            let owner = Some(r.str()?).filter(|o| !o.is_empty());
            let mut calls = Vec::new();
            for _ in 0..r.u32()? {
                let name = r.str()?;
                let qual = match r.u8()? {
                    0 => Qual::None,
                    1 => Qual::SelfRef,
                    _ => Qual::Some(r.str()?),
                };
                calls.push(Call { name, qual });
            }
            let (targets, refs) = (r.locs()?, r.locs()?);
            symbols.push(Symbol { name, kind, start, end, depth, owner, calls, targets, refs, callees: Vec::new(), callers: Vec::new() });
        }
        let imports = (0..r.u32()?).map(|_| Some((r.str()?, r.str()?))).collect::<Option<HashMap<_, _>>>()?;
        let mut hl = Vec::new();
        for _ in 0..r.u32()? {
            hl.push((0..r.u32()?).map(|_| Some((r.u32()?, r.u32()?, r.u8()?))).collect::<Option<Vec<Span>>>()?);
        }
        out.insert(path.clone(), File { path, lines: Vec::new(), hl, symbols, imports, mtime: None, hash, backend, pending: false });
    }
    Some(out)
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
            kind: kind.to_owned(),
            start,
            end,
            depth,
            owner: if container { own_type.clone() } else { owner.map(str::to_owned) },
            calls,
            targets: Vec::new(),
            refs: Vec::new(),
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

/// `word` appears on line `li` of `f` as a whole identifier outside comments and strings: a
/// place that names it in code.
pub fn call_site(f: &File, li: usize, word: &str) -> bool {
    let Some(line) = f.lines.get(li) else { return false };
    let is_id = |c: char| c.is_alphanumeric() || c == '_';
    let spans = f.hl.get(li).map(Vec::as_slice).unwrap_or(&[]);
    line.match_indices(word).any(|(i, _)| {
        let whole = !line[..i].chars().next_back().is_some_and(is_id) && !line[i + word.len()..].chars().next().is_some_and(is_id);
        let quoted = spans.iter().any(|&(s, e, class)| (class == HL_COMMENT || class == HL_STRING) && (s as usize) <= i && i < e as usize);
        whole && !quoted
    })
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

    /// The innermost symbol of `path` containing `line`.
    pub fn by_line(&self, path: &str, line: usize) -> Option<SymRef> {
        let file = self.find_file(path)?;
        let sym = self.files[file].symbols.iter().enumerate().filter(|(_, s)| s.start <= line && line <= s.end).max_by_key(|(_, s)| s.depth)?.0;
        Some(SymRef { file, sym })
    }

    pub fn save_cache(&self) {
        let mut b = Vec::with_capacity(1 << 16);
        b.extend_from_slice(CACHE_MAGIC);
        w_u32(&mut b, CACHE_VERSION);
        w_u32(&mut b, self.files.len() as u32);
        for f in &self.files {
            w_str(&mut b, &f.path);
            b.extend_from_slice(&f.hash.to_le_bytes());
            b.push(f.backend as u8);
            w_u32(&mut b, f.symbols.len() as u32);
            for s in &f.symbols {
                w_str(&mut b, &s.name);
                w_str(&mut b, &s.kind);
                w_u32(&mut b, s.start as u32);
                w_u32(&mut b, s.end as u32);
                b.push(s.depth);
                w_str(&mut b, s.owner.as_deref().unwrap_or(""));
                w_u32(&mut b, s.calls.len() as u32);
                for c in &s.calls {
                    w_str(&mut b, &c.name);
                    match &c.qual {
                        Qual::None => b.push(0),
                        Qual::SelfRef => b.push(1),
                        Qual::Some(q) => {
                            b.push(2);
                            w_str(&mut b, q);
                        }
                    }
                }
                w_locs(&mut b, &s.targets);
                w_locs(&mut b, &s.refs);
            }
            w_u32(&mut b, f.imports.len() as u32);
            for (k, v) in &f.imports {
                w_str(&mut b, k);
                w_str(&mut b, v);
            }
            w_u32(&mut b, f.hl.len() as u32);
            for spans in &f.hl {
                w_u32(&mut b, spans.len() as u32);
                for &(s, e, c) in spans {
                    w_u32(&mut b, s);
                    w_u32(&mut b, e);
                    b.push(c);
                }
            }
        }
        let _ = crate::map::write_retry(&self.root.join(CACHE), &b);
    }

    /// Files waiting on a server, grouped by language.
    pub fn pending(&self) -> Vec<(&'static Lang, Vec<(String, u64)>)> {
        let mut out: Vec<(&'static Lang, Vec<(String, u64)>)> = Vec::new();
        for f in self.files.iter().filter(|f| f.pending) {
            let Some(lang) = lang_for(&f.path) else { continue };
            match out.iter_mut().find(|(l, _)| l.server == lang.server) {
                Some((_, v)) => v.push((f.path.clone(), f.hash)),
                None => out.push((lang, vec![(f.path.clone(), f.hash)])),
            }
        }
        out
    }

    /// Takes a server's answer for a file, unless the file changed since it was asked. The
    /// caller re-links afterwards.
    pub fn apply(&mut self, r: ServerFile) -> bool {
        let Some(fi) = self.find_file(&r.path) else { return false };
        let f = &mut self.files[fi];
        if f.hash != r.hash {
            return false;
        }
        f.symbols = r.symbols;
        f.backend = Backend::Server;
        f.pending = false;
        true
    }

    /// Files of a language stop waiting: its server is not available, so tree-sitter's answer
    /// stands.
    pub fn give_up(&mut self, lang: &Lang) {
        for f in &mut self.files {
            if lang_for(&f.path).is_some_and(|l| l.server == lang.server) {
                f.pending = false;
            }
        }
    }

    /// Runs every pending language's server to completion on this thread, reporting progress
    /// and failures as one-line messages, then re-links and saves the cache.
    pub fn run_backends(&mut self, mut report: impl FnMut(&str)) {
        for (lang, files) in self.pending() {
            let n = files.len();
            let mut results = Vec::new();
            report(&format!("{}: {n} files to index", lang.server));
            let run = query_server(&self.root, lang, &files, |r| results.push(r));
            match run {
                Ok(()) => results.into_iter().for_each(|r| {
                    self.apply(r);
                }),
                Err(e) => {
                    report(&format!("{e}: {n} files keep the tree-sitter resolver"));
                    self.give_up(lang);
                }
            }
        }
        self.link();
        self.save_cache();
    }

    /// All symbols with this name.
    /// Every symbol called `name`. `Owner::name` narrows to that owner, `file:name` to a file
    /// (its stem or a path suffix), `file:Owner::name` to both; a bare qualifier that is neither
    /// is tried as each.
    pub fn find_symbols(&self, name: &str) -> Vec<SymRef> {
        let (qual, name) = match name.rsplit_once("::").or_else(|| name.rsplit_once(':')) {
            Some((q, n)) => (Some(q.replace('\\', "/")), n),
            None => (None, name),
        };
        let (file_q, owner_q): (Option<&str>, Option<&str>) = match qual.as_deref() {
            None => (None, None),
            Some(q) => match q.split_once(':') {
                Some((f, o)) if !f.is_empty() && !o.is_empty() => (Some(f), Some(o)),
                _ => (None, Some(q)),
            },
        };
        let in_file = |f: &File, q: &str| f.stem() == q || f.path.ends_with(q);
        let mut out = Vec::new();
        for (file, f) in self.files.iter().enumerate() {
            for (sym, s) in f.symbols.iter().enumerate() {
                if s.name != name {
                    continue;
                }
                let ok = match (file_q, owner_q) {
                    (None, None) => true,
                    (Some(fq), Some(oq)) => in_file(f, fq) && s.owner.as_deref() == Some(oq),
                    (None, Some(q)) => s.owner.as_deref() == Some(q) || in_file(f, q),
                    (Some(fq), None) => in_file(f, fq),
                };
                if ok {
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

    /// Rebuilds every callee / caller list: server targets by (file, line), tree-sitter calls
    /// by name.
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
                for (path, line) in &s.targets {
                    if let Some(to) = self.by_line(path, *line as usize) {
                        edges.push((from, to));
                    }
                }
            }
        }
        edges.sort_by_key(|(a, b)| (a.file, a.sym, b.file, b.sym));
        edges.dedup();

        for f in &mut self.files {
            for s in &mut f.symbols {
                s.callees.clear();
                s.callers.clear();
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
