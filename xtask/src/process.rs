use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use crate::text::{Argument, Literal, Message, Program, Root};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Success,
    Failure,
}

#[derive(Debug)]
pub(crate) struct Run {
    pub(crate) outcome: Outcome,
    pub(crate) output: Message,
}

#[derive(Debug)]
pub(crate) struct Setting {
    name: Literal,
    value: OsString,
}

impl Setting {
    pub(crate) fn new(name: Literal, value: impl Into<OsString>) -> Self {
        Self {
            name,
            value: value.into(),
        }
    }
}

pub(crate) fn run(root: &Root, program: Program, arguments: &[Argument]) -> Run {
    run_in(root.path(), program, arguments, &[])
}

#[expect(
    clippy::disallowed_methods,
    reason = "xtask starts every process it runs here"
)]
pub(crate) fn run_in(
    directory: &Path,
    program: Program,
    arguments: &[Argument],
    environment: &[Setting],
) -> Run {
    let mut command = Command::new(program.as_str());
    command.current_dir(directory);
    command.args(arguments.iter().map(Argument::as_str));
    for setting in environment {
        command.env(setting.name.as_str(), &setting.value);
    }
    match command.output() {
        Ok(result) => Run {
            outcome: if result.status.success() {
                Outcome::Success
            } else {
                Outcome::Failure
            },
            output: Message::new(format!(
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            )),
        },
        Err(error) => Run {
            outcome: Outcome::Failure,
            output: Message::new(format!("{program}: {error}")),
        },
    }
}
