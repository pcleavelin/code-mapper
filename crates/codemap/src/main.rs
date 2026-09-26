mod output;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use cli::{Argument, Channel, Failure, Invocation, Output};
use domain::{Author, Root};
use index::Servers;
use io_map::MapStore;

enum Status {
    Done,
    SaveFailed,
    WindowFailed,
    Refused,
}

impl Status {
    fn code(self) -> ExitCode {
        match self {
            Self::Done => ExitCode::SUCCESS,
            Self::SaveFailed | Self::WindowFailed => ExitCode::from(1),
            Self::Refused => ExitCode::from(2),
        }
    }
}

fn run(root: &Root, arguments: &[Argument]) -> Status {
    let invocation = match Invocation::parse(arguments) {
        Ok(invocation) => invocation,
        Err(failure) => {
            output::text(failure.channel(), &failure.text());
            return match failure.channel() {
                Channel::Stdout => Status::Done,
                Channel::Stderr => Status::Refused,
            };
        }
    };
    let mut index = index::build(root).index;
    let mut store = MapStore::new(root);
    let mut map = match store.load() {
        Ok(map) => map,
        Err(error) => {
            output::line(Channel::Stderr, &Failure::Load(error));
            return Status::Refused;
        }
    };
    map.resolve_all(&index);
    let mut report = Output::new();
    let mut servers = Servers::new(root, |notice| {
        output::line(Channel::Stderr, &cli::notice(notice));
    });
    let done = cli::exec(
        &mut index,
        &mut map,
        invocation,
        Author::Agent,
        Some(&mut servers),
        &mut report,
    );
    drop(servers);
    match done {
        Ok(changed) => {
            if changed.is_some()
                && let Err(error) = store.save(&map)
            {
                output::text(Channel::Stdout, &report);
                output::line(Channel::Stderr, &Failure::Save(error));
                return Status::SaveFailed;
            }
            output::text(Channel::Stdout, &report);
            Status::Done
        }
        Err(failure) => {
            output::text(Channel::Stdout, &report);
            output::line(Channel::Stderr, &failure);
            Status::Refused
        }
    }
}

fn window(root: &Root) -> Status {
    match gui::run(root) {
        Ok(()) => Status::Done,
        Err(error) => {
            output::line(Channel::Stderr, &error);
            Status::WindowFailed
        }
    }
}

fn main() -> ExitCode {
    let arguments: Vec<Argument> = env::args()
        .skip(1)
        .map(|argument| Argument::new(&argument))
        .collect();
    let first = arguments.first();
    if first.is_some_and(|first| matches!(first.as_str(), "help" | "--help" | "-h")) {
        output::text(Channel::Stdout, &cli::help());
        return ExitCode::SUCCESS;
    }
    let root = first.map_or_else(
        || env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        |first| PathBuf::from(first.as_str()),
    );
    let rest: Vec<Argument> = arguments.iter().skip(1).cloned().collect();
    if rest.is_empty() {
        return window(&Root::new(&root)).code();
    }
    run(&Root::new(&root), &rest).code()
}
