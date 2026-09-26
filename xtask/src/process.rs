use std::process::Command;

use crate::text::{Argument, Message, Program, Root};

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

#[expect(
    clippy::disallowed_methods,
    reason = "xtask starts every process it runs here"
)]
pub(crate) fn run(root: &Root, program: Program, arguments: &[Argument]) -> Run {
    let mut command = Command::new(program.as_str());
    command.current_dir(root.path());
    command.args(arguments.iter().map(Argument::as_str));
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
