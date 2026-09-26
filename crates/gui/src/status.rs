use std::fmt;
use std::path::PathBuf;

use domain::{Line, LineCount, Map, MapError, PathName, Program, RelativePath, SymbolName};
use ui::{Count, Label};

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
    CommandRejected(Label),
    CommandFailed(Label),
    RegexRefused(Label),
    Hits {
        count: Count,
        pattern: Label,
    },
    MapRefused(Label),
    PathCreated(PathName),
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

impl Status {
    pub(crate) fn refused(map: &Map, error: MapError) -> Self {
        Self::MapRefused(Label::new(cli::map_failure(map, error).to_string()))
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
            Self::CommandRejected(first) => formatter.write_str(first.as_str()),
            Self::CommandFailed(error) => write!(formatter, "error: {}", error.as_str()),
            Self::RegexRefused(reason) => write!(formatter, "bad regex: {}", reason.as_str()),
            Self::Hits { count, pattern } => {
                write!(formatter, "{count} hits for /{}/", pattern.as_str())
            }
            Self::MapRefused(error) => formatter.write_str(error.as_str()),
            Self::PathCreated(name) => write!(formatter, "path '{name}' created (unsaved)"),
            Self::SelectLinesFirst => formatter.write_str("select lines in the listing first"),
            Self::SelectPathFirst => formatter.write_str("select a path first"),
            Self::StepAdded {
                number,
                path,
                under,
            } => {
                let under = match under {
                    Under::TopLevel => "the top level",
                    Under::Step(parent) => parent.as_str(),
                };
                write!(
                    formatter,
                    "step {} added to '{path}' under {under}",
                    number.as_str()
                )
            }
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
            Self::NoFileOpen => formatter.write_str("no file open in the listing"),
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
                "map changed on disk while you have unsaved changes: save overwrites it, or use the command line to reload",
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
pub(crate) struct OutputLog(String);

impl Default for OutputLog {
    fn default() -> Self {
        Self("type 'help' for commands; roots and promote live here\n".to_owned())
    }
}

impl OutputLog {
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
