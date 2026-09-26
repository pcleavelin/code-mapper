use std::fmt;

use crate::id::{Key, Position};
use crate::text::{Line, RelativePath, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(Position);

impl Key for FileId {
    fn at(position: Position) -> Self {
        Self(position)
    }

    fn position(self) -> Position {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolIndex(Position);

impl FileId {
    pub const fn number(self) -> usize {
        self.0.value()
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.value())
    }
}

impl Key for SymbolIndex {
    fn at(position: Position) -> Self {
        Self(position)
    }

    fn position(self) -> Position {
        self.0
    }
}

impl SymbolIndex {
    pub const fn number(self) -> usize {
        self.0.value()
    }
}

impl fmt::Display for SymbolIndex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0.value())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId {
    file: FileId,
    symbol: SymbolIndex,
}

impl SymbolId {
    pub(crate) const fn new(file: FileId, symbol: SymbolIndex) -> Self {
        Self { file, symbol }
    }

    pub const fn file(self) -> FileId {
        self.file
    }

    pub const fn symbol(self) -> SymbolIndex {
        self.symbol
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Depth(u32);

impl Depth {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }

    pub(crate) fn position(self) -> usize {
        usize::try_from(self.0).unwrap_or_default()
    }

    #[must_use]
    pub const fn deeper(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for Depth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolName(String);

impl SymbolName {
    pub fn new(name: &str) -> Self {
        Self(name.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SymbolName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolKind(String);

impl SymbolKind {
    pub fn new(kind: &str) -> Self {
        Self(kind.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn contains(&self, word: &str) -> bool {
        self.0.contains(word)
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TypeName(String);

impl TypeName {
    pub fn new(name: &str) -> Self {
        Self(name.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TypeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Scope(String);

impl Scope {
    pub fn new(name: &str) -> Self {
        Self(name.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Qualifier {
    Plain,
    SelfReference,
    Named(Scope),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Call {
    pub name: SymbolName,
    pub qualifier: Qualifier,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    pub file: RelativePath,
    pub line: Line,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolKey {
    pub file: RelativePath,
    pub name: SymbolName,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    pub from: SymbolId,
    pub to: SymbolId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    name: SymbolName,
    kind: SymbolKind,
    span: Span,
    depth: Depth,
    owner: Option<TypeName>,
    calls: Vec<Call>,
    targets: Vec<Location>,
    references: Vec<Location>,
    callees: Vec<SymbolId>,
    callers: Vec<SymbolId>,
}

impl Symbol {
    pub fn new(
        name: SymbolName,
        kind: SymbolKind,
        span: Span,
        depth: Depth,
        owner: Option<TypeName>,
        calls: Vec<Call>,
    ) -> Self {
        Self {
            name,
            kind,
            span,
            depth,
            owner,
            calls,
            targets: Vec::new(),
            references: Vec::new(),
            callees: Vec::new(),
            callers: Vec::new(),
        }
    }

    pub fn name(&self) -> &SymbolName {
        &self.name
    }

    pub fn kind(&self) -> &SymbolKind {
        &self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn depth(&self) -> Depth {
        self.depth
    }

    pub fn owner(&self) -> Option<&TypeName> {
        self.owner.as_ref()
    }

    pub fn calls(&self) -> &[Call] {
        &self.calls
    }

    pub fn targets(&self) -> &[Location] {
        &self.targets
    }

    pub fn references(&self) -> &[Location] {
        &self.references
    }

    pub fn callees(&self) -> &[SymbolId] {
        &self.callees
    }

    pub fn callers(&self) -> &[SymbolId] {
        &self.callers
    }

    pub fn set_targets(&mut self, targets: Vec<Location>) {
        self.targets = targets;
    }

    pub fn set_references(&mut self, references: Vec<Location>) {
        self.references = references;
    }

    pub fn set_callees(&mut self, callees: Vec<SymbolId>) {
        self.callees = callees;
    }

    pub fn set_callers(&mut self, callers: Vec<SymbolId>) {
        self.callers = callers;
    }

    pub(crate) fn clear_edges(&mut self) {
        self.callees.clear();
        self.callers.clear();
    }

    pub(crate) fn push_callee(&mut self, callee: SymbolId) {
        self.callees.push(callee);
    }

    pub(crate) fn push_caller(&mut self, caller: SymbolId) {
        self.callers.push(caller);
    }
}
