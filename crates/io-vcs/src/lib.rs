mod convert;
mod wire;

use std::io;
use std::process::ExitStatus;

use domain::{FileText, RelativePath, Revision, Root};
use io_process::Process;

use crate::convert::{Invocation, file_text, revision_text};
use crate::wire::{CommandLine, TreeListing};

pub use io_process::{Captured, ProcessError, Program};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VcsKind {
    Jj,
    Git,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionText(String);

impl RevisionText {
    pub(crate) fn new(text: String) -> Self {
        Self(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub enum VcsError {
    NoRepository,
    UnreadableRoot(io::Error),
    Process(ProcessError),
    Failure {
        program: Program,
        status: ExitStatus,
        stderr: Captured,
    },
    NoMapFiles {
        directory: RelativePath,
        revision: Revision,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vcs {
    kind: VcsKind,
    root: Root,
}

impl Vcs {
    pub fn detect(root: &Root) -> Result<Self, VcsError> {
        let absolute = root
            .as_path()
            .canonicalize()
            .map_err(VcsError::UnreadableRoot)?;
        let kind = absolute
            .ancestors()
            .find_map(|directory| {
                if directory.join(wire::JJ_MARKER).is_dir() {
                    Some(VcsKind::Jj)
                } else if directory.join(wire::GIT_MARKER).exists() {
                    Some(VcsKind::Git)
                } else {
                    None
                }
            })
            .ok_or(VcsError::NoRepository)?;
        Ok(Self {
            kind,
            root: root.clone(),
        })
    }

    pub fn kind(&self) -> VcsKind {
        self.kind
    }

    pub fn program(&self) -> Program {
        match self.kind {
            VcsKind::Jj => Program::new(wire::JJ),
            VcsKind::Git => Program::new(wire::GIT),
        }
    }

    pub fn parent(&self) -> Revision {
        match self.kind {
            VcsKind::Jj => Revision::new(wire::JJ_PARENT),
            VcsKind::Git => Revision::new(wire::GIT_PARENT),
        }
    }

    pub fn file_at(&self, revision: &Revision, file: &RelativePath) -> Result<FileText, VcsError> {
        let line = match self.kind {
            VcsKind::Jj => wire::jj_show_file(revision.as_str(), file.as_str()),
            VcsKind::Git => wire::git_show_files(revision.as_str(), &[file.as_str().to_owned()]),
        };
        self.run(&line).map(|stdout| file_text(&stdout))
    }

    pub fn map_at(
        &self,
        revision: &Revision,
        directory: &RelativePath,
    ) -> Result<RevisionText, VcsError> {
        let no_map_files = || VcsError::NoMapFiles {
            directory: directory.clone(),
            revision: revision.clone(),
        };
        let stdout = match self.kind {
            VcsKind::Jj => self.run(&wire::jj_show_map_files(
                revision.as_str(),
                directory.as_str(),
            ))?,
            VcsKind::Git => {
                let tree = self.run(&wire::git_list_tree(revision.as_str(), directory.as_str()))?;
                let listing = TreeListing::parse(&tree.text());
                if listing.map_files.is_empty() {
                    return Err(no_map_files());
                }
                self.run(&wire::git_show_files(revision.as_str(), &listing.map_files))?
            }
        };
        if stdout.is_empty() {
            return Err(no_map_files());
        }
        Ok(revision_text(&stdout))
    }

    fn run(&self, line: &CommandLine) -> Result<Captured, VcsError> {
        let invocation = Invocation::from(line);
        let output = Process::run(
            &invocation.program,
            &invocation.arguments,
            self.root.as_path(),
        )
        .map_err(VcsError::Process)?;
        if output.success() {
            Ok(output.stdout)
        } else {
            Err(VcsError::Failure {
                program: invocation.program,
                status: output.status,
                stderr: output.stderr,
            })
        }
    }
}

#[cfg(test)]
mod tests;
