//! The version control the root is kept in, reached by running its command line: jj when a
//! `.jj` directory is at or above the root, else git when a `.git` is. A repo colocated with
//! git has both and is jj's. There is no VCS library.

use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vcs {
    Jj,
    Git,
}

impl Vcs {
    pub fn detect(root: &Path) -> Option<Vcs> {
        let root = root.canonicalize().ok()?;
        for d in root.ancestors() {
            if d.join(".jj").is_dir() {
                return Some(Vcs::Jj);
            }
            if d.join(".git").exists() {
                return Some(Vcs::Git);
            }
        }
        None
    }

    pub fn name(self) -> &'static str {
        match self {
            Vcs::Jj => "jj",
            Vcs::Git => "git",
        }
    }

    /// The revision the working copy's changes sit on: `@-` in jj, `HEAD` in git.
    pub fn parent(self) -> &'static str {
        match self {
            Vcs::Jj => "@-",
            Vcs::Git => "HEAD",
        }
    }

    /// The bytes of `path`, relative to `root`, in revision `rev`. None when the file is not
    /// in that revision.
    pub fn show(self, root: &Path, rev: &str, path: &str) -> Option<Vec<u8>> {
        match self {
            Vcs::Jj => run(root, "jj", &["file", "show", "-r", rev, path]),
            Vcs::Git => run(root, "git", &["show", &format!("{rev}:./{path}")]),
        }
    }

    /// Every `.cmap` file directly in `dir`, relative to `root`, in revision `rev`, one after
    /// another in file name order. None when `rev` has none.
    pub fn show_dir(self, root: &Path, rev: &str, dir: &str) -> Option<Vec<u8>> {
        match self {
            Vcs::Jj => run(
                root,
                "jj",
                &["file", "show", "-r", rev, &format!("glob:{dir}/*.cmap")],
            )
            .filter(|b| !b.is_empty()),
            Vcs::Git => {
                let list = run(
                    root,
                    "git",
                    &["ls-tree", "--name-only", rev, "--", &format!("{dir}/")],
                )?;
                let mut files: Vec<String> = String::from_utf8_lossy(&list)
                    .lines()
                    .filter(|f| f.ends_with(".cmap"))
                    .map(|f| format!("{rev}:./{f}"))
                    .collect();
                if files.is_empty() {
                    return None;
                }
                files.sort();
                let mut args = vec!["show"];
                args.extend(files.iter().map(String::as_str));
                run(root, "git", &args)
            }
        }
    }
}

/// `program args` in `root`: its output when it succeeds.
fn run(root: &Path, program: &str, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}
