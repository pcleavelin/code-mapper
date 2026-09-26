use crate::process::{Outcome, run};
use crate::text::{Argument, Content, Message, Program, RepoPath, Root};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Vcs {
    Jj,
    Git,
}

impl Vcs {
    pub(crate) fn detect(root: &Root) -> Option<Self> {
        if root.path().join(".jj").is_dir() {
            Some(Self::Jj)
        } else if root.path().join(".git").exists() {
            Some(Self::Git)
        } else {
            None
        }
    }

    pub(crate) fn changed(self, root: &Root) -> Result<Vec<RepoPath>, Message> {
        let lists = match self {
            Self::Jj => vec![run(
                root,
                Program::JJ,
                &Argument::list(&["diff", "--name-only"]),
            )],
            Self::Git => vec![
                run(
                    root,
                    Program::GIT,
                    &Argument::list(&["diff", "--name-only", "HEAD"]),
                ),
                run(
                    root,
                    Program::GIT,
                    &Argument::list(&["ls-files", "--others", "--exclude-standard"]),
                ),
            ],
        };
        let mut paths = Vec::new();
        for list in lists {
            if list.outcome == Outcome::Failure {
                return Err(list.output);
            }
            paths.extend(
                list.output
                    .as_str()
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(RepoPath::new),
            );
        }
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    pub(crate) fn parent_text(self, root: &Root, path: &RepoPath) -> Option<Content> {
        let found = match self {
            Self::Jj => run(
                root,
                Program::JJ,
                &Argument::list(&["file", "show", "-r", "@-", path.as_str()]),
            ),
            Self::Git => run(
                root,
                Program::GIT,
                &Argument::list(&["show", &format!("HEAD:{path}")]),
            ),
        };
        (found.outcome == Outcome::Success).then(|| Content::new(found.output.as_str()))
    }
}
