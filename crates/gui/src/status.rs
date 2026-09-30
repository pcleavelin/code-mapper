use std::fmt;
use std::path::PathBuf;

use domain::{Line, LineCount, Map, MapError, PathName, Program, RelativePath, SymbolName};
use ui::{Count, Label};

use crate::model::HitsShown;
use crate::text::{Counted, Noun};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IndexCounts {
    pub(crate) files: Count,
    pub(crate) symbols: Count,
}

impl fmt::Display for IndexCounts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} files, {} symbols indexed",
            self.files, self.symbols
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Plain,
    Done,
    Warning,
    Problem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Under {
    TopLevel,
    Step(Label),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Nothing,
    Indexed(IndexCounts),
    MapUnreadable(Label),
    SaveRefused,
    Saved,
    SaveFailed(Label),
    LayoutUnsaved(Label),
    CommandRejected(Label),
    CommandFailed(Label),
    RegexRefused(Label),
    Hits {
        count: Count,
        pattern: Label,
        shown: HitsShown,
    },
    MapRefused(Label),
    PathCreated(PathName),
    NameThePath,
    PathPromoted {
        name: PathName,
        steps: Count,
    },
    AlreadyStep {
        number: Label,
        path: PathName,
    },
    StepPlaced {
        number: Label,
        path: PathName,
        under: Under,
    },
    TopLevelTarget(PathName),
    SelectLinesFirst,
    SelectPathFirst,
    StepAdded {
        number: Label,
        path: PathName,
        under: Under,
    },
    StepRemoved {
        number: Label,
        symbol: Option<SymbolName>,
        file: RelativePath,
        path: PathName,
    },
    PathRemoved {
        name: PathName,
        steps: Count,
    },
    LineOutside {
        line: Label,
        last: LineCount,
    },
    NoLineNumber(Label),
    NoFileOpen,
    OffPathSymbol {
        symbol: SymbolName,
        path: PathName,
    },
    NoDefinitionOf(Label),
    DefinedOutside {
        file: PathBuf,
        line: Line,
    },
    NoDefinition,
    Starting(Program),
    ServerFailed(Label),
    Reindexing,
    Reindexed(IndexCounts),
    ReindexFailed,
    DiskChanged,
    MapUnreadableKept(Label),
    MapReloaded,
    OnlyInParent {
        name: PathName,
        steps: Count,
    },
}

impl fmt::Display for Under {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TopLevel => formatter.write_str("at the top level"),
            Self::Step(parent) => write!(formatter, "under {}", parent.as_str()),
        }
    }
}

impl Status {
    fn authoring(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PathCreated(name) => write!(formatter, "path '{name}' created (unsaved)"),
            Self::NameThePath => formatter.write_str("type a name for the new path"),
            Self::PathPromoted { name, steps } => write!(
                formatter,
                "path '{name}' made from the symbol and its calls, {steps} steps (unsaved)"
            ),
            Self::AlreadyStep { number, path } => write!(
                formatter,
                "already step {} of '{path}' in that place",
                number.as_str()
            ),
            Self::SelectLinesFirst => formatter.write_str("select lines in the Source view first"),
            Self::SelectPathFirst => formatter.write_str("open a path to add steps to first"),
            Self::StepAdded {
                number,
                path,
                under,
            } => write!(
                formatter,
                "step {} added to '{path}' {under}",
                number.as_str()
            ),
            Self::StepPlaced {
                number,
                path,
                under,
            } => write!(
                formatter,
                "moved to step {} of '{path}' {under}",
                number.as_str()
            ),
            Self::TopLevelTarget(path) => {
                write!(formatter, "steps added to '{path}' now go at the top level")
            }
            _ => Ok(()),
        }
    }

    fn hits(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hits {
                count,
                pattern,
                shown: HitsShown::All,
            } => write!(
                formatter,
                "{} for /{}/",
                Counted::new(*count, Noun::Hit),
                pattern.as_str()
            ),
            Self::Hits {
                count,
                pattern,
                shown: HitsShown::First,
            } => write!(
                formatter,
                "the first {count} hits for /{}/; the search stops there, a narrower regex finds the rest",
                pattern.as_str()
            ),
            _ => Ok(()),
        }
    }

    pub(crate) fn refused(map: &Map, error: MapError) -> Self {
        Self::MapRefused(Label::new(cli::map_failure(map, error).to_string()))
    }

    pub(crate) const fn tone(&self) -> Tone {
        match self {
            Self::Saved
            | Self::PathCreated(_)
            | Self::PathPromoted { .. }
            | Self::StepAdded { .. }
            | Self::StepPlaced { .. }
            | Self::StepRemoved { .. }
            | Self::PathRemoved { .. }
            | Self::MapReloaded
            | Self::Reindexed(_) => Tone::Done,
            Self::Hits {
                shown: HitsShown::First,
                ..
            }
            | Self::NameThePath
            | Self::AlreadyStep { .. }
            | Self::SelectLinesFirst
            | Self::SelectPathFirst
            | Self::LineOutside { .. }
            | Self::NoLineNumber(_)
            | Self::NoFileOpen
            | Self::NoDefinitionOf(_)
            | Self::NoDefinition
            | Self::DiskChanged => Tone::Warning,
            Self::MapUnreadable(_)
            | Self::SaveRefused
            | Self::SaveFailed(_)
            | Self::LayoutUnsaved(_)
            | Self::CommandRejected(_)
            | Self::CommandFailed(_)
            | Self::RegexRefused(_)
            | Self::MapRefused(_)
            | Self::ServerFailed(_)
            | Self::ReindexFailed
            | Self::MapUnreadableKept(_) => Tone::Problem,
            _ => Tone::Plain,
        }
    }

    pub(crate) const fn is_indexed(&self) -> bool {
        matches!(self, Self::Indexed(_))
    }

    pub(crate) const fn is_starting(&self) -> bool {
        matches!(self, Self::Starting(_))
    }

    pub(crate) fn line(&self) -> Label {
        let text = self.to_string();
        Label::new(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
        )
    }
}

impl fmt::Display for Status {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nothing => Ok(()),
            Self::Indexed(counts) => write!(formatter, "{counts}"),
            Self::MapUnreadable(error) => write!(
                formatter,
                "{}; the map is not shown and cannot be saved until it reads",
                error.as_str()
            ),
            Self::SaveRefused => formatter
                .write_str("not saved: the map on disk does not read; fix it and it reloads"),
            Self::Saved => formatter.write_str("saved"),
            Self::SaveFailed(error) => write!(formatter, "save FAILED: {}", error.as_str()),
            Self::LayoutUnsaved(error) => {
                write!(formatter, "layout not saved: {}", error.as_str())
            }
            Self::CommandRejected(first) => formatter.write_str(first.as_str()),
            Self::CommandFailed(error) => write!(formatter, "error: {}", error.as_str()),
            Self::RegexRefused(reason) => write!(formatter, "bad regex: {}", reason.as_str()),
            Self::Hits { .. } => self.hits(formatter),
            Self::MapRefused(error) => formatter.write_str(error.as_str()),
            Self::PathCreated(_)
            | Self::NameThePath
            | Self::PathPromoted { .. }
            | Self::AlreadyStep { .. }
            | Self::SelectLinesFirst
            | Self::SelectPathFirst
            | Self::StepAdded { .. }
            | Self::StepPlaced { .. }
            | Self::TopLevelTarget(_) => self.authoring(formatter),
            Self::StepRemoved {
                number,
                symbol,
                file,
                path,
            } => write!(
                formatter,
                "deleted step {} {} ({file}) from '{path}'; unsaved",
                number.as_str(),
                symbol.as_ref().map_or("", SymbolName::as_str)
            ),
            Self::PathRemoved { name, steps } => {
                write!(formatter, "deleted path '{name}' ({steps} steps); unsaved")
            }
            Self::LineOutside { line, last } => write!(
                formatter,
                "line {} is outside 1-{last}; went to the nearest",
                line.as_str()
            ),
            Self::NoLineNumber(text) => write!(formatter, "'{}' is not a line number", text.as_str()),
            Self::NoFileOpen => formatter.write_str("no file open in the Source view"),
            Self::OffPathSymbol { symbol, path } => {
                write!(formatter, "{symbol} selected; not a step of '{path}'")
            }
            Self::NoDefinitionOf(word) => {
                write!(formatter, "no definition of '{}' in this repo", word.as_str())
            }
            Self::DefinedOutside { file, line } => write!(
                formatter,
                "defined outside the repo: {}:{}",
                file.display(),
                line.number()
            ),
            Self::NoDefinition => formatter.write_str("no definition found"),
            Self::Starting(program) => write!(formatter, "{program}: starting"),
            Self::ServerFailed(error) => write!(
                formatter,
                "{}: its files keep the tree-sitter resolver, no hover or go-to for them",
                error.as_str()
            ),
            Self::Reindexing => formatter.write_str("re-indexing (source changed)"),
            Self::Reindexed(counts) => write!(formatter, "re-indexed: {counts} (source changed)"),
            Self::ReindexFailed => {
                formatter.write_str("re-index failed; the index in use is the old one")
            }
            Self::DiskChanged => formatter.write_str(
                "map changed on disk while you have unsaved changes: save overwrites it, or use the console to reload",
            ),
            Self::MapUnreadableKept(error) => write!(
                formatter,
                "{}; the map in memory is kept and cannot be saved until the one on disk reads",
                error.as_str()
            ),
            Self::MapReloaded => formatter.write_str("map reloaded (changed on disk)"),
            Self::OnlyInParent { name, steps } => write!(
                formatter,
                "'{name}' exists only in the parent revision; its {steps} steps are listed under the path it was removed from"
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ConsoleLog(String);

impl Default for ConsoleLog {
    fn default() -> Self {
        Self("type 'help' for commands; roots and promote live here\n".to_owned())
    }
}

impl ConsoleLog {
    pub(crate) fn lines(&self) -> impl Iterator<Item = &str> {
        self.0.lines()
    }

    pub(crate) fn line_count(&self) -> Count {
        Count::new(self.0.lines().count())
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    pub(crate) fn command(&mut self, line: &str) {
        self.0.push_str("> ");
        self.0.push_str(line);
        self.0.push('\n');
    }

    pub(crate) fn append(&mut self, text: &str) {
        self.0.push_str(text);
    }

    pub(crate) fn rejected(&mut self, text: &str) {
        self.0.push_str(text.trim_end());
        self.0.push('\n');
    }

    pub(crate) fn changed(&mut self) {
        self.0.push_str("(map changed, ctrl+s to save)\n");
    }

    pub(crate) fn failed(&mut self, error: &str) {
        self.0.push_str("error: ");
        self.0.push_str(error);
        self.0.push('\n');
    }
}
