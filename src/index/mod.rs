//! The auto layer: every file under the root, its symbols and highlight spans, and the call
//! graph between symbols. Each language is indexed by its server when one is on PATH
//! (`server`), by tree-sitter otherwise (`treesitter`); `link` turns either backend's answer
//! into callee and caller lists, and `cache` keeps the result between runs.

mod cache;
mod link;
mod server;
mod treesitter;

pub use server::{ServerFile, Servers, index_files, name_position, references_at, start_server};
pub use treesitter::{Parsers, parse_file};

use crate::codec::fnv1a;
use cache::{CACHE, load_cache};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

#[derive(Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub start: usize, // inclusive 0-based line rows
    pub end: usize,
    pub depth: u8,
    pub owner: Option<String>, // the type / class a member belongs to (impl Foo -> "Foo")
    pub calls: Vec<Call>,      // tree-sitter backend: raw call sites, resolved by name in `link`
    pub targets: Vec<(String, u32)>, // server backend: (file, line) of each callee's definition, resolved in `link`
    pub refs: Vec<(String, u32)>, // (file, line) of every reference, once the GUI has asked the server for them
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
    Lang {
        exts: &["rs"],
        server: "rust-analyzer",
        args: &[],
    },
    Lang {
        exts: &["odin"],
        server: "ols",
        args: &[],
    },
    Lang {
        exts: &["c", "h"],
        server: "clangd",
        args: &[],
    },
    Lang {
        exts: &["py"],
        server: "pyright-langserver",
        args: &["--stdio"],
    },
    Lang {
        exts: &["js", "mjs", "cjs", "ts", "tsx"],
        server: "typescript-language-server",
        args: &["--stdio"],
    },
];

/// The language of `path`, by extension. Each has a bundled grammar (`language_for`), so a
/// file with a language has symbols even without its server.
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

/// Walks the root. A file whose text matches its cache entry is taken from the cache without
/// parsing; anything else is parsed with tree-sitter now and, if its language has a server,
/// marked pending for it. A server answer names lines in other files, so a cached answer is
/// also pending again when any file it points into changed.
// ponytail: reads the whole tree into memory up front on the main thread; move to a background
// thread with a progress bar when startup on a big repo becomes annoying.
pub fn build(root: &Path) -> Index {
    let mut parsers = Parsers::default();
    let mut cache = load_cache(&root.join(CACHE)).unwrap_or_default();
    let mut changed: HashSet<String> = HashSet::new();
    let mut files = Vec::new();

    // require_git(false): honour .gitignore even when the root is not a git repo (target/ etc.)
    for entry in ignore::WalkBuilder::new(root)
        .require_git(false)
        .build()
        .flatten()
    {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(raw) = std::fs::read(entry.path()) else {
            continue;
        };
        if raw.len() > MAX_FILE || raw[..raw.len().min(1024)].contains(&0) {
            continue; // too big or binary
        }

        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        // ponytail: tabs become 4 spaces once, for display and hashing alike
        let text = String::from_utf8_lossy(&raw).replace('\t', "    ");
        let hash = fnv1a(text.bytes());
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
        if f.backend == Backend::Server
            && f.symbols.iter().any(|s| {
                s.targets
                    .iter()
                    .chain(&s.refs)
                    .any(|(p, _)| changed.contains(p))
            })
        {
            f.pending = true;
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut idx = Index {
        root: root.to_path_buf(),
        files,
    };
    idx.link();
    if !changed.is_empty() || !cache.is_empty() {
        idx.save_cache(); // parsed something, or a cached file is gone
    }
    idx
}

/// `word` appears on line `li` of `f` as a whole identifier outside comments and strings: a
/// place that names it in code.
pub fn call_site(f: &File, li: usize, word: &str) -> bool {
    let Some(line) = f.lines.get(li) else {
        return false;
    };
    let is_id = |c: char| c.is_alphanumeric() || c == '_';
    let spans = f.hl.get(li).map(Vec::as_slice).unwrap_or(&[]);
    line.match_indices(word).any(|(i, _)| {
        let whole = !line[..i].chars().next_back().is_some_and(is_id)
            && !line[i + word.len()..].chars().next().is_some_and(is_id);
        let quoted = spans.iter().any(|&(s, e, class)| {
            (class == HL_COMMENT || class == HL_STRING) && (s as usize) <= i && i < e as usize
        });
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
        let sym = self.files[file]
            .symbols
            .iter()
            .position(|s| s.name == key.1)?;
        Some(SymRef { file, sym })
    }

    /// The innermost symbol of `path` containing `line`.
    pub fn by_line(&self, path: &str, line: usize) -> Option<SymRef> {
        self.sym_at(self.find_file(path)?, line)
    }

    /// The innermost symbol of file `file` spanning `line`.
    fn sym_at(&self, file: usize, line: usize) -> Option<SymRef> {
        let sym = self.files[file]
            .symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| s.start <= line && line <= s.end)
            .max_by_key(|(_, s)| s.depth)?
            .0;
        Some(SymRef { file, sym })
    }

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

    fn walk(
        &self,
        r: SymRef,
        depth: usize,
        max: usize,
        seen: &mut HashSet<SymRef>,
        out: &mut Vec<(SymRef, usize)>,
    ) {
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
