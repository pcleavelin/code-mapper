pub(crate) mod checks;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use strum::VariantArray;

use crate::arch;
use crate::source::SourceFile;
use crate::text::{CrateName, LineNumber, Message, RepoPath, RuleCode, TypeName};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, VariantArray)]
pub(crate) enum Rule {
    Comment,
    Primitive,
    NewtypeField,
    Indexing,
    WireWording,
    Absence,
    EnvAccess,
    WireLeak,
    DomainIo,
    Suppression,
    TestRegistry,
    Alias,
    Widget,
    Theme,
    ElementId,
    KeyBinding,
    Compared,
}

impl Rule {
    pub(crate) const fn code(self) -> RuleCode {
        RuleCode::new(match self {
            Self::Comment => "L1",
            Self::Primitive => "L2",
            Self::NewtypeField => "L3",
            Self::Indexing => "L4",
            Self::WireWording => "L5",
            Self::Absence => "L6",
            Self::EnvAccess => "L7",
            Self::WireLeak => "L8",
            Self::DomainIo => "L9",
            Self::Suppression => "L10",
            Self::TestRegistry => "L11",
            Self::Alias => "L16",
            Self::Widget | Self::Theme => "L12",
            Self::ElementId => "L13",
            Self::KeyBinding => "L15",
            Self::Compared => "L17",
        })
    }

    pub(crate) fn one_way(self) -> Message {
        Message::new(match self {
            Self::Comment => {
                "no comments: say it with a type, a name or a test, or put it in the map as a step note"
            }
            Self::Primitive => {
                "signatures, fields and variants take named types: wrap the value in a newtype that says what it is (a two-way choice is an enum)"
            }
            Self::NewtypeField => {
                "a newtype's field is private: construct it through its constructor and read it through its methods"
            }
            Self::Indexing => {
                "no indexing or slicing: read the collection through its interface (get, iter, a typed id)"
            }
            Self::WireWording => {
                "cli wording lives in crates/cli/src/wire.rs: format text and write_line! there (output.rs defines the helper)"
            }
            Self::Absence => "absence is Option: no \"\", -1 or MAX standing for none",
            Self::EnvAccess => {
                "environment variables are read in one place per concern: add the read next to the other env reads in platform, io-layout, io-process, gui/app.rs or codemap/main.rs"
            }
            Self::WireLeak => {
                "wire types stay inside their crate: convert to a domain type in convert.rs before it leaves"
            }
            Self::DomainIo => {
                "the domain crate does no I/O: files, processes, threads and printing belong to an io crate"
            }
            Self::Suppression => {
                "a suppression is #[expect(lint, reason = \"...\")] and is listed in xtask/src/arch.rs EXPECTS"
            }
            Self::TestRegistry => {
                "tests are registered once: add the scenario to its table, which generates the #[test]"
            }
            Self::Alias => "no type aliases: a new name for a type is a newtype",
            Self::Widget => {
                "GUI elements are built by the widget helpers: call one from crates/gui/src/widgets.rs, or add the helper there"
            }
            Self::Theme => {
                "sizes and colours are named in crates/gui/src/theme.rs: use the constant, or add one there"
            }
            Self::ElementId => {
                "element ids are named in crates/gui/src/ids.rs: use the constant, or add one there"
            }
            Self::KeyBinding => {
                "keys are bound in crates/gui/src/keys.rs from the feature registry's chords: add the trigger to crates/features"
            }
            Self::Compared => {
                "a string literal is never matched or compared: name the spellings in an enum whose name() spells each one, and compare through it"
            }
        })
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.code())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Finding {
    pub(crate) path: RepoPath,
    pub(crate) line: LineNumber,
    pub(crate) rule: Rule,
    pub(crate) detail: Message,
}

impl fmt::Display for Finding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}: {}: {} ({})",
            self.path,
            self.line,
            self.rule,
            self.rule.one_way(),
            self.detail
        )
    }
}

#[derive(Debug, Default)]
pub(crate) struct Workspace {
    pub(crate) newtypes: BTreeSet<TypeName>,
    pub(crate) wire_types: BTreeMap<CrateName, BTreeSet<TypeName>>,
}

impl Workspace {
    pub(crate) fn of(files: &[SourceFile]) -> Self {
        let mut workspace = Self::default();
        for file in files {
            checks::collect_types(file, &mut workspace);
        }
        workspace
    }
}

pub(crate) fn lint(workspace: &Workspace, files: &[SourceFile]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in files {
        for rule in Rule::VARIANTS.iter().copied() {
            if arch::applies(rule, file) {
                checks::check(rule, workspace, file, &mut findings);
            }
        }
    }
    findings.sort();
    findings.dedup();
    findings
}

#[cfg(test)]
mod tests;
