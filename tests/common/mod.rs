//! The fixture repo and the runners the integration tests share. Every run gets a PATH that
//! finds jj and git and no language server, so every file is indexed by tree-sitter: output is
//! the same on every machine and every run.

#![expect(
    dead_code,
    reason = "each test binary uses its own part of the shared harness"
)]

pub mod cli;
pub mod gui;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const FILES: &[(&str, &str)] = &[
    (
        "src/main.rs",
        r#"mod shapes;
mod store;

use shapes::{Circle, Square};
use store::Store;

fn main() {
    let mut store = Store::new();
    fill(&mut store);
    report(&store);
}

fn fill(store: &mut Store) {
    store.add(Box::new(Circle { r: 1.0 }));
    store.add(Box::new(Square { side: 2.0 }));
}

fn report(store: &Store) {
    let total = store.total_area();
    println!("{} shapes, {total:.2} area", store.len());
    log_line("done");
}

fn log_line(msg: &str) {
	eprintln!("{msg}");
}
"#,
    ),
    (
        "src/shapes.rs",
        r#"pub trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> &str;
}

pub struct Circle {
    pub r: f64,
}

pub struct Square {
    pub side: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.r * self.r
    }
    fn name(&self) -> &str {
        "circle"
    }
}

impl Shape for Square {
    fn area(&self) -> f64 {
        self.side * self.side
    }
    fn name(&self) -> &str {
        "square"
    }
}

/// Nothing calls this.
pub fn describe(s: &dyn Shape) -> String {
    format!("{} of area {:.2}", s.name(), s.area())
}
"#,
    ),
    (
        "src/store.rs",
        r#"use crate::shapes::Shape;

pub struct Store {
    items: Vec<Box<dyn Shape>>,
}

impl Store {
    pub fn new() -> Store {
        Store { items: Vec::new() }
    }

    pub fn add(&mut self, s: Box<dyn Shape>) {
        self.items.push(s);
        self.check();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn total_area(&self) -> f64 {
        let mut sum = 0.0;
        for s in &self.items {
            sum += s.area();
        }
        sum
    }

    fn check(&self) {
        assert!(self.len() < 1000, "too many shapes");
    }
}
"#,
    ),
    (
        "tools/stats.py",
        r#"import math
from helpers import mean


def spread(xs):
    m = mean(xs)
    return math.sqrt(sum((x - m) ** 2 for x in xs) / len(xs))


class Summary:
    def __init__(self, xs):
        self.xs = xs

    def show(self):
        print(mean(self.xs), spread(self.xs))


def main():
    Summary([1, 2, 3]).show()
"#,
    ),
    (
        "tools/helpers.py",
        "def mean(xs):\n    return sum(xs) / len(xs)\n",
    ),
    (
        "c/lib.c",
        r#"#include "lib.h"

static int square(int x) {
    return x * x;
}

int sum_squares(int n) {
    int s = 0;
    for (int i = 0; i < n; i++) {
        s += square(i);
    }
    return s;
}
"#,
    ),
    ("c/lib.h", "int sum_squares(int n);\n"),
    (
        "README.md",
        "# shapes\n\nA store of shapes and the sum of their areas.\n",
    ),
    (".gitignore", "build/\n"),
    ("build/generated.rs", "fn ignored() {}\n"),
    ("data.bin", "\0\x01binary"),
];

/// The agent's map of the fixture, as the CLI builds it.
pub const MAP: &[&[&str]] = &[
    &[
        "path-new",
        "startup",
        "flow",
        "What running the program does: fill the store, then report its total area.",
    ],
    &["path-add", "startup", "src/main.rs:main", "-1"],
    &["path-add", "startup", "fill"],
    &["path-add", "startup", "Store::add"],
    &["path-add", "startup", "check"],
    &["path-add", "startup", "report", "0"],
    &["path-add", "startup", "src/store.rs", "21", "25", "4"],
    &["path-add", "startup", "log_line", "4"],
    &[
        "step-note",
        "startup",
        "0",
        "Entry: builds the store and hands it to the two phases.",
    ],
    &["step-note", "startup", "1", "Adds one shape of each kind."],
    &[
        "step-note",
        "startup",
        "3",
        "Guards the size after every add.",
    ],
    &[
        "step-note",
        "startup",
        "5",
        "The sum over every shape's own area.",
    ],
    &[
        "path-new",
        "shapes",
        "type",
        "The Shape trait and the two shapes that implement it.",
    ],
    &["path-add", "shapes", "Shape", "-1"],
    &["path-add", "shapes", "impl Shape for Circle", "0"],
    &["path-add", "shapes", "impl Shape for Square", "0"],
    &["promote", "spread", "1", "stats"],
    &["path-new", "c-lib", "layer", "The C library's surface."],
    &["path-add", "c-lib", "c/lib.c", "7", "13", "-1"],
];

pub fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codemap"))
}

/// Where `tool` is on this process's PATH.
fn find_tool(tool: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let exe = if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_owned()
    };
    std::env::split_paths(&path)
        .map(|d| d.join(&exe))
        .find(|f| f.is_file())
}

/// A PATH that finds jj and git and no language server. On Unix it is one directory of links
/// to the two, since git's own directory can hold a server (`/usr/bin/clangd` on macOS); on
/// Windows it is their directories, since a link to git.exe does not run there.
fn tool_path() -> String {
    let tools: Vec<PathBuf> = ["jj", "git"].iter().filter_map(|t| find_tool(t)).collect();
    if cfg!(windows) {
        let dirs = tools
            .iter()
            .filter_map(|t| t.parent().map(Path::to_path_buf));
        return std::env::join_paths(dirs)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    let dir = scratch("tools");
    std::fs::create_dir_all(&dir).unwrap();
    for t in &tools {
        let link = dir.join(t.file_name().unwrap());
        if std::fs::read_link(&link).ok().as_ref() != Some(t) {
            let _ = std::fs::remove_file(&link);
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(t, &link); // a test running beside this one may have made it
        }
    }
    dir.to_string_lossy().into_owned()
}

/// A tool a scenario needs that is not on PATH: the scenario is skipped, not compared.
pub struct Missing(pub &'static str);

/// `Err` when `tool` is not on PATH, for a scenario to return early with `?`.
pub fn needs(tool: &'static str) -> Result<(), Missing> {
    find_tool(tool).map(|_| ()).ok_or(Missing(tool))
}

/// Says on stderr that scenario `name` did not run for want of a tool.
pub fn skip(name: &str, Missing(tool): &Missing) {
    eprintln!("{name}: skipped, {tool} is not on PATH");
}

/// A scratch directory for `name`, outside any repository so a fixture finds no VCS of its
/// own, and private to this process so concurrent runs do not share it; within the process it
/// is fixed, so a scenario played by two binaries in turn sees the same absolute paths.
pub fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join("codemap-tests")
        .join(std::process::id().to_string())
        .join(name)
}

/// Replaces the first `from` in `root/path` with `to`; `from` must be there.
pub fn edit_file(root: &Path, path: &str, from: &str, to: &str) {
    let p = root.join(path);
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains(from), "{path} has no {from:?}");
    std::fs::write(&p, text.replacen(from, to, 1)).unwrap();
}

/// A fresh fixture for `name`, with the map built when `map` is set.
pub fn fixture(name: &str, bin: &Path, map: bool) -> PathBuf {
    let root = scratch(name).join("repo");
    let _ = std::fs::remove_dir_all(scratch(name));
    for (path, text) in FILES {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    if map {
        for args in MAP {
            let (_, err, code) = codemap(bin, &root, args);
            assert_eq!(code, 0, "{args:?}: {err}");
        }
    }
    root
}

pub fn codemap(bin: &Path, root: &Path, args: &[&str]) -> (String, String, i32) {
    let out = Command::new(bin)
        .arg(root)
        .args(args)
        .env("PATH", tool_path())
        .output()
        .expect("run codemap");
    (
        rooted(&String::from_utf8_lossy(&out.stdout), root),
        rooted(&String::from_utf8_lossy(&out.stderr), root),
        out.status.code().unwrap_or(-1),
    )
}

/// `text` with Unix line ends and `root`, in either slash style, written as `<root>`.
fn rooted(text: &str, root: &Path) -> String {
    let r = root.display().to_string();
    text.replace("\r\n", "\n")
        .replace(&r, "<root>")
        .replace(&r.replace('\\', "/"), "<root>")
}

/// A git repo at the fixture root with the current state committed as HEAD.
pub fn git_commit(root: &Path, message: &str) {
    if !root.join(".git").exists() {
        git(root, &["init", "-q"]);
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
}

pub fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(root)
        .env("PATH", tool_path())
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A jj repo at the fixture root with the current state committed as the parent revision.
pub fn jj_commit(root: &Path, message: &str) {
    jj(root, &["git", "init"]);
    jj(root, &["commit", "-m", message]);
}

pub fn jj(root: &Path, args: &[&str]) -> String {
    let cfg = root.parent().unwrap().join("jj.toml");
    std::fs::write(
        &cfg,
        "[user]\nname = \"test\"\nemail = \"test@example.com\"\n",
    )
    .unwrap();
    let out = Command::new("jj")
        .args(args)
        .current_dir(root)
        .env("PATH", tool_path())
        .env("JJ_CONFIG", &cfg)
        .output()
        .expect("run jj");
    assert!(
        out.status.success() || args[0] == "git",
        "jj {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A shell-like record of commands and what they printed, for a golden file.
pub struct Transcript<'a> {
    pub bin: &'a Path,
    pub root: PathBuf,
    pub out: String,
}

impl Transcript<'_> {
    pub fn run(&mut self, args: &[&str]) -> i32 {
        let shown: Vec<String> = args
            .iter()
            .map(|a| {
                if a.contains(' ') || a.is_empty() {
                    format!("\"{a}\"")
                } else {
                    a.to_string()
                }
            })
            .collect();
        self.out
            .push_str(&format!("$ codemap {}\n", shown.join(" ")));
        let (out, err, code) = codemap(self.bin, &self.root, args);
        self.out.push_str(&out);
        for l in err.lines() {
            self.out.push_str(&format!("! {l}\n"));
        }
        if code != 0 {
            self.out.push_str(&format!("[exit {code}]\n"));
        }
        code
    }

    pub fn note(&mut self, s: &str) {
        self.out.push_str(&format!("# {s}\n"));
    }
}

/// Compare a scenario's output with `tests/golden/<name>.txt`, or write it when
/// CODEMAP_BLESS=1. A scenario missing a tool is skipped with the reason on stderr.
pub fn golden(name: &str, played: Result<String, Missing>) {
    let actual = match played {
        Ok(a) => a.replace('\r', ""),
        Err(m) => return skip(name, &m),
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"));
    if std::env::var("CODEMAP_BLESS").is_ok_and(|b| b == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no golden {}; run with CODEMAP_BLESS=1", path.display()))
        .replace("\r\n", "\n");
    if want != actual {
        let first = first_difference(&want, &actual);
        panic!(
            "{name} differs from its golden at line {}:\n  want: {:?}\n  got:  {:?}\n--- got in full ---\n{actual}",
            first + 1,
            want.lines().nth(first),
            actual.lines().nth(first)
        );
    }
}

/// The index of the first line where `a` and `b` differ.
pub fn first_difference(a: &str, b: &str) -> usize {
    a.lines()
        .zip(b.lines())
        .position(|(x, y)| x != y)
        .unwrap_or(a.lines().count().min(b.lines().count()))
}

/// Opens the GUI on `root` and plays `script` (see CLAUDE.md), `{shots}` standing for a
/// directory the screenshots go to. `hook` sees every stderr line as it arrives, so a test can
/// change files on disk at a point the script marks. Returns stderr.
pub fn gui(
    bin: &Path,
    root: &Path,
    name: &str,
    script: &str,
    hook: &mut dyn FnMut(&str),
) -> String {
    let dir = root.parent().unwrap();
    let shots = dir.join("shots");
    std::fs::create_dir_all(&shots).unwrap();
    let file = dir.join("script.txt");
    std::fs::write(
        &file,
        script.replace("{shots}", &shots.display().to_string().replace('\\', "/")),
    )
    .unwrap();
    let mut child = Command::new(bin)
        .arg(root)
        .env("PATH", tool_path())
        .env("CODEMAP_SCRIPT", &file)
        .env("JJ_CONFIG", dir.join("jj.toml"))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run codemap");
    let (tx, rx) = std::sync::mpsc::channel();
    let err = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(err).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let start = Instant::now();
    let mut out = String::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => {
                hook(&line);
                out.push_str(&line);
                out.push('\n');
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(_) if start.elapsed() > Duration::from_secs(90) => {
                let _ = child.kill();
                panic!("{name}: the GUI did not quit within 90 s\n{out}");
            }
            Err(_) => {}
        }
    }
    let status = child.wait().unwrap();
    assert!(status.success(), "{name}: GUI exited with {status}\n{out}");
    rooted(&out, root)
}

/// The lines of a GUI run that do not depend on the screen's size: selection, tooltip, peek,
/// status, backend state, node and button names, and script errors.
pub fn gui_state(stderr: &str) -> String {
    let mut out = String::new();
    for l in stderr.lines() {
        let keep = if l.starts_with("DUMP tab=")
            || l.starts_with("DUMP dock")
            || l.starts_with("DUMP tip=")
            || l.starts_with("DUMP backend")
            || l.starts_with("script:")
        {
            Some(l.to_owned())
        } else if l.starts_with("DUMP node ") || l.starts_with("DUMP button ") {
            Some(l.split(" rect=").next().unwrap_or(l).to_owned())
        } else if l.starts_with("DUMP graph") {
            Some(l.split(" pan=").next().unwrap_or(l).to_owned())
        } else {
            None
        };
        if let Some(k) = keep {
            out.push_str(&k);
            out.push('\n');
        }
    }
    out
}

/// Everything a GUI run printed that two builds of the same behaviour must agree on: all but
/// frame timings and where screenshots were written.
pub fn gui_parity(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|l| !l.starts_with("DUMP frames") && !l.starts_with("screenshot:"))
        .map(|l| format!("{l}\n"))
        .collect()
}
