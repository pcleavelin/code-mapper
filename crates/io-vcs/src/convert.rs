use domain::FileText;
use io_process::{Argument, Captured, Program};

use crate::RevisionText;
use crate::wire::CommandLine;

pub(crate) struct Invocation {
    pub(crate) program: Program,
    pub(crate) arguments: Vec<Argument>,
}

impl From<&CommandLine> for Invocation {
    fn from(line: &CommandLine) -> Self {
        Self {
            program: Program::new(line.program),
            arguments: line
                .arguments
                .iter()
                .map(|argument| Argument::new(argument))
                .collect(),
        }
    }
}

pub(crate) fn file_text(stdout: &Captured) -> FileText {
    FileText::from(stdout.text().as_str())
}

pub(crate) fn revision_text(stdout: &Captured) -> RevisionText {
    RevisionText::new(stdout.text())
}
