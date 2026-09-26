use std::fmt;
use std::fs;

use crate::api;
use crate::arch;
use crate::lint::{Workspace, lint};
use crate::manifest::Manifest;
use crate::process::{Outcome, run};
use crate::source::{SourceFile, rust_sources};
use crate::state::{GateState, Light, stamp};
use crate::text::{Argument, Content, Count, Literal, Message, Program, RepoPath, Root};
use crate::vcs::Vcs;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Depth {
    Fast,
    Full,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ownership {
    Agent,
    Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Format,
    Lint,
    Architecture,
    Api,
    Rulebook,
    Clippy,
    Test,
    Map,
    Features,
    Goldens,
}

impl fmt::Display for Step {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Format => "format (cargo fmt --all)",
            Self::Lint => "archlint (cargo xtask lint <file>)",
            Self::Architecture => "crate dependencies (xtask/src/arch.rs DEPENDENCIES)",
            Self::Api => "public API (cargo xtask api)",
            Self::Rulebook => "rulebook",
            Self::Clippy => "clippy",
            Self::Test => "unit tests",
            Self::Map => "codemap check",
            Self::Features => "feature coverage (crates/codemap/tests/features.rs)",
            Self::Goldens => "CLI goldens",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Member {
    Root,
}

impl Member {
    const fn name(self) -> Literal {
        match self {
            Self::Root => Literal::new("."),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum File {
    Manifest,
}

impl File {
    const fn name(self) -> Literal {
        match self {
            Self::Manifest => Literal::new("Cargo.toml"),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) step: Step,
    pub(crate) report: Message,
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "gate failed at {}:\n{}",
            self.step,
            self.report.head(80)
        )
    }
}

pub(crate) fn run_gate(root: &Root, depth: Depth, ownership: Ownership) -> Result<(), Failure> {
    let before = stamp(root);
    let outcome = steps(root, depth, ownership);
    let previous = GateState::load(root);
    let state = match &outcome {
        Ok(()) => GateState {
            stamp: before,
            light: Light::Green,
            repeats: Count::ZERO,
            failures: Message::default(),
        },
        Err(failure) => {
            let failures = Message::new(failure.to_string());
            let repeats = previous
                .filter(|last| last.light == Light::Red && last.failures == failures)
                .map_or(Count::new(1), |last| last.repeats.next());
            GateState {
                stamp: before,
                light: Light::Red,
                repeats,
                failures,
            }
        }
    };
    state.save(root).map_err(|report| Failure {
        step: Step::Rulebook,
        report,
    })?;
    outcome
}

fn steps(root: &Root, depth: Depth, ownership: Ownership) -> Result<(), Failure> {
    command(
        root,
        Step::Format,
        Program::CARGO,
        &Argument::list(&["fmt", "--all", "--check"]),
    )?;
    check(Step::Lint, lint_all(root))?;
    check(Step::Architecture, architecture(root))?;
    check(Step::Api, api::check(root))?;
    if ownership == Ownership::Agent {
        check(Step::Rulebook, rulebook(root))?;
    }
    command(
        root,
        Step::Clippy,
        Program::CARGO,
        &Argument::list(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--quiet",
            "--",
            "-D",
            "warnings",
        ]),
    )?;
    let libraries = api::surfaces(root).map_err(|report| Failure {
        step: Step::Test,
        report,
    })?;
    let targets: &[&str] = if libraries.is_empty() {
        &["test", "--workspace", "--quiet", "--bins"]
    } else {
        &["test", "--workspace", "--quiet", "--bins", "--lib"]
    };
    command(root, Step::Test, Program::CARGO, &Argument::list(targets))?;
    command(
        root,
        Step::Map,
        Program::CARGO,
        &Argument::list(&["run", "--quiet", "--package", "codemap", "--", ".", "check"]),
    )?;
    command(
        root,
        Step::Features,
        Program::CARGO,
        &Argument::list(&[
            "test",
            "--quiet",
            "--package",
            "codemap",
            "--test",
            "features",
        ]),
    )?;
    if depth == Depth::Full {
        command(
            root,
            Step::Goldens,
            Program::CARGO,
            &Argument::list(&["test", "--quiet", "--package", "codemap", "--test", "cli"]),
        )?;
    }
    Ok(())
}

fn check(step: Step, result: Result<(), Message>) -> Result<(), Failure> {
    result.map_err(|report| Failure { step, report })
}

fn command(root: &Root, step: Step, program: Program, words: &[Argument]) -> Result<(), Failure> {
    let result = run(root, program, words);
    match result.outcome {
        Outcome::Success => Ok(()),
        Outcome::Failure => Err(Failure {
            step,
            report: result.output,
        }),
    }
}

pub(crate) fn lint_files(root: &Root, paths: &[RepoPath]) -> Result<(), Message> {
    let every: Vec<SourceFile> = rust_sources(root)
        .into_iter()
        .map(|path| SourceFile::load(root, path))
        .collect::<Result<_, _>>()?;
    let workspace = Workspace::of(&every);
    let chosen: Vec<SourceFile> = if paths.is_empty() {
        every
    } else {
        every
            .into_iter()
            .filter(|file| paths.contains(&file.path))
            .collect()
    };
    let findings = lint(&workspace, &chosen);
    if findings.is_empty() {
        return Ok(());
    }
    let mut report = Message::default();
    for finding in &findings {
        report.push_line(&finding.to_string());
    }
    Err(report)
}

fn lint_all(root: &Root) -> Result<(), Message> {
    lint_files(root, &[])
}

fn architecture(root: &Root) -> Result<(), Message> {
    let workspace = Manifest::load(root, &RepoPath::new("Cargo.toml"))?;
    let mut report = Message::default();
    for member in workspace.members(root) {
        let path = if member.as_str() == Member::Root.name().as_str() {
            RepoPath::new(File::Manifest.name().as_str())
        } else {
            RepoPath::new(&format!("{member}/{}", File::Manifest.name()))
        };
        let manifest = Manifest::load(root, &path)?;
        let Some(package) = manifest.package() else {
            continue;
        };
        for dependency in manifest.dependencies() {
            if !arch::allowed_dependency(&package, &dependency) {
                report.push_line(&format!(
                    "{path}: {package} may not depend on {dependency}: the crate graph is xtask/src/arch.rs DEPENDENCIES, changed only by the owner"
                ));
            }
        }
    }
    if report.is_empty() {
        Ok(())
    } else {
        Err(report)
    }
}

fn rulebook(root: &Root) -> Result<(), Message> {
    let Some(vcs) = Vcs::detect(root) else {
        return Ok(());
    };
    let mut report = Message::default();
    for path in vcs.changed(root)? {
        let parent = vcs.parent_text(root, &path).unwrap_or_default();
        let current = Content::new(fs::read_to_string(root.join(&path)).unwrap_or_default());
        let changed = if arch::guarded(&path) {
            true
        } else if path.ends_with(File::Manifest.name().as_str()) {
            Manifest::parse(&parent).guarded() != Manifest::parse(&current).guarded()
        } else {
            false
        };
        if changed {
            report.push_line(&format!(
                "{path}: part of the rulebook, which only the owner changes; undo it and propose the change in your reply"
            ));
        }
    }
    if report.is_empty() {
        Ok(())
    } else {
        Err(report)
    }
}
