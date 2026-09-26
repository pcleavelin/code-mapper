use std::collections::BTreeMap;

use crate::id::{Entry, IdList};
use crate::index::language::Language;
use crate::index::symbol::{Symbol, SymbolIndex, SymbolName};
use crate::text::{ByteOffset, FileText, Line, RelativePath, SourceLine, Span, TextHash};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HighlightClass {
    Plain,
    Keyword,
    String,
    Comment,
    Function,
    Type,
    Constant,
    Property,
}

impl HighlightClass {
    pub const ALL: [Self; 8] = [
        Self::Plain,
        Self::Keyword,
        Self::String,
        Self::Comment,
        Self::Function,
        Self::Type,
        Self::Constant,
        Self::Property,
    ];

    pub const fn is_quoted(self) -> bool {
        matches!(self, Self::Comment | Self::String)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Highlight {
    pub start: ByteOffset,
    pub end: ByteOffset,
    pub class: HighlightClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Backend {
    TreeSitter,
    Server,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Readiness {
    Ready,
    Pending,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Imports(BTreeMap<String, String>);

impl Imports {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: &str, module: &str) {
        self.0.insert(name.to_owned(), module.to_owned());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn count(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|pair| (pair.0.as_str(), pair.1.as_str()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    path: RelativePath,
    text: FileText,
    highlights: Vec<Vec<Highlight>>,
    symbols: IdList<SymbolIndex, Symbol>,
    imports: Imports,
    hash: TextHash,
    backend: Backend,
    readiness: Readiness,
}

impl SourceFile {
    pub fn new(
        path: RelativePath,
        text: FileText,
        highlights: Vec<Vec<Highlight>>,
        symbols: Vec<Symbol>,
        imports: Imports,
        hash: TextHash,
        backend: Backend,
    ) -> Self {
        Self {
            path,
            text,
            highlights,
            symbols: symbols.into_iter().collect(),
            imports,
            hash,
            backend,
            readiness: Readiness::Ready,
        }
    }

    pub fn path(&self) -> &RelativePath {
        &self.path
    }

    pub fn text(&self) -> &FileText {
        &self.text
    }

    pub fn highlights(&self) -> &[Vec<Highlight>] {
        &self.highlights
    }

    pub fn highlights_on(&self, line: Line) -> &[Highlight] {
        self.highlights
            .get(line.position())
            .map_or(&[], Vec::as_slice)
    }

    pub fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    pub fn symbol_entries(&self) -> impl Iterator<Item = Entry<SymbolIndex, &Symbol>> {
        self.symbols.entries()
    }

    pub fn symbol(&self, symbol: SymbolIndex) -> Option<&Symbol> {
        self.symbols.get(symbol)
    }

    pub fn symbol_mut(&mut self, symbol: SymbolIndex) -> Option<&mut Symbol> {
        self.symbols.get_mut(symbol)
    }

    pub fn symbols_mut(&mut self) -> impl Iterator<Item = &mut Symbol> {
        self.symbols.iter_mut()
    }

    pub fn imports(&self) -> &Imports {
        &self.imports
    }

    pub fn hash(&self) -> TextHash {
        self.hash
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn readiness(&self) -> Readiness {
        self.readiness
    }

    pub fn is_pending(&self) -> bool {
        self.readiness == Readiness::Pending
    }

    pub fn language(&self) -> Option<Language> {
        Language::of(&self.path)
    }

    pub fn set_text(&mut self, text: FileText) {
        let count = text.all().len();
        self.text = text;
        self.highlights.resize(count, Vec::new());
    }

    pub fn set_symbols(&mut self, symbols: Vec<Symbol>) {
        self.symbols = symbols.into_iter().collect();
    }

    pub fn set_backend(&mut self, backend: Backend) {
        self.backend = backend;
    }

    pub fn set_readiness(&mut self, readiness: Readiness) {
        self.readiness = readiness;
    }

    #[must_use]
    pub fn without_text(&self) -> Self {
        Self {
            path: self.path.clone(),
            text: FileText::default(),
            highlights: Vec::new(),
            symbols: self.symbols.clone(),
            imports: self.imports.clone(),
            hash: self.hash,
            backend: self.backend,
            readiness: self.readiness,
        }
    }

    pub fn innermost(&self, span: Span) -> Option<SymbolIndex> {
        self.symbols
            .entries()
            .filter(|entry| entry.item.span().encloses(span))
            .max_by_key(|entry| entry.item.depth())
            .map(|entry| entry.id)
    }

    pub fn first_named(&self, name: &SymbolName) -> Option<SymbolIndex> {
        self.symbols
            .entries()
            .find(|entry| entry.item.name() == name)
            .map(|entry| entry.id)
    }

    pub fn call_site(&self, line: Line, name: &SymbolName) -> bool {
        let Some(text) = self.text.line(line).map(SourceLine::as_str) else {
            return false;
        };
        let word = name.as_str();
        let highlights = self.highlights_on(line);
        let identifier = |character: char| character.is_alphanumeric() || character == '_';
        text.match_indices(word).any(|found| {
            let at = found.0;
            let before = text
                .get(..at)
                .and_then(|head| head.chars().next_back())
                .is_some_and(identifier);
            let after = text
                .get(at + word.len()..)
                .and_then(|tail| tail.chars().next())
                .is_some_and(identifier);
            let quoted = highlights.iter().any(|highlight| {
                highlight.class.is_quoted()
                    && highlight.start.position() <= at
                    && at < highlight.end.position()
            });
            !before && !after && !quoted
        })
    }
}
