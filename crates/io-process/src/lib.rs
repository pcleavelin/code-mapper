use std::env;
use std::fmt;
use std::io;
use std::iter;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Program(String);

impl Program {
    pub fn new(name: &str) -> Self {
        Self(name.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn find_on_path(&self) -> Option<PathBuf> {
        let named = Path::new(&self.0);
        if named.components().count() > 1 {
            return named.is_file().then(|| named.to_path_buf());
        }
        let suffixes = Self::executable_suffixes();
        env::split_paths(&env::var_os("PATH")?).find_map(|directory| {
            iter::once(String::new())
                .chain(suffixes.iter().cloned())
                .map(|suffix| directory.join(format!("{}{suffix}", self.0)))
                .find(|candidate| candidate.is_file())
        })
    }

    fn executable_suffixes() -> Vec<String> {
        if cfg!(windows) {
            env::var("PATHEXT")
                .unwrap_or_else(|_| ".EXE;.CMD;.BAT".to_owned())
                .split(';')
                .map(str::to_lowercase)
                .collect()
        } else {
            Vec::new()
        }
    }
}

impl fmt::Display for Program {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Argument(String);

impl Argument {
    pub fn new(argument: &str) -> Self {
        Self(argument.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Argument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Captured(Vec<u8>);

impl Captured {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0).into_owned()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Captured,
    pub stderr: Captured,
}

impl Output {
    pub fn success(&self) -> bool {
        self.status.success()
    }
}

#[derive(Debug)]
pub struct Spawned {
    pub child: Child,
    pub input: ChildStdin,
    pub output: ChildStdout,
}

#[derive(Debug)]
pub enum ProcessError {
    Missing(Program),
    Start { program: Program, error: io::Error },
    NoPipe(Program),
}

#[derive(Clone, Copy, Debug)]
pub struct Process;

impl Process {
    pub fn run(
        program: &Program,
        arguments: &[Argument],
        directory: &Path,
    ) -> Result<Output, ProcessError> {
        let output = command(program, arguments, directory)?
            .stdin(Stdio::null())
            .output()
            .map_err(|error| ProcessError::Start {
                program: program.clone(),
                error,
            })?;
        Ok(Output {
            status: output.status,
            stdout: Captured(output.stdout),
            stderr: Captured(output.stderr),
        })
    }

    pub fn spawn(
        program: &Program,
        arguments: &[Argument],
        directory: &Path,
    ) -> Result<Spawned, ProcessError> {
        let mut child = command(program, arguments, directory)?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| ProcessError::Start {
                program: program.clone(),
                error,
            })?;
        let pipes = child.stdin.take().zip(child.stdout.take());
        let Some((input, output)) = pipes else {
            drop(child.kill());
            drop(child.wait());
            return Err(ProcessError::NoPipe(program.clone()));
        };
        Ok(Spawned {
            child,
            input,
            output,
        })
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "io-process is the one place that starts processes"
)]
fn command(
    program: &Program,
    arguments: &[Argument],
    directory: &Path,
) -> Result<Command, ProcessError> {
    let executable = program
        .find_on_path()
        .ok_or_else(|| ProcessError::Missing(program.clone()))?;
    let mut command = Command::new(executable);
    command
        .args(arguments.iter().map(Argument::as_str))
        .current_dir(directory);
    Ok(command)
}

#[cfg(test)]
mod tests;
