mod api;
mod arch;
mod files;
mod gate;
mod hook;
mod lint;
mod manifest;
mod parity;
mod process;
mod source;
mod state;
mod terminal;
mod text;

use std::env;
use std::process::ExitCode;

use strum::VariantArray;

use crate::gate::{Depth, lint_files, run_gate};
use crate::hook::Event;
use crate::terminal::{complain, say};
use crate::text::{Argument, Literal, Message, RepoPath, Root};

const USAGE: Literal = Literal::new(
    "cargo xtask <task>
  gate [--full]             format, archlint, crate graph, API lock, clippy, unit tests, codemap check; --full adds the CLI scenarios
  parity [scenario]         run every scenario against the parent revision's build and list the ones whose output differs
  lint [file...]            archlint over the workspace or the given files
  api                       record every library crate's public API in api/<crate>.api
  hook <event>              a Claude Code hook: pre-tool, post-tool, stop, session-start",
);

#[derive(Debug)]
enum Task {
    Gate(Depth),
    Lint(Vec<RepoPath>),
    Api,
    Parity(Option<Argument>),
    Hook(Event),
    Usage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
enum TaskKind {
    Gate,
    Lint,
    Api,
    Parity,
    Hook,
}

impl TaskKind {
    const fn name(self) -> Literal {
        match self {
            Self::Gate => Literal::new("gate"),
            Self::Lint => Literal::new("lint"),
            Self::Api => Literal::new("api"),
            Self::Parity => Literal::new("parity"),
            Self::Hook => Literal::new("hook"),
        }
    }

    fn named(word: &Argument) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|task| task.name().as_str() == word.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flag {
    Full,
}

impl Flag {
    const fn name(self) -> Literal {
        match self {
            Self::Full => Literal::new("--full"),
        }
    }
}

impl Task {
    fn parse(words: &[Argument]) -> Self {
        let rest: Vec<&str> = words.iter().skip(1).map(Argument::as_str).collect();
        match words.first().and_then(TaskKind::named) {
            Some(TaskKind::Gate) => Self::Gate(if rest.contains(&Flag::Full.name().as_str()) {
                Depth::Full
            } else {
                Depth::Fast
            }),
            Some(TaskKind::Lint) => {
                Self::Lint(rest.iter().map(|path| RepoPath::new(path)).collect())
            }
            Some(TaskKind::Api) => Self::Api,
            Some(TaskKind::Parity) => Self::Parity(words.get(1).cloned()),
            Some(TaskKind::Hook) => words
                .get(1)
                .and_then(Event::parse)
                .map_or(Self::Usage, Self::Hook),
            None => Self::Usage,
        }
    }
}

fn perform(root: &Root, task: Task) -> Result<Message, Message> {
    match task {
        Task::Gate(depth) => run_gate(root, depth)
            .map(|()| Message::new("gate: green"))
            .map_err(|failure| Message::new(failure.to_string())),
        Task::Lint(paths) => lint_files(root, &paths).map(|()| Message::new("archlint: clean")),
        Task::Api => api::record(root).map(|()| Message::new("api: recorded")),
        Task::Parity(only) => parity::compare(root, only.as_ref()),
        Task::Hook(event) => hook::handle(root, event).map(|()| Message::default()),
        Task::Usage => Err(Message::new(USAGE.as_str())),
    }
}

fn main() -> ExitCode {
    let words: Vec<Argument> = env::args().skip(1).map(Argument::new).collect();
    let root = Root::of_workspace();
    match perform(&root, Task::parse(&words)) {
        Ok(message) => {
            if !message.is_empty() {
                say(&message);
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            complain(&message);
            ExitCode::FAILURE
        }
    }
}
