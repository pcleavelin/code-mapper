use std::env::consts::EXE_SUFFIX;
use std::fmt;
use std::path::PathBuf;

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
