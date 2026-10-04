mod convert;
mod wire;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
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
    UnreadableMarker {
        path: PathBuf,
        error: io::Error,
    },
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
    marker: PathBuf,
}

impl Vcs {
    pub fn detect(root: &Root) -> Result<Self, VcsError> {
        let absolute = root
            .as_path()
            .canonicalize()
            .map_err(VcsError::UnreadableRoot)?;
        let (kind, marker) = absolute
            .ancestors()
            .find_map(|directory| {
                let jj = directory.join(wire::JJ_MARKER);
                let git = directory.join(wire::GIT_MARKER);
                if jj.is_dir() {
                    Some((VcsKind::Jj, jj))
                } else if git.exists() {
                    Some((VcsKind::Git, git))
                } else {
                    None
                }
            })
            .ok_or(VcsError::NoRepository)?;
        Ok(Self {
            kind,
            root: root.clone(),
            marker,
        })
    }

    pub fn shared_directory(&self) -> Result<PathBuf, VcsError> {
        let unreadable = |path: &Path| {
            let path = path.to_path_buf();
            move |error| VcsError::UnreadableMarker { path, error }
        };
        let pointed = |file: &Path, text: &str, base: &Path| -> Result<PathBuf, VcsError> {
            base.join(text.trim())
                .canonicalize()
                .map_err(unreadable(file))
        };
        match self.kind {
            VcsKind::Jj => {
                let repo = self.marker.join(wire::JJ_REPO);
                if repo.is_dir() {
                    return repo.canonicalize().map_err(unreadable(&repo));
                }
                let text = fs::read_to_string(&repo).map_err(unreadable(&repo))?;
                pointed(&repo, &text, &self.marker)
            }
            VcsKind::Git => {
                if self.marker.is_dir() {
                    return self.marker.canonicalize().map_err(unreadable(&self.marker));
                }
                let text = fs::read_to_string(&self.marker).map_err(unreadable(&self.marker))?;
                let base = self.marker.parent().unwrap_or(&self.marker);
                let directory = pointed(
                    &self.marker,
                    text.trim().trim_start_matches(wire::GIT_DIR_PREFIX),
                    base,
                )?;
                let common = directory.join(wire::GIT_COMMON_DIR);
                match fs::read_to_string(&common) {
                    Ok(relative) => pointed(&common, &relative, &directory),
                    Err(_) => Ok(directory),
                }
            }
        }
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
