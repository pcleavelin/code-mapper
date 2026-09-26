mod api;
mod arch;
mod files;
mod gate;
mod hook;
mod lint;
mod manifest;
mod process;
mod source;
mod state;
mod terminal;
mod text;
mod vcs;
mod vocabulary;
mod words;

use std::env;
use std::process::ExitCode;

use crate::gate::{Depth, Ownership, lint_files, run_gate};
use crate::hook::Event;
use crate::terminal::{complain, say};
use crate::text::{Argument, Literal, Message, RepoPath, Root};

const USAGE: Literal = Literal::new(
    "cargo xtask <task>
  gate [--full] [--owner]   format, archlint, crate graph, API lock, rulebook, clippy, unit tests, codemap check; --full adds the CLI goldens
  lint [file...]            archlint over the workspace or the given files
  api                       record every library crate's public API in api/<crate>.api
  words                     identifier words not in vocabulary.txt, with where they occur
  hook <event>              a Claude Code hook: pre-tool, post-tool, stop, session-start",
);

#[derive(Debug)]
enum Task {
    Gate(Depth, Ownership),
    Lint(Vec<RepoPath>),
    Api,
    Words,
    Hook(Event),
    Usage,
}

impl Task {
    fn parse(words: &[Argument]) -> Self {
        let rest: Vec<&str> = words.iter().skip(1).map(Argument::as_str).collect();
        match words.first().map(Argument::as_str) {
            Some("gate") => Self::Gate(
                if rest.contains(&"--full") {
                    Depth::Full
                } else {
                    Depth::Fast
                },
                if rest.contains(&"--owner") {
                    Ownership::Owner
                } else {
                    Ownership::Agent
                },
            ),
            Some("lint") => Self::Lint(rest.iter().map(|path| RepoPath::new(path)).collect()),
            Some("api") => Self::Api,
            Some("words") => Self::Words,
            Some("hook") => words
                .get(1)
                .and_then(Event::parse)
                .map_or(Self::Usage, Self::Hook),
            _ => Self::Usage,
        }
    }
}

fn perform(root: &Root, task: Task) -> Result<Message, Message> {
    match task {
        Task::Gate(depth, ownership) => run_gate(root, depth, ownership)
            .map(|()| Message::new("gate: green"))
            .map_err(|failure| Message::new(failure.to_string())),
        Task::Lint(paths) => lint_files(root, &paths).map(|()| Message::new("archlint: clean")),
        Task::Api => api::record(root).map(|()| Message::new("api: recorded")),
        Task::Words => words::unknown(root),
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
