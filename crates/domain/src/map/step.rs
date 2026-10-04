use std::fmt;

use crate::id::ShortId;
use crate::index::{Index, SourceFile, SymbolId, SymbolName};
use crate::map::name::{Author, Note, TourName};
use crate::text::{Line, LineOffset, RelativePath, Span, TextHash};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepId(String);

impl StepId {
    const DIGITS: usize = 6;

    pub fn new(id: &str) -> Option<Self> {
        ShortId::valid(id, Self::DIGITS).then(|| Self(id.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn fresh(seed: &str, taken: impl Fn(&Self) -> bool) -> Self {
        Self(ShortId::fresh(seed, Self::DIGITS, |id| taken(&Self(id.to_owned()))).into_text())
    }
}

impl fmt::Display for StepId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepOrder(u32);

impl StepOrder {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for StepOrder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Anchor {
    file: RelativePath,
    symbol: Option<SymbolName>,
    start: LineOffset,
    end: LineOffset,
    hash: TextHash,
}

impl Anchor {
    pub fn new(
        file: RelativePath,
        symbol: Option<SymbolName>,
        start: LineOffset,
        end: LineOffset,
        hash: TextHash,
    ) -> Self {
        Self {
            file,
            symbol,
            start,
            end,
            hash,
        }
    }

    pub fn at(file: &SourceFile, span: Span) -> Option<Self> {
        let hash = file.text().hash(span)?;
        let enclosing = file.innermost(span).and_then(|symbol| file.symbol(symbol));
        let base = enclosing.map_or(Line::new(0), |symbol| symbol.span().start());
        Some(Self {
            file: file.path().clone(),
            symbol: enclosing.map(|symbol| symbol.name().clone()),
            start: span.start().offset_from(base)?,
            end: span.end().offset_from(base)?,
            hash,
        })
    }

    pub fn file(&self) -> &RelativePath {
        &self.file
    }

    pub fn symbol(&self) -> Option<&SymbolName> {
        self.symbol.as_ref()
    }

    pub fn start(&self) -> LineOffset {
        self.start
    }

    pub fn end(&self) -> LineOffset {
        self.end
    }

    pub fn hash(&self) -> TextHash {
        self.hash
    }

    pub fn resolve(&self, index: &Index) -> Option<Resolution> {
        let file_id = index.find_file(&self.file)?;
        let file = index.file(file_id)?;
        let candidates: Vec<Option<SymbolId>> = match &self.symbol {
            None => vec![None],
            Some(name) => file
                .symbol_entries()
                .filter(|entry| entry.item.name() == name)
                .map(|entry| Some(SymbolId::new(file_id, entry.id)))
                .collect(),
        };
        let mut best: Option<Resolution> = None;
        for candidate in candidates {
            let base = candidate
                .and_then(|symbol| index.symbol(symbol))
                .map_or(Line::new(0), |symbol| symbol.span().start());
            let Some(span) = base
                .shifted(self.start)
                .zip(base.shifted(self.end))
                .and_then(|lines| Span::new(lines.0, lines.1))
            else {
                continue;
            };
            let Some(hash) = file.text().hash(span) else {
                continue;
            };
            let freshness = if hash == self.hash {
                Freshness::Current
            } else {
                Freshness::Stale
            };
            if best.is_none() || freshness == Freshness::Current {
                best = Some(Resolution {
                    span,
                    freshness,
                    symbol: candidate,
                });
            }
            if freshness == Freshness::Current {
                break;
            }
        }
        best
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Freshness {
    Current,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Resolution {
    pub span: Span,
    pub freshness: Freshness,
    pub symbol: Option<SymbolId>,
}

impl Default for Resolution {
    fn default() -> Self {
        Self {
            span: Span::line(Line::new(0)),
            freshness: Freshness::Stale,
            symbol: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    id: StepId,
    order: StepOrder,
    parent: Option<StepId>,
    author: Author,
    anchor: Anchor,
    note: Option<Note>,
    link: Option<TourName>,
    resolution: Resolution,
}

impl Step {
    pub fn new(
        id: StepId,
        order: StepOrder,
        parent: Option<StepId>,
        author: Author,
        anchor: Anchor,
        note: Option<Note>,
        link: Option<TourName>,
    ) -> Self {
        Self {
            id,
            order,
            parent,
            author,
            anchor,
            note,
            link,
            resolution: Resolution::default(),
        }
    }

    pub(crate) fn fresh(
        id: StepId,
        author: Author,
        anchor: Anchor,
        resolution: Resolution,
    ) -> Self {
        Self {
            id,
            order: StepOrder::default(),
            parent: None,
            author,
            anchor,
            note: None,
            link: None,
            resolution,
        }
    }

    pub fn id(&self) -> &StepId {
        &self.id
    }

    pub fn order(&self) -> StepOrder {
        self.order
    }

    pub fn parent(&self) -> Option<&StepId> {
        self.parent.as_ref()
    }

    pub fn author(&self) -> Author {
        self.author
    }

    pub fn anchor(&self) -> &Anchor {
        &self.anchor
    }

    pub fn file(&self) -> &RelativePath {
        &self.anchor.file
    }

    pub fn symbol(&self) -> Option<&SymbolName> {
        self.anchor.symbol.as_ref()
    }

    pub fn note(&self) -> Option<&Note> {
        self.note.as_ref()
    }

    pub fn link(&self) -> Option<&TourName> {
        self.link.as_ref()
    }

    pub fn resolution(&self) -> Resolution {
        self.resolution
    }

    pub fn span(&self) -> Span {
        self.resolution.span
    }

    pub fn is_stale(&self) -> bool {
        self.resolution.freshness == Freshness::Stale
    }

    pub fn resolved_symbol(&self) -> Option<SymbolId> {
        self.resolution.symbol
    }

    pub fn resolve(&mut self, index: &Index) {
        if let Some(resolution) = self.anchor.resolve(index) {
            self.resolution = resolution;
        } else {
            self.resolution.freshness = Freshness::Stale;
            self.resolution.symbol = None;
        }
    }

    pub(crate) fn set_order(&mut self, order: StepOrder) {
        self.order = order;
    }

    pub(crate) fn set_parent(&mut self, parent: Option<StepId>) {
        self.parent = parent;
    }

    pub(crate) fn set_note(&mut self, note: Option<Note>) {
        self.note = note;
    }

    pub(crate) fn set_link(&mut self, link: Option<TourName>) {
        self.link = link;
    }

    pub(crate) fn repin(&mut self, author: Author, anchor: Anchor, resolution: Resolution) {
        self.author = author;
        self.anchor = anchor;
        self.resolution = resolution;
    }
}
