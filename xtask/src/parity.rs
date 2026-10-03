use std::env;
use std::env::consts::EXE_SUFFIX;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::files;
use crate::process::{Outcome, Run, Setting, run, run_in};
use crate::text::{Argument, Literal, Message, Program, RepoPath, Root};

const DIRECTORY: Literal = Literal::new("target/parity");
const BINARY: Literal = Literal::new("codemap");

const BASE_BINARY: Literal = Literal::new("CODEMAP_BASE_BIN");
const BASE_REVISION: Literal = Literal::new("CODEMAP_PARITY_REV");
const ONLY: Literal = Literal::new("CODEMAP_PARITY_ONLY");
const TARGET: Literal = Literal::new("CARGO_TARGET_DIR");

const DIFFERENCES: [Literal; 4] = [
    Literal::new("differences:"),
    Literal::new(" differs"),
    Literal::new(" screenshots old, "),
    Literal::new("script failed:"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommitId(String);

impl fmt::Display for CommitId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub(crate) fn compare(root: &Root, only: Option<&Argument>) -> Result<Message, Message> {
    if selects_gui(only) {
        require_linux_compositor()?;
    }
    let revision = parent_commit(root)?;
    let base = base_binary(root, &revision)?;
    let mut environment = vec![
        Setting::new(BASE_BINARY, base),
        Setting::new(BASE_REVISION, revision.0.clone()),
    ];
    if let Some(scenario) = only {
        environment.push(Setting::new(ONLY, scenario.as_str()));
    }
    let tested = run_in(
        root.path(),
        Program::CARGO,
        &Argument::list(&[
            "test",
            "--release",
            "--quiet",
            "--package",
            "codemap",
            "--test",
            "parity",
            "--",
            "--ignored",
        ]),
        &environment,
    );
    match tested.outcome {
        Outcome::Success => Ok(Message::new(format!(
            "parity: every scenario matches {revision}"
        ))),
        Outcome::Failure => Err(Message::new(format!(
            "parity: scenarios whose output differs from {revision} (old.txt and new.txt kept at each path):\n{}",
            differences(&tested.output)
        ))),
    }
}

const LINUX: Literal = Literal::new("linux");
const CLI_FILTER: Literal = Literal::new("cli");
const CLI_SCENARIO: Literal = Literal::new("cli-");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum System {
    Linux,
    Other,
}

impl System {
    fn here() -> Self {
        if env::consts::OS == LINUX.as_str() {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Compositor {
    Ready,
    Unset,
    SocketMissing,
}

fn compositor_here() -> Compositor {
    let runtime = env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|dir| !dir.is_empty());
    let display = env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|name| !name.is_empty());
    match (runtime, display) {
        (Some(dir), Some(name)) => {
            if Path::new(&dir).join(name).exists() {
                Compositor::Ready
            } else {
                Compositor::SocketMissing
            }
        }
        _ => Compositor::Unset,
    }
}

fn selects_gui(only: Option<&Argument>) -> bool {
    let Some(only) = only else {
        return true;
    };
    let text = only.as_str();
    if text.is_empty() {
        return true;
    }
    text != CLI_FILTER.as_str() && !text.starts_with(CLI_SCENARIO.as_str())
}

fn require_linux_compositor() -> Result<(), Message> {
    match linux_gui_refusal(System::here(), compositor_here()) {
        Some(message) => Err(message),
        None => Ok(()),
    }
}

fn linux_gui_refusal(system: System, compositor: Compositor) -> Option<Message> {
    if system == System::Other {
        return None;
    }
    match compositor {
        Compositor::Ready => None,
        Compositor::Unset => Some(Message::new(
            "parity: GUI scenarios on Linux need one headless weston. Weston exits with \"fatal: environment variable XDG_RUNTIME_DIR is not set\" when that variable is missing from the weston process. Run .cursor/skills/verify-codemap/bin/control-codemap parity (it passes XDG_RUNTIME_DIR to weston, then runs this task). One GUI window at a time on a desktop display. A hidden window stops getting frames and the script stalls.",
        )),
        Compositor::SocketMissing => Some(Message::new(
            "parity: WAYLAND_DISPLAY is set but its socket is not in XDG_RUNTIME_DIR. Run .cursor/skills/verify-codemap/bin/control-codemap parity to start weston. One GUI window at a time on a desktop display. A hidden window stops getting frames and the script stalls.",
        )),
    }
}

fn parent_commit(root: &Root) -> Result<CommitId, Message> {
    let jj = run(
        root,
        Program::JJ,
        &Argument::list(&["log", "-r", "@-", "--no-graph", "-T", "commit_id"]),
    );
    let asked = if jj.outcome == Outcome::Success {
        jj
    } else {
        run(root, Program::GIT, &Argument::list(&["rev-parse", "HEAD"]))
    };
    let text = asked.output.as_str().trim();
    if asked.outcome == Outcome::Success && !text.is_empty() {
        Ok(CommitId(text.to_owned()))
    } else {
        Err(Message::new(format!(
            "parity: no parent revision (neither jj nor git answered):\n{}",
            asked.output
        )))
    }
}

fn base_binary(root: &Root, revision: &CommitId) -> Result<PathBuf, Message> {
    let parity = root.join(&RepoPath::new(DIRECTORY.as_str()));
    let binary = parity
        .join("bin")
        .join(&revision.0)
        .join(format!("{}{EXE_SUFFIX}", BINARY.as_str()));
    if binary.is_file() {
        return Ok(binary);
    }
    let source = parity.join("source");
    let archive = parity.join("source.tar");
    files::fresh_directory(&source)?;
    succeed(&run(
        root,
        Program::GIT,
        &[
            Argument::new("archive"),
            Argument::new("--format=tar"),
            Argument::new("-o"),
            Argument::new(archive.display().to_string()),
            Argument::new(revision.0.clone()),
        ],
    ))?;
    succeed(&run(
        root,
        Program::TAR,
        &[
            Argument::new("-xf"),
            Argument::new(archive.display().to_string()),
            Argument::new("-C"),
            Argument::new(source.display().to_string()),
        ],
    ))?;
    let targets = parity.join("build");
    succeed(&run_in(
        &source,
        Program::CARGO,
        &Argument::list(&["build", "--release", "--quiet", "--package", "codemap"]),
        &[Setting::new(TARGET, targets.clone())],
    ))?;
    let fresh = targets
        .join("release")
        .join(format!("{}{EXE_SUFFIX}", BINARY.as_str()));
    files::copy(&fresh, &binary)?;
    Ok(binary)
}

fn succeed(done: &Run) -> Result<(), Message> {
    match done.outcome {
        Outcome::Success => Ok(()),
        Outcome::Failure => Err(Message::new(format!(
            "parity: building the base failed:\n{}",
            done.output
        ))),
    }
}

fn differences(output: &Message) -> Message {
    let found: Vec<&str> = output
        .as_str()
        .lines()
        .filter(|line| DIFFERENCES.iter().any(|marker| marker.found_in(line)))
        .collect();
    if found.is_empty() {
        output.head(40)
    } else {
        Message::new(found.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::{Compositor, System, linux_gui_refusal};

    fn shown(system: System, compositor: Compositor) -> Option<String> {
        linux_gui_refusal(system, compositor).map(|message| message.as_str().to_owned())
    }

    #[test]
    fn linux_without_a_runtime_dir_names_weston_and_the_command() {
        let message = shown(System::Linux, Compositor::Unset).unwrap();
        assert!(message.contains("fatal: environment variable XDG_RUNTIME_DIR is not set"));
        assert!(message.contains("control-codemap parity"));
        assert!(message.contains("One GUI window at a time"));
    }

    #[test]
    fn linux_with_a_live_socket_is_ready() {
        assert!(shown(System::Linux, Compositor::Ready).is_none());
    }

    #[test]
    fn linux_with_names_but_no_socket_names_the_command() {
        let message = shown(System::Linux, Compositor::SocketMissing).unwrap();
        assert!(message.contains("socket is not in XDG_RUNTIME_DIR"));
        assert!(message.contains("control-codemap parity"));
    }

    #[test]
    fn empty_runtime_dir_is_missing() {
        let message = shown(System::Linux, Compositor::Unset).unwrap();
        assert!(message.contains("XDG_RUNTIME_DIR is not set"));
    }

    #[test]
    fn macos_does_not_need_weston() {
        assert!(shown(System::Other, Compositor::Unset).is_none());
    }

    #[test]
    fn a_cli_filter_does_not_open_a_window() {
        let filter = |text: &str| super::Argument::new(text);
        assert!(!super::selects_gui(Some(&filter("cli"))));
        assert!(!super::selects_gui(Some(&filter("cli-read"))));
        assert!(super::selects_gui(None));
        assert!(super::selects_gui(Some(&filter(""))));
        assert!(super::selects_gui(Some(&filter("gui-graph"))));
        assert!(super::selects_gui(Some(&filter("graph"))));
    }
}
