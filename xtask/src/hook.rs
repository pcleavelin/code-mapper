mod wire;

use std::fs;
use std::io::{Read, stdin};
use std::path::Path;
use std::slice;

use crate::arch;
use crate::gate::{Depth, Ownership, lint_files, run_gate};
use crate::process::{Outcome, run};
use crate::state::{GateState, Light, stamp};
use crate::terminal::say;
use crate::text::{Argument, Count, Literal, Message, Program, RepoPath, Root};

const REPEATS_BEFORE_RELEASE: Count = Count::new(3);

const CONFLICT_MARKERS: [Literal; 3] = [
    Literal::new("<<<<<<<"),
    Literal::new(">>>>>>>"),
    Literal::new("%%%%%%%"),
];

const WRITING: [Literal; 12] = [
    Literal::new(">"),
    Literal::new("sed -i"),
    Literal::new("tee "),
    Literal::new("mv "),
    Literal::new("rm "),
    Literal::new("cp "),
    Literal::new("python"),
    Literal::new("perl"),
    Literal::new("truncate"),
    Literal::new("ln "),
    Literal::new("chmod"),
    Literal::new("dd "),
];

const COMMITTING: [Literal; 7] = [
    Literal::new("git commit"),
    Literal::new("jj commit"),
    Literal::new("jj describe"),
    Literal::new("jj new"),
    Literal::new("jj squash"),
    Literal::new("git push"),
    Literal::new("jj git push"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    PreTool,
    PostTool,
    Stop,
    SessionStart,
}

impl Event {
    pub(crate) fn parse(word: &Argument) -> Option<Self> {
        match word.as_str() {
            "pre-tool" => Some(Self::PreTool),
            "post-tool" => Some(Self::PostTool),
            "stop" => Some(Self::Stop),
            "session-start" => Some(Self::SessionStart),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Edit,
    Shell,
    Other,
}

#[derive(Debug)]
enum Decision {
    Allow,
    Deny(Message),
    Block(Message),
    Context(Message),
    Notice(Message),
}

#[derive(Debug)]
struct Input {
    tool: Tool,
    file: Option<RepoPath>,
    command: Option<Message>,
    stop_active: StopActive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopActive {
    First,
    Continued,
}

fn convert(root: &Root, wire: wire::WireInput) -> Input {
    let tool = match wire.tool_name.as_str() {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => Tool::Edit,
        "Bash" => Tool::Shell,
        _ => Tool::Other,
    };
    Input {
        tool,
        file: wire
            .file_path
            .as_deref()
            .and_then(|path| root.relative(Path::new(path))),
        command: wire.command.map(Message::new),
        stop_active: if wire.stop_hook_active {
            StopActive::Continued
        } else {
            StopActive::First
        },
    }
}

pub(crate) fn handle(root: &Root, event: Event) -> Result<(), Message> {
    let mut text = String::new();
    stdin()
        .read_to_string(&mut text)
        .map_err(|error| Message::new(format!("hook input: {error}")))?;
    let wire = wire::read(&text)?;
    let event_name = wire.hook_event_name.clone();
    let input = convert(root, wire);
    let decision = match event {
        Event::PreTool => before_tool(root, &input),
        Event::PostTool => after_tool(root, &input),
        Event::Stop => stopping(root, &input),
        Event::SessionStart => starting(root),
    };
    let output = match decision {
        Decision::Allow => return Ok(()),
        Decision::Deny(reason) => wire::deny(reason.as_str()),
        Decision::Block(reason) => wire::block(reason.as_str()),
        Decision::Context(context) => wire::context(&event_name, context.as_str()),
        Decision::Notice(notice) => wire::notice(notice.as_str()),
    };
    say(&Message::new(output));
    Ok(())
}

fn before_tool(root: &Root, input: &Input) -> Decision {
    match (input.tool, &input.file, &input.command) {
        (Tool::Edit, Some(path), _) => guard_edit(root, path),
        (Tool::Shell, _, Some(command)) => guard_command(root, command),
        _ => Decision::Allow,
    }
}

fn guard_edit(root: &Root, path: &RepoPath) -> Decision {
    if arch::guarded(path) {
        return Decision::Deny(Message::new(format!(
            "{path} is part of the rulebook, which only the owner changes. Propose the change in your reply instead."
        )));
    }
    if path.starts_with(".codemap/") {
        let current = fs::read_to_string(root.join(path)).unwrap_or_default();
        if CONFLICT_MARKERS
            .iter()
            .any(|marker| marker.found_in(&current))
        {
            return Decision::Allow;
        }
        return Decision::Deny(Message::new(
            "The map is written only through codemap commands (`codemap . help`): path-add, step-note, note-edit, path-pin, repin.",
        ));
    }
    if path.starts_with("crates/codemap/tests/golden/") {
        return Decision::Deny(Message::new(
            "Goldens are written only by the tests: CODEMAP_BLESS=1 cargo test --test <scenario file>.",
        ));
    }
    Decision::Allow
}

fn guard_command(root: &Root, command: &Message) -> Decision {
    let text = command.as_str();
    let writes = WRITING.iter().any(|verb| verb.found_in(text));
    let protected = arch::RULEBOOK
        .iter()
        .map(|guard| match guard {
            arch::Guard::File(file) | arch::Guard::Tree(file) => file.as_str(),
        })
        .chain([".codemap/", "tests/golden/"])
        .find(|path| text.contains(path));
    if let (true, Some(path)) = (writes, protected)
        && !(path == ".codemap/" && text.contains("codemap"))
    {
        return Decision::Deny(Message::new(format!(
            "This command looks like it writes {path}, which is written only by the owner, codemap commands, or the tests."
        )));
    }
    if COMMITTING.iter().any(|verb| verb.found_in(text)) {
        return match current_gate(root) {
            Ok(()) => Decision::Allow,
            Err(report) => Decision::Deny(Message::new(format!(
                "Nothing is committed while the gate is red. Fix these first:\n{report}"
            ))),
        };
    }
    Decision::Allow
}

fn current_gate(root: &Root) -> Result<(), Message> {
    let now = stamp(root);
    if GateState::load(root).is_some_and(|state| state.light == Light::Green && state.stamp == now)
    {
        return Ok(());
    }
    run_gate(root, Depth::Fast, Ownership::Agent)
        .map_err(|failure| Message::new(failure.to_string()))
}

fn after_tool(root: &Root, input: &Input) -> Decision {
    let (Tool::Edit, Some(path)) = (input.tool, &input.file) else {
        return Decision::Allow;
    };
    if !path.ends_with(".rs") {
        return Decision::Allow;
    }
    let formatted = run(
        root,
        Program::RUSTFMT,
        &[
            Argument::new("--edition"),
            Argument::new("2024"),
            Argument::new(path.as_str()),
        ],
    );
    let mut report = Message::default();
    if formatted.outcome == Outcome::Failure {
        report.push_line(&format!("rustfmt: {}", formatted.output.head(20)));
    }
    if let Err(findings) = lint_files(root, slice::from_ref(path)) {
        report.push_line(findings.as_str());
    }
    if report.is_empty() {
        Decision::Allow
    } else {
        Decision::Block(report)
    }
}

fn stopping(root: &Root, input: &Input) -> Decision {
    match current_gate(root) {
        Ok(()) => Decision::Allow,
        Err(report) => {
            let repeats = GateState::load(root).map_or(Count::ZERO, |state| state.repeats);
            if input.stop_active == StopActive::Continued && repeats >= REPEATS_BEFORE_RELEASE {
                Decision::Notice(Message::new(format!(
                    "The gate is still red after {repeats} identical runs. The session stops here; commits stay refused and the next session starts with these failures:\n{report}"
                )))
            } else {
                Decision::Block(Message::new(format!(
                    "The gate is red. Fix these before stopping:\n{report}"
                )))
            }
        }
    }
}

fn starting(root: &Root) -> Decision {
    match GateState::load(root) {
        Some(state) if state.light == Light::Red => Decision::Context(Message::new(format!(
            "The previous session left the gate red. Fix these first:\n{}",
            state.failures
        ))),
        _ => Decision::Allow,
    }
}
