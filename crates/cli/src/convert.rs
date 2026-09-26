use std::fmt;

use domain::{
    Depth, GroupName, Line, MapError, Note, Path, PathKind, PathName, RelativePath, Revision,
    SourceFile, Span, StepId, SymbolName, TextFragment,
};

use crate::wire::{
    Command, FilterArguments, GroupRenameArguments, NoteEditArguments, PathAddArguments,
    PathArguments, PathGroupArguments, PathMoveArguments, PathNewArguments, PathNoteArguments,
    PathPinArguments, PathRenameArguments, PathRmArguments, PathSwapArguments, PromoteArguments,
    RegexArguments, ShowArguments, StepArguments, StepLinkArguments, StepNoteArguments,
    SymbolArguments, TreeArguments,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Filter(String);

impl Filter {
    pub(crate) fn new(filter: Option<String>) -> Self {
        Self(filter.unwrap_or_default())
    }

    pub(crate) fn matches(&self, text: &str) -> bool {
        text.contains(self.0.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepIndex(usize);

impl StepIndex {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub(crate) const fn value(self) -> usize {
        self.0
    }
}

impl fmt::Display for StepIndex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Under(i64);

impl Under {
    pub(crate) const fn new(under: i64) -> Self {
        Self(under)
    }

    pub(crate) fn of(step: Option<StepIndex>) -> Self {
        Self(
            step.and_then(|step| i64::try_from(step.value()).ok())
                .unwrap_or(-1),
        )
    }

    pub(crate) fn step(self) -> Option<StepIndex> {
        usize::try_from(self.0).ok().map(StepIndex)
    }

    pub(crate) fn fits(self, steps: Count) -> bool {
        match usize::try_from(self.0) {
            Ok(step) => step < steps.0,
            Err(_) => self.0 >= -1,
        }
    }
}

impl fmt::Display for Under {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Count(usize);

impl Count {
    pub(crate) const fn new(count: usize) -> Self {
        Self(count)
    }

    pub(crate) const fn value(self) -> usize {
        self.0
    }

    pub(crate) const fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub(crate) const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    #[must_use]
    pub(crate) const fn plus(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub(crate) fn last(self) -> Under {
        Under::of(self.0.checked_sub(1).map(StepIndex))
    }
}

impl fmt::Display for Count {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LineNumber(i64);

impl LineNumber {
    #[cfg(test)]
    pub(crate) const fn new(number: i64) -> Self {
        Self(number)
    }

    pub(crate) fn of(number: usize) -> Self {
        Self(i64::try_from(number).unwrap_or(0x7fff_ffff_ffff_ffff))
    }

    pub(crate) fn visible(
        file: &SourceFile,
        start: Option<Self>,
        end: Option<Self>,
    ) -> Option<Span> {
        let length = i64::from(file.text().count().value());
        let first = start.map_or(1, |start| start.0.max(1)) - 1;
        let last = end
            .map_or(length, |end| end.0.min(length))
            .saturating_sub(1)
            .min(length.saturating_sub(1))
            .max(0);
        Span::new(
            Line::new(u32::try_from(first).ok()?),
            Line::new(u32::try_from(last).ok()?),
        )
    }

    fn line(self) -> Option<Line> {
        self.0
            .checked_sub(1)
            .and_then(|value| u32::try_from(value).ok())
            .map(Line::new)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Levels(usize);

impl Levels {
    #[cfg(test)]
    pub(crate) const fn new(levels: usize) -> Self {
        Self(levels)
    }

    pub(crate) fn depth(self) -> Depth {
        u32::try_from(self.0).map_or(Depth::new(0xffff_ffff), Depth::new)
    }
}

pub(crate) fn step_id(path: &Path, index: StepIndex) -> Option<StepId> {
    path.steps()
        .get(index.value())
        .map(|step| step.id().clone())
}

pub(crate) fn step_index(path: &Path, id: &StepId) -> Option<StepIndex> {
    path.steps()
        .iter()
        .position(|step| step.id() == id)
        .map(StepIndex::new)
}

pub(crate) fn step_count(path: &Path) -> Count {
    Count::new(path.steps().len())
}

pub(crate) fn line_range(file: &SourceFile, start: LineNumber, end: LineNumber) -> Option<Span> {
    let length = LineNumber(i64::from(file.text().count().value()));
    if start.0 < 1 || end < start || end > length {
        return None;
    }
    Span::new(start.line()?, end.line()?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lines {
    pub(crate) start: LineNumber,
    pub(crate) end: LineNumber,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) target: TextFragment,
    pub(crate) lines: Option<Lines>,
    pub(crate) under: Option<Under>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GroupPlacement {
    Keep,
    At(Option<GroupName>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LinkView {
    Expanded,
    Plain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Query {
    Files(Filter),
    Symbols(Filter),
    Show {
        file: RelativePath,
        start: Option<LineNumber>,
        end: Option<LineNumber>,
    },
    Grep(TextFragment),
    Notes(TextFragment),
    Callers(SymbolName),
    Callees(SymbolName),
    References(SymbolName),
    Index(Filter),
    Tree {
        symbol: SymbolName,
        levels: Levels,
    },
    Roots(Count),
    Paths(Option<TextFragment>),
    Path {
        name: TextFragment,
        view: LinkView,
    },
    Groups,
    Stale,
    Check,
    Uncovered(Filter),
    Coverage,
    Diff,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    PathNew {
        name: Result<PathName, MapError>,
        kind: PathKind,
        note: Option<Note>,
        group: GroupPlacement,
    },
    PathGroup {
        name: TextFragment,
        group: Option<GroupName>,
    },
    GroupRename {
        old: Option<GroupName>,
        new: Option<GroupName>,
    },
    PathNote {
        name: TextFragment,
        note: Option<Note>,
    },
    StepNote {
        name: TextFragment,
        step: StepIndex,
        note: Option<Note>,
    },
    StepLink {
        name: TextFragment,
        step: StepIndex,
        target: TextFragment,
    },
    StepUnlink {
        name: TextFragment,
        step: StepIndex,
    },
    NoteReplace {
        name: TextFragment,
        step: Option<StepIndex>,
        old: TextFragment,
        new: TextFragment,
    },
    PathRename {
        name: TextFragment,
        new: Result<PathName, MapError>,
    },
    PathAdd {
        name: TextFragment,
        placement: Placement,
    },
    PathPin {
        name: TextFragment,
        step: StepIndex,
        file: RelativePath,
        lines: Lines,
    },
    PathMove {
        name: TextFragment,
        step: StepIndex,
        under: Under,
    },
    PathSwap {
        name: TextFragment,
        one: StepIndex,
        other: StepIndex,
    },
    PathRemove {
        name: TextFragment,
        step: Option<StepIndex>,
    },
    Promote {
        symbol: SymbolName,
        levels: Levels,
        name: Result<Option<PathName>, MapError>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Request {
    Query(Query),
    Edit(Edit),
    Repin(Option<Revision>),
}

impl From<Command> for Request {
    fn from(command: Command) -> Self {
        match command {
            Command::Files(arguments) => Self::Query(Query::Files(arguments.into())),
            Command::Symbols(arguments) => Self::Query(Query::Symbols(arguments.into())),
            Command::Show(arguments) => Self::Query(arguments.into()),
            Command::Grep(arguments) => Self::Query(Query::Grep(arguments.into())),
            Command::Notes(arguments) => Self::Query(Query::Notes(arguments.into())),
            Command::Callers(arguments) => Self::Query(Query::Callers(arguments.into())),
            Command::Callees(arguments) => Self::Query(Query::Callees(arguments.into())),
            Command::Refs(arguments) => Self::Query(Query::References(arguments.into())),
            Command::Index(arguments) => Self::Query(Query::Index(arguments.into())),
            Command::Tree(arguments) => Self::Query(arguments.into()),
            Command::Roots(arguments) => Self::Query(Query::Roots(Count(arguments.count))),
            Command::Paths(arguments) => Self::Query(Query::Paths(
                arguments.name.as_deref().map(TextFragment::new),
            )),
            Command::Path(arguments) => Self::Query(arguments.into()),
            Command::Groups => Self::Query(Query::Groups),
            Command::Stale => Self::Query(Query::Stale),
            Command::Check => Self::Query(Query::Check),
            Command::Uncovered(arguments) => Self::Query(Query::Uncovered(arguments.into())),
            Command::Coverage => Self::Query(Query::Coverage),
            Command::Diff => Self::Query(Query::Diff),
            Command::Repin(arguments) => {
                Self::Repin(arguments.revision.as_deref().map(Revision::new))
            }
            Command::PathNew(arguments) => Self::Edit(arguments.into()),
            Command::PathGroup(arguments) => Self::Edit(arguments.into()),
            Command::GroupRename(arguments) => Self::Edit(arguments.into()),
            Command::PathNote(arguments) => Self::Edit(arguments.into()),
            Command::StepNote(arguments) => Self::Edit(arguments.into()),
            Command::StepLink(arguments) => Self::Edit(arguments.into()),
            Command::StepUnlink(arguments) => Self::Edit(arguments.into()),
            Command::NoteEdit(arguments) => Self::Edit(arguments.into()),
            Command::PathRename(arguments) => Self::Edit(arguments.into()),
            Command::PathAdd(arguments) => Self::Edit(arguments.into()),
            Command::PathPin(arguments) => Self::Edit(arguments.into()),
            Command::PathMove(arguments) => Self::Edit(arguments.into()),
            Command::PathSwap(arguments) => Self::Edit(arguments.into()),
            Command::PathRm(arguments) => Self::Edit(arguments.into()),
            Command::Promote(arguments) => Self::Edit(arguments.into()),
        }
    }
}

impl From<FilterArguments> for Filter {
    fn from(arguments: FilterArguments) -> Self {
        Self::new(arguments.filter)
    }
}

impl From<RegexArguments> for TextFragment {
    fn from(arguments: RegexArguments) -> Self {
        Self::new(&arguments.regex)
    }
}

impl From<SymbolArguments> for SymbolName {
    fn from(arguments: SymbolArguments) -> Self {
        Self::new(&arguments.symbol)
    }
}

impl From<ShowArguments> for Query {
    fn from(arguments: ShowArguments) -> Self {
        Self::Show {
            file: RelativePath::new(&arguments.file),
            start: arguments.start.map(LineNumber::of),
            end: arguments.end.map(LineNumber::of),
        }
    }
}

impl From<TreeArguments> for Query {
    fn from(arguments: TreeArguments) -> Self {
        Self::Tree {
            symbol: SymbolName::new(&arguments.symbol),
            levels: Levels(arguments.depth),
        }
    }
}

impl From<PathArguments> for Query {
    fn from(arguments: PathArguments) -> Self {
        Self::Path {
            name: TextFragment::new(&arguments.name),
            view: if arguments.expand {
                LinkView::Expanded
            } else {
                LinkView::Plain
            },
        }
    }
}

impl From<PathNewArguments> for Edit {
    fn from(arguments: PathNewArguments) -> Self {
        Self::PathNew {
            name: PathName::new(&arguments.name),
            kind: arguments.kind,
            note: arguments.note.as_deref().and_then(Note::new),
            group: arguments
                .group
                .as_deref()
                .map_or(GroupPlacement::Keep, |group| {
                    GroupPlacement::At(GroupName::new(group))
                }),
        }
    }
}

impl From<PathGroupArguments> for Edit {
    fn from(arguments: PathGroupArguments) -> Self {
        Self::PathGroup {
            name: TextFragment::new(&arguments.name),
            group: GroupName::new(&arguments.group),
        }
    }
}

impl From<GroupRenameArguments> for Edit {
    fn from(arguments: GroupRenameArguments) -> Self {
        Self::GroupRename {
            old: GroupName::new(&arguments.old),
            new: GroupName::new(&arguments.new),
        }
    }
}

impl From<PathNoteArguments> for Edit {
    fn from(arguments: PathNoteArguments) -> Self {
        Self::PathNote {
            name: TextFragment::new(&arguments.name),
            note: Note::new(&arguments.note),
        }
    }
}

impl From<StepNoteArguments> for Edit {
    fn from(arguments: StepNoteArguments) -> Self {
        Self::StepNote {
            name: TextFragment::new(&arguments.name),
            step: StepIndex(arguments.index),
            note: Note::new(&arguments.note),
        }
    }
}

impl From<StepLinkArguments> for Edit {
    fn from(arguments: StepLinkArguments) -> Self {
        Self::StepLink {
            name: TextFragment::new(&arguments.name),
            step: StepIndex(arguments.index),
            target: TextFragment::new(&arguments.target),
        }
    }
}

impl From<StepArguments> for Edit {
    fn from(arguments: StepArguments) -> Self {
        Self::StepUnlink {
            name: TextFragment::new(&arguments.name),
            step: StepIndex(arguments.index),
        }
    }
}

impl From<NoteEditArguments> for Edit {
    fn from(arguments: NoteEditArguments) -> Self {
        Self::NoteReplace {
            name: TextFragment::new(&arguments.name),
            step: usize::try_from(arguments.index).ok().map(StepIndex),
            old: TextFragment::new(&arguments.old),
            new: TextFragment::new(&arguments.new),
        }
    }
}

impl From<PathRenameArguments> for Edit {
    fn from(arguments: PathRenameArguments) -> Self {
        Self::PathRename {
            name: TextFragment::new(&arguments.name),
            new: PathName::new(&arguments.new),
        }
    }
}

impl From<PathAddArguments> for Edit {
    fn from(arguments: PathAddArguments) -> Self {
        let lines = |start: &i64, end: &i64| Lines {
            start: LineNumber(*start),
            end: LineNumber(*end),
        };
        let (lines, under) = match arguments.numbers.as_slice() {
            [under] => (None, Some(Under(*under))),
            [start, end] => (Some(lines(start, end)), None),
            [start, end, under] => (Some(lines(start, end)), Some(Under(*under))),
            _ => (None, None),
        };
        Self::PathAdd {
            name: TextFragment::new(&arguments.name),
            placement: Placement {
                target: TextFragment::new(&arguments.target),
                lines,
                under,
            },
        }
    }
}

impl From<PathPinArguments> for Edit {
    fn from(arguments: PathPinArguments) -> Self {
        Self::PathPin {
            name: TextFragment::new(&arguments.name),
            step: StepIndex(arguments.index),
            file: RelativePath::new(&arguments.file),
            lines: Lines {
                start: LineNumber::of(arguments.start),
                end: LineNumber::of(arguments.end),
            },
        }
    }
}

impl From<PathMoveArguments> for Edit {
    fn from(arguments: PathMoveArguments) -> Self {
        Self::PathMove {
            name: TextFragment::new(&arguments.name),
            step: StepIndex(arguments.index),
            under: Under(arguments.under),
        }
    }
}

impl From<PathSwapArguments> for Edit {
    fn from(arguments: PathSwapArguments) -> Self {
        Self::PathSwap {
            name: TextFragment::new(&arguments.name),
            one: StepIndex(arguments.one),
            other: StepIndex(arguments.other),
        }
    }
}

impl From<PathRmArguments> for Edit {
    fn from(arguments: PathRmArguments) -> Self {
        Self::PathRemove {
            name: TextFragment::new(&arguments.name),
            step: arguments.index.map(StepIndex),
        }
    }
}

impl From<PromoteArguments> for Edit {
    fn from(arguments: PromoteArguments) -> Self {
        Self::Promote {
            symbol: SymbolName::new(&arguments.symbol),
            levels: Levels(arguments.depth),
            name: arguments.name.as_deref().map(PathName::new).transpose(),
        }
    }
}
