//! The server backend: files' symbols and outgoing calls asked of the language's server a batch
//! at a time, one symbol's callers and references asked when they are wanted, and the
//! bookkeeping of which files still wait for a server.

use super::{Backend, Index, Lang, SymRef, Symbol, lang_for, treesitter::bare_type};
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
    "symbol",
    "file",
    "module",
    "namespace",
    "package",
    "class",
    "method",
    "property",
    "field",
    "constructor",
    "enum",
    "interface",
    "function",
    "variable",
    "constant",
    "string",
    "number",
    "boolean",
    "array",
    "object",
    "key",
    "null",
    "enum-member",
    "struct",
    "event",
    "operator",
    "type",
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
    bare_type(
        name.rsplit(" for ")
            .next()
            .unwrap_or(name)
            .trim_start_matches("impl")
            .trim(),
    )
}

/// One DocumentSymbol and, for a container, its direct children. Records where each symbol's
/// name sits so call-hierarchy and reference requests can point at it.
fn push_symbol(
    v: &Value,
    depth: u8,
    owner: Option<&str>,
    out: &mut Vec<Symbol>,
    sel: &mut Vec<(u64, u64)>,
) {
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
    let own: Option<String> = if container {
        Some(if k == 19 {
            impl_type(&name)
        } else {
            name.clone()
        })
    } else {
        owner.map(str::to_owned)
    };
    sel.push((
        p["line"].as_u64().unwrap_or(0),
        p["character"].as_u64().unwrap_or(0),
    ));
    out.push(Symbol {
        kind: symbol_kind_name(k, &name).to_owned(),
        name,
        start,
        end,
        depth,
        owner: own.clone(),
        calls: Vec::new(),
        targets: Vec::new(),
        refs: Vec::new(),
        callees: Vec::new(),
        callers: Vec::new(),
    });
    if container {
        for c in v["children"].as_array().into_iter().flatten() {
            push_symbol(c, 1, own.as_deref(), out, sel);
        }
    }
}

/// (file, line) for each element of a Location-like array, files outside the root dropped.
fn locations(
    v: &Value,
    root: &Path,
    item: impl Fn(&Value) -> (&Value, &Value),
) -> Vec<(String, u32)> {
    let mut out: Vec<(String, u32)> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| {
            let (uri, pos) = item(l);
            Some((
                lsp::relative(&lsp::uri_path(uri.as_str()?)?, root)?,
                pos["line"].as_u64()? as u32,
            ))
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
    let exe =
        lsp::find_on_path(lang.server).ok_or_else(|| format!("{} not on PATH", lang.server))?;
    let root = std::path::absolute(root).map_err(|e| e.to_string())?;
    let mut c = lsp::Client::start(&exe, lang.args, &root)
        .ok_or_else(|| format!("{} would not start", lang.server))?;
    c.wait_ready(Duration::from_secs(300));
    Ok((c, root))
}

/// Symbols and outgoing calls for a batch of files from a running server. Each stage (the
/// files' symbols, every symbol's call-hierarchy item, every item's outgoing calls) is sent
/// whole, so the server works on many requests at once.
pub fn index_files(c: &mut lsp::Client, root: &Path, files: &[(String, u64)]) -> Vec<ServerFile> {
    let docs: Vec<Value> = files
        .iter()
        .map(|(p, _)| json!({"uri": lsp::to_uri(&root.join(p))}))
        .collect();
    let answers = c.request_all(
        docs.iter()
            .map(|d| ("textDocument/documentSymbol", json!({"textDocument": d})))
            .collect(),
    );
    let mut per_file: Vec<(Vec<Symbol>, Vec<(u64, u64)>)> = Vec::new();
    for a in answers {
        let (mut symbols, mut sel) = (Vec::new(), Vec::new());
        for s in a.unwrap_or(Value::Null).as_array().into_iter().flatten() {
            push_symbol(s, 0, None, &mut symbols, &mut sel);
        }
        per_file.push((symbols, sel));
    }
    // (file, symbol) of every request in the next two stages, in the order sent
    let mut who = Vec::new();
    let mut reqs = Vec::new();
    for (fi, (_, sel)) in per_file.iter().enumerate() {
        for (si, (line, ch)) in sel.iter().enumerate() {
            who.push((fi, si));
            reqs.push((
                "textDocument/prepareCallHierarchy",
                json!({"textDocument": docs[fi], "position": {"line": line, "character": ch}}),
            ));
        }
    }
    let items = c.request_all(reqs);
    let (mut owner, mut reqs) = (Vec::new(), Vec::new());
    for (w, item) in who.into_iter().zip(items) {
        for it in item.unwrap_or(Value::Null).as_array().into_iter().flatten() {
            owner.push(w);
            reqs.push(("callHierarchy/outgoingCalls", json!({"item": it})));
        }
    }
    for ((fi, si), calls) in owner.into_iter().zip(c.request_all(reqs)) {
        let calls = calls.unwrap_or(Value::Null);
        per_file[fi].0[si]
            .targets
            .extend(locations(&calls, root, |call| {
                (&call["to"]["uri"], &call["to"]["selectionRange"]["start"])
            }));
    }
    files
        .iter()
        .zip(per_file)
        .map(|((path, hash), (mut symbols, _))| {
            for s in &mut symbols {
                s.targets.sort();
                s.targets.dedup();
            }
            ServerFile {
                path: path.clone(),
                hash: *hash,
                symbols,
            }
        })
        .collect()
}

/// Where a symbol's name sits, as a server counts it: the symbol's first line, which is the
/// line of its name, and the UTF-16 column of the name on it (0 when the name is not there
/// as written, as with an impl block).
pub fn name_position(idx: &Index, r: SymRef) -> (u32, u32) {
    let (f, s) = (&idx.files[r.file], idx.sym(r));
    let line = f.lines.get(s.start).map_or("", String::as_str);
    let col = line
        .find(&s.name)
        .map_or(0, |b| line[..b].encode_utf16().count());
    (s.start as u32, col as u32)
}

/// Every reference to `r`, as (file, line) inside the root, from a running server.
pub fn references(c: &mut lsp::Client, root: &Path, idx: &Index, r: SymRef) -> Vec<(String, u32)> {
    let (line, col) = name_position(idx, r);
    references_at(c, root, &idx.files[r.file].path, line, col)
}

/// Every reference to the name at (`line`, `col`) of `path`, as (file, line) inside the root.
pub fn references_at(
    c: &mut lsp::Client,
    root: &Path,
    path: &str,
    line: u32,
    col: u32,
) -> Vec<(String, u32)> {
    let at = json!({"textDocument": {"uri": lsp::to_uri(&root.join(path))}, "position": {"line": line, "character": col}, "context": {"includeDeclaration": false}});
    let refs = c
        .request("textDocument/references", at)
        .unwrap_or(Value::Null);
    locations(&refs, root, |l| (&l["uri"], &l["range"]["start"]))
}

/// Every place that calls `r`, as the (file, line) of the caller's name inside the root, from
/// a running server.
pub fn incoming_calls(
    c: &mut lsp::Client,
    root: &Path,
    idx: &Index,
    r: SymRef,
) -> Vec<(String, u32)> {
    let (line, col) = name_position(idx, r);
    let at = json!({"textDocument": {"uri": lsp::to_uri(&root.join(&idx.files[r.file].path))}, "position": {"line": line, "character": col}});
    let items = c
        .request("textDocument/prepareCallHierarchy", at)
        .unwrap_or(Value::Null);
    let reqs: Vec<(&str, Value)> = items
        .as_array()
        .into_iter()
        .flatten()
        .map(|it| ("callHierarchy/incomingCalls", json!({"item": it})))
        .collect();
    let mut out: Vec<(String, u32)> = c
        .request_all(reqs)
        .into_iter()
        .flat_map(|calls| {
            locations(&calls.unwrap_or(Value::Null), root, |call| {
                (
                    &call["from"]["uri"],
                    &call["from"]["selectionRange"]["start"],
                )
            })
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The CLI's servers: each language's started the first time a command needs it and kept
/// for the rest of the command, so a command that asks several things starts it once.
pub struct Servers {
    root: PathBuf,
    live: std::collections::HashMap<&'static str, Option<(lsp::Client, PathBuf)>>,
    report: Box<dyn FnMut(&str)>,
}

impl Servers {
    pub fn new(root: &Path, report: impl FnMut(&str) + 'static) -> Servers {
        Servers {
            root: root.to_owned(),
            live: Default::default(),
            report: Box::new(report),
        }
    }

    /// The running server for `lang`, started now if it is not yet. None when it is not on
    /// PATH or will not start, which is reported once.
    fn client(&mut self, lang: &'static Lang) -> Option<&mut (lsp::Client, PathBuf)> {
        if !self.live.contains_key(lang.server) {
            let started = start_server(&self.root, lang);
            if let Err(e) = &started {
                (self.report)(&format!("{e}: its files keep the tree-sitter resolver"));
            }
            self.live.insert(lang.server, started.ok());
        }
        self.live.get_mut(lang.server).and_then(Option::as_mut)
    }

    /// Asks the servers for the files among `paths` that still wait for one, then re-links and
    /// saves the cache when anything was answered. Files of a language with no server keep the
    /// tree-sitter answer.
    pub fn index(&mut self, idx: &mut Index, paths: &[String]) {
        let want: std::collections::HashSet<&str> = paths.iter().map(String::as_str).collect();
        let mut answered = false;
        for (lang, files) in idx.pending() {
            let files: Vec<(String, u64)> = files
                .into_iter()
                .filter(|(p, _)| want.contains(p.as_str()))
                .collect();
            if files.is_empty() {
                continue;
            }
            (self.report)(&format!("{}: {} files to index", lang.server, files.len()));
            let Some((c, root)) = self.client(lang) else {
                idx.give_up(lang);
                continue;
            };
            let root = root.clone();
            let mut done = Vec::new();
            for chunk in files.chunks(32) {
                done.extend(index_files(c, &root, chunk));
            }
            for f in done {
                idx.apply(f);
                answered = true;
            }
        }
        if answered {
            idx.link();
            idx.save_cache();
        }
    }

    /// Where `r` is called from, asked of its language's server; None when there is none.
    pub fn incoming_calls(&mut self, idx: &Index, r: SymRef) -> Option<Vec<(String, u32)>> {
        let lang = lang_for(&idx.files[r.file].path)?;
        let (c, root) = self.client(lang)?;
        let root = root.clone();
        Some(incoming_calls(c, &root, idx, r))
    }

    /// Every reference to `r`, asked of its language's server; None when there is none.
    pub fn references(&mut self, idx: &Index, r: SymRef) -> Option<Vec<(String, u32)>> {
        let lang = lang_for(&idx.files[r.file].path)?;
        let (c, root) = self.client(lang)?;
        let root = root.clone();
        Some(references(c, &root, idx, r))
    }
}

impl Drop for Servers {
    fn drop(&mut self) {
        for (c, _) in self.live.drain().filter_map(|(_, v)| v) {
            c.shutdown();
        }
    }
}

impl Index {
    /// Files waiting on a server, grouped by language.
    pub fn pending(&self) -> Vec<(&'static Lang, Vec<(String, u64)>)> {
        let mut out: Vec<(&'static Lang, Vec<(String, u64)>)> = Vec::new();
        for f in self.files.iter().filter(|f| f.pending) {
            let Some(lang) = lang_for(&f.path) else {
                continue;
            };
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
        let Some(fi) = self.find_file(&r.path) else {
            return;
        };
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
}
