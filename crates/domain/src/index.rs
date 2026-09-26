mod file;
mod language;
mod symbol;

use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::mem;
use std::path::{Path, PathBuf};

use crate::id::{Entry, IdList};
use crate::text::{Line, RelativePath, Span};

pub use file::{Backend, Highlight, HighlightClass, Imports, Readiness, SourceFile};
pub use language::{Argument, Extension, Language, Program};
pub use symbol::{
    Call, Depth, Edge, FileId, Location, Qualifier, Scope, Symbol, SymbolId, SymbolIndex,
    SymbolKey, SymbolKind, SymbolName, TypeName,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Root(PathBuf);

impl Root {
    pub fn new(path: &Path) -> Self {
        Self(path.to_path_buf())
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, path: &RelativePath) -> PathBuf {
        self.0.join(path.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolQuery {
    name: SymbolName,
    file: Option<Scope>,
    owner: Option<Scope>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MarkName(&'static str);

impl MarkName {
    const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Path,
}

impl Mark {
    const fn name(self) -> MarkName {
        MarkName(match self {
            Self::Path => "::",
        })
    }
}

impl From<&str> for SymbolQuery {
    fn from(query: &str) -> Self {
        let (qualifier, name) = match query
            .rsplit_once(Mark::Path.name().as_str())
            .or_else(|| query.rsplit_once(':'))
        {
            Some((qualifier, name)) => (Some(qualifier.replace('\\', "/")), name),
            None => (None, query),
        };
        let (file, owner) = match qualifier.as_deref() {
            None => (None, None),
            Some(qualifier) => match qualifier.split_once(':') {
                Some((file, owner)) if !file.is_empty() && !owner.is_empty() => {
                    (Some(Scope::new(file)), Some(Scope::new(owner)))
                }
                _ => (None, Some(Scope::new(qualifier))),
            },
        };
        Self {
            name: SymbolName::new(name),
            file,
            owner,
        }
    }
}

impl SymbolQuery {
    pub fn name(&self) -> &SymbolName {
        &self.name
    }

    fn matches(&self, file: &SourceFile, symbol: &Symbol) -> bool {
        if symbol.name() != &self.name {
            return false;
        }
        let in_file = |scope: &Scope| {
            file.path().stem() == scope.as_str() || file.path().ends_with(scope.as_str())
        };
        let owner_is = |scope: &Scope| symbol.owner().map(TypeName::as_str) == Some(scope.as_str());
        match (&self.file, &self.owner) {
            (None, None) => true,
            (Some(file_scope), Some(owner_scope)) => in_file(file_scope) && owner_is(owner_scope),
            (None, Some(scope)) => owner_is(scope) || in_file(scope),
            (Some(file_scope), None) => in_file(file_scope),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeEntry {
    pub symbol: SymbolId,
    pub depth: Depth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingBatch {
    pub language: Language,
    pub files: Vec<FileId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    root: Root,
    files: IdList<FileId, SourceFile>,
}

impl Index {
    pub fn new(root: Root) -> Self {
        Self {
            root,
            files: IdList::new(),
        }
    }

    pub fn root(&self) -> &Root {
        &self.root
    }

    pub fn push(&mut self, file: SourceFile) -> FileId {
        self.files.push(file)
    }

    pub fn file(&self, file: FileId) -> Option<&SourceFile> {
        self.files.get(file)
    }

    pub fn file_mut(&mut self, file: FileId) -> Option<&mut SourceFile> {
        self.files.get_mut(file)
    }

    pub fn replace(&mut self, file: FileId, source: SourceFile) -> Option<SourceFile> {
        self.files
            .get_mut(file)
            .map(|slot| mem::replace(slot, source))
    }

    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.files.iter()
    }

    pub fn files_mut(&mut self) -> impl Iterator<Item = &mut SourceFile> {
        self.files.iter_mut()
    }

    pub fn file_entries(&self) -> impl Iterator<Item = Entry<FileId, &SourceFile>> {
        self.files.entries()
    }

    pub fn symbol_ids(&self) -> impl Iterator<Item = SymbolId> {
        self.files.entries().flat_map(|file| {
            file.item
                .symbol_entries()
                .map(move |symbol| SymbolId::new(file.id, symbol.id))
        })
    }

    pub fn find_file(&self, path: &RelativePath) -> Option<FileId> {
        self.files
            .entries()
            .find(|entry| entry.item.path() == path)
            .map(|entry| entry.id)
    }

    pub fn symbol(&self, symbol: SymbolId) -> Option<&Symbol> {
        self.files.get(symbol.file())?.symbol(symbol.symbol())
    }

    pub fn symbol_mut(&mut self, symbol: SymbolId) -> Option<&mut Symbol> {
        self.files
            .get_mut(symbol.file())?
            .symbol_mut(symbol.symbol())
    }

    pub fn symbol_key(&self, symbol: SymbolId) -> Option<SymbolKey> {
        Some(SymbolKey {
            file: self.files.get(symbol.file())?.path().clone(),
            name: self.symbol(symbol)?.name().clone(),
        })
    }

    pub fn by_key(&self, key: &SymbolKey) -> Option<SymbolId> {
        let file = self.find_file(&key.file)?;
        let symbol = self.files.get(file)?.first_named(&key.name)?;
        Some(SymbolId::new(file, symbol))
    }

    pub fn by_line(&self, path: &RelativePath, line: Line) -> Option<SymbolId> {
        self.symbol_at(self.find_file(path)?, line)
    }

    pub fn symbol_at(&self, file: FileId, line: Line) -> Option<SymbolId> {
        let symbol = self.files.get(file)?.innermost(Span::line(line))?;
        Some(SymbolId::new(file, symbol))
    }

    pub fn find_symbols(&self, query: &SymbolQuery) -> Vec<SymbolId> {
        self.files
            .entries()
            .flat_map(|file| {
                file.item
                    .symbol_entries()
                    .filter(move |symbol| query.matches(file.item, symbol.item))
                    .map(move |symbol| SymbolId::new(file.id, symbol.id))
            })
            .collect()
    }

    pub fn roots(&self) -> Vec<SymbolId> {
        let mut roots: Vec<SymbolId> = self
            .symbol_ids()
            .filter(|id| {
                self.symbol(*id).is_some_and(|symbol| {
                    !symbol.callees().is_empty() && symbol.callers().is_empty()
                })
            })
            .collect();
        roots
            .sort_by_key(|id| Reverse(self.symbol(*id).map_or(0, |symbol| symbol.callees().len())));
        roots
    }

    pub fn call_tree(&self, root: SymbolId, deepest: Depth) -> Vec<TreeEntry> {
        let mut tree = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![TreeEntry {
            symbol: root,
            depth: Depth::default(),
        }];
        while let Some(entry) = stack.pop() {
            if !seen.insert(entry.symbol) {
                continue;
            }
            tree.push(entry);
            if entry.depth < deepest
                && let Some(symbol) = self.symbol(entry.symbol)
            {
                stack.extend(symbol.callees().iter().rev().map(|callee| TreeEntry {
                    symbol: *callee,
                    depth: entry.depth.deeper(),
                }));
            }
        }
        tree
    }

    pub fn call_site(&self, file: FileId, line: Line, name: &SymbolName) -> bool {
        self.files
            .get(file)
            .is_some_and(|source| source.call_site(line, name))
    }

    pub fn connect(&mut self, edges: &[Edge]) {
        for file in self.files.iter_mut() {
            for symbol in file.symbols_mut() {
                symbol.clear_edges();
            }
        }
        for edge in edges {
            if let Some(from) = self.symbol_mut(edge.from) {
                from.push_callee(edge.to);
            }
            if let Some(to) = self.symbol_mut(edge.to) {
                to.push_caller(edge.from);
            }
        }
    }

    #[must_use]
    pub fn symbols_only(&self) -> Self {
        Self {
            root: self.root.clone(),
            files: self.files.iter().map(SourceFile::without_text).collect(),
        }
    }

    pub fn take_edges(&mut self, linked: &Self) {
        let same = {
            let mut own_files = self.files.iter();
            let mut other_files = linked.files.iter();
            loop {
                match (own_files.next(), other_files.next()) {
                    (None, None) => break true,
                    (Some(one), Some(other))
                        if one.hash() == other.hash()
                            && one.backend() == other.backend()
                            && one.readiness() == other.readiness()
                            && one.symbols().count() == other.symbols().count() => {}
                    _ => break false,
                }
            }
        };
        if !same {
            return;
        }
        for (file, other) in self.files.iter_mut().zip(linked.files.iter()) {
            for (symbol, source) in file.symbols_mut().zip(other.symbols()) {
                symbol.set_callees(source.callees().to_vec());
                symbol.set_callers(source.callers().to_vec());
            }
        }
    }

    pub fn pending(&self) -> Vec<PendingBatch> {
        let mut batches: Vec<PendingBatch> = Vec::new();
        for entry in self.files.entries().filter(|entry| entry.item.is_pending()) {
            let Some(language) = entry.item.language() else {
                continue;
            };
            match batches.iter_mut().find(|batch| batch.language == language) {
                Some(batch) => batch.files.push(entry.id),
                None => batches.push(PendingBatch {
                    language,
                    files: vec![entry.id],
                }),
            }
        }
        batches
    }

    pub fn give_up(&mut self, language: Language) {
        for file in self.files.iter_mut() {
            if file.language() == Some(language) {
                file.set_readiness(Readiness::Ready);
            }
        }
    }
}

#[cfg(test)]
mod tests;
