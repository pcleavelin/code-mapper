//! The server backend: a file's symbols, outgoing calls and references asked of the language's
//! server, and the bookkeeping of which files still wait for one.

use super::{Backend, Index, Lang, Symbol, lang_for, treesitter::bare_type};
use crate::lsp;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One file's symbols as a server reported them, with the text hash they were asked for.
pub struct ServerFile {
    pub path: String,
    pub hash: u64,
    pub symbols: Vec<Symbol>,
}

/// LSP SymbolKind names, indexed by kind; 0 is not a kind.
const KINDS: [&str; 27] = [
    "symbol", "file", "module", "namespace", "package", "class", "method", "property", "field", "constructor", "enum", "interface", "function", "variable", "constant", "string", "number", "boolean", "array", "object", "key", "null", "enum-member", "struct", "event", "operator", "type",
];

fn symbol_kind_name(k: u64, name: &str) -> &'static str {
    match k {
        19 if name.starts_with("impl") => "impl",
        _ => KINDS.get(k as usize).copied().unwrap_or("symbol"),
    }
}

/// `impl Trait for Foo<T>` -> `Foo`; `impl Foo` -> `Foo`. Takes a server's kind-19 name,
/// which carries no generic parameters of the impl itself (see `impl_name`).
fn impl_type(name: &str) -> String {
    bare_type(name.rsplit(" for ").next().unwrap_or(name).trim_start_matches("impl").trim())
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
            Some((lsp::relative(&lsp::uri_path(uri.as_str()?)?, root)?, pos["line"].as_u64()? as u32))
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

impl Index {
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
    pub fn apply(&mut self, r: ServerFile) {
        let Some(fi) = self.find_file(&r.path) else { return };
        let f = &mut self.files[fi];
        if f.hash != r.hash {
            return;
        }
        f.symbols = r.symbols;
        f.backend = Backend::Server;
        f.pending = false;
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
            report(&format!("{}: {n} files to index", lang.server));
            match start_server(&self.root, lang) {
                Ok((mut c, root)) => {
                    for (path, hash) in &files {
                        self.apply(index_file(&mut c, &root, path, *hash));
                    }
                    c.shutdown();
                }
                Err(e) => {
                    report(&format!("{e}: {n} files keep the tree-sitter resolver"));
                    self.give_up(lang);
                }
            }
        }
        self.link();
        self.save_cache();
    }
}
