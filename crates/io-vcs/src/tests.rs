use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU32, Ordering};

use domain::{Line, RelativePath, Revision, Root};
use io_process::{Argument, Process, Program};

use crate::{Vcs, VcsError, VcsKind};

static SCRATCH_COUNT: AtomicU32 = AtomicU32::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let count = SCRATCH_COUNT.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("io-vcs-{label}-{}-{count}", process::id()));
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn put(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }

    fn run(&self, program: &str, arguments: &[&str]) {
        let arguments: Vec<Argument> = arguments
            .iter()
            .map(|argument| Argument::new(argument))
            .collect();
        let output = Process::run(&Program::new(program), &arguments, &self.0).unwrap();
        assert!(
            output.success(),
            "{program} {arguments:?}: {}",
            output.stderr.text()
        );
    }

    fn root(&self) -> Root {
        Root::new(&self.0)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

fn available(program: &str) -> bool {
    Program::new(program).find_on_path().is_some()
}

fn fill(scratch: &Scratch) {
    scratch.put(".codemap/b.cmap", "codemap 8\npath b\n");
    scratch.put(".codemap/a.cmap", "codemap 8\npath a\n");
    scratch.put(".codemap/notes.txt", "not a map file\n");
    scratch.put("src/lib.rs", "fn main() {\n\tbody();\n}\n");
}

fn check_revision(vcs: &Vcs) {
    let parent = vcs.parent();
    let map = vcs.map_at(&parent, &RelativePath::new(".codemap")).unwrap();
    assert_eq!(map.as_str(), "codemap 8\npath a\ncodemap 8\npath b\n");
    let text = vcs
        .file_at(&parent, &RelativePath::new("src/lib.rs"))
        .unwrap();
    assert_eq!(text.line(Line::new(1)).unwrap().as_str(), "    body();");
    assert!(matches!(
        vcs.file_at(&parent, &RelativePath::new("src/gone.rs")),
        Err(VcsError::Failure { .. })
    ));
    match vcs.map_at(&parent, &RelativePath::new("src")) {
        Err(VcsError::NoMapFiles {
            directory,
            revision,
        }) => {
            assert_eq!(directory.as_str(), "src");
            assert_eq!(revision, parent);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_directory_outside_any_repository_has_no_vcs() {
    let scratch = Scratch::new("none");
    assert!(matches!(
        Vcs::detect(&scratch.root()),
        Err(VcsError::NoRepository)
    ));
}

#[test]
fn a_missing_root_keeps_the_reason() {
    let root = Root::new(Path::new("/codemap-no-such-root"));
    assert!(matches!(
        Vcs::detect(&root),
        Err(VcsError::UnreadableRoot(_))
    ));
}

#[test]
fn git_reads_the_parent_revision() {
    if !available("git") {
        return;
    }
    let scratch = Scratch::new("git");
    scratch.run("git", &["init", "-q"]);
    fill(&scratch);
    scratch.run("git", &["add", "."]);
    scratch.run(
        "git",
        &[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-q",
            "-m",
            "first",
        ],
    );
    let vcs = Vcs::detect(&scratch.root()).unwrap();
    assert_eq!(vcs.kind(), VcsKind::Git);
    assert_eq!(vcs.parent(), Revision::new("HEAD"));
    assert_eq!(vcs.program().as_str(), "git");
    check_revision(&vcs);
}

#[test]
fn jj_reads_the_parent_revision() {
    if !available("jj") {
        return;
    }
    let scratch = Scratch::new("jj");
    let config = [
        "--config",
        "user.name=test",
        "--config",
        "user.email=test@example.com",
    ];
    scratch.run(
        "jj",
        &[&["git", "init", "--quiet"], config.as_slice()].concat(),
    );
    fill(&scratch);
    scratch.run("jj", &[&["new", "--quiet"], config.as_slice()].concat());
    let vcs = Vcs::detect(&scratch.root()).unwrap();
    assert_eq!(vcs.kind(), VcsKind::Jj);
    assert_eq!(vcs.parent(), Revision::new("@-"));
    assert_eq!(vcs.program().as_str(), "jj");
    check_revision(&vcs);
}

#[test]
fn a_subdirectory_finds_the_repository_above_it() {
    let scratch = Scratch::new("above");
    fs::create_dir_all(scratch.0.join(".git")).unwrap();
    fs::create_dir_all(scratch.0.join("inner/.jj")).unwrap();
    fs::create_dir_all(scratch.0.join("inner/deeper")).unwrap();
    let deeper = Root::new(&scratch.0.join("inner/deeper"));
    assert_eq!(Vcs::detect(&deeper).unwrap().kind(), VcsKind::Jj);
    let other = Root::new(&scratch.0.join("inner"));
    fs::remove_dir(scratch.0.join("inner/.jj")).unwrap();
    assert_eq!(Vcs::detect(&other).unwrap().kind(), VcsKind::Git);
}
