mod wire;

use std::fs;
use std::io::{Read, stdin};
use std::path::Path;
use std::slice;

use strum::VariantArray;

use crate::gate::{Depth, lint_files, run_gate};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
pub(crate) enum Event {
    PreTool,
    PostTool,
    Stop,
    SessionStart,
}

impl Event {
    const fn name(self) -> Literal {
        match self {
            Self::PreTool => Literal::new("pre-tool"),
            Self::PostTool => Literal::new("post-tool"),
            Self::Stop => Literal::new("stop"),
            Self::SessionStart => Literal::new("session-start"),
        }
    }

    pub(crate) fn parse(word: &Argument) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|event| event.name().as_str() == word.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Edit,
    Shell,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
enum ToolName {
    Edit,
    Write,
    MultiEdit,
    NotebookEdit,
    Bash,
}

impl ToolName {
    const fn name(self) -> Literal {
        match self {
            Self::Edit => Literal::new("Edit"),
            Self::Write => Literal::new("Write"),
            Self::MultiEdit => Literal::new("MultiEdit"),
            Self::NotebookEdit => Literal::new("NotebookEdit"),
            Self::Bash => Literal::new("Bash"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProtectedPath {
    Map,
}

impl ProtectedPath {
    const fn name(self) -> Literal {
        match self {
            Self::Map => Literal::new(".codemap/"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    Codemap,
}

impl Marker {
    const fn name(self) -> Literal {
        match self {
            Self::Codemap => Literal::new("codemap"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Suffix {
    Rust,
}

impl Suffix {
    const fn name(self) -> Literal {
        match self {
            Self::Rust => Literal::new(".rs"),
        }
    }
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
    stopping: Stopping,
    background: Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopActive {
    First,
    Continued,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
enum Stopping {
    Session,
    Agent,
}

impl Stopping {
    const fn name(self) -> Literal {
        match self {
            Self::Session => Literal::new("Stop"),
            Self::Agent => Literal::new("SubagentStop"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Background {
    Idle,
    Busy,
}

fn convert(root: &Root, wire: wire::WireInput) -> Input {
    let tool_name = wire.tool_name.as_str();
    let tool = match ToolName::VARIANTS
        .iter()
        .copied()
        .find(|tool| tool.name().as_str() == tool_name)
    {
        Some(ToolName::Edit | ToolName::Write | ToolName::MultiEdit | ToolName::NotebookEdit) => {
            Tool::Edit
        }
        Some(ToolName::Bash) => Tool::Shell,
        None => Tool::Other,
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
        stopping: Stopping::VARIANTS
            .iter()
            .copied()
            .find(|stopping| stopping.name().as_str() == wire.hook_event_name)
            .unwrap_or(Stopping::Session),
        background: if wire.background_tasks > 0 {
            Background::Busy
        } else {
            Background::Idle
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
    if path.starts_with(ProtectedPath::Map.name().as_str()) {
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
    Decision::Allow
}

fn guard_command(root: &Root, command: &Message) -> Decision {
    let text = command.as_str();
    let writes = WRITING.iter().any(|verb| verb.found_in(text));
    let map = ProtectedPath::Map.name();
    if writes && text.contains(map.as_str()) && !text.contains(Marker::Codemap.name().as_str()) {
        return Decision::Deny(Message::new(format!(
            "This command looks like it writes {map}, which is written only through codemap commands (`codemap . help`)."
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
    run_gate(root, Depth::Fast).map_err(|failure| Message::new(failure.to_string()))
}

fn after_tool(root: &Root, input: &Input) -> Decision {
    let (Tool::Edit, Some(path)) = (input.tool, &input.file) else {
        return Decision::Allow;
    };
    if !path.ends_with(Suffix::Rust.name().as_str()) {
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
    if input.stopping == Stopping::Agent || input.background == Background::Busy {
        return Decision::Allow;
    }
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
