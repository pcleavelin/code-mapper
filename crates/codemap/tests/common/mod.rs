#![expect(
    dead_code,
    reason = "each test binary uses its own part of the shared harness"
)]

pub(crate) mod cli;
pub(crate) mod gui;

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub(crate) trait Outcome {
    fn outcome(self) -> Result<String, Missing>;
}

impl Outcome for String {
    fn outcome(self) -> Result<String, Missing> {
        Ok(self)
    }
}

impl Outcome for Result<String, Missing> {
    fn outcome(self) -> Result<String, Missing> {
        self
    }
}

pub(crate) const FILES: &[(&str, &str)] = &[
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
        r"import math
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
",
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

pub(crate) const MAP: &[&[&str]] = &[
    &[
        "tour-new",
        "startup",
        "flow",
        "What running the program does: fill the store, then report its total area.",
    ],
    &["tour-add", "startup", "src/main.rs:main", "-1"],
    &["tour-add", "startup", "fill"],
    &["tour-add", "startup", "Store::add"],
    &["tour-add", "startup", "check"],
    &["tour-add", "startup", "report", "0"],
    &["tour-add", "startup", "src/store.rs", "21", "25", "4"],
    &["tour-add", "startup", "log_line", "4"],
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
        "tour-new",
        "shapes",
        "data",
        "The Shape trait and the two shapes that implement it.",
    ],
    &["tour-add", "shapes", "Shape", "-1"],
    &["tour-add", "shapes", "impl Shape for Circle", "0"],
    &["tour-add", "shapes", "impl Shape for Square", "0"],
    &["promote", "spread", "1", "stats"],
    &["tour-new", "c-lib", "layer", "The C library's surface."],
    &["tour-add", "c-lib", "c/lib.c", "7", "13", "-1"],
];

pub(crate) fn bin() -> PathBuf {
    env::var_os("CODEMAP_BIN").map_or_else(
        || PathBuf::from(env!("CARGO_BIN_EXE_codemap")),
        PathBuf::from,
    )
}

fn find_tool(tool: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH").unwrap_or_default();
    let exe = if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_owned()
    };
    env::split_paths(&path)
        .map(|dir| dir.join(&exe))
        .find(|file| file.is_file())
}

fn tool_path() -> String {
    let tools: Vec<PathBuf> = ["jj", "git"]
        .iter()
        .filter_map(|tool| find_tool(tool))
        .collect();
    if cfg!(windows) {
        let dirs = tools
            .iter()
            .filter_map(|found| found.parent().map(Path::to_path_buf));
        return env::join_paths(dirs)
            .map(|joined| joined.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
    let dir = scratch("tools");
    fs::create_dir_all(&dir).unwrap();
    for found in &tools {
        let link = dir.join(found.file_name().unwrap());
        if fs::read_link(&link).ok().as_ref() != Some(found) {
            fs::remove_file(&link).ok();
            #[cfg(unix)]
            symlink(found, &link).ok();
        }
    }
    dir.to_string_lossy().into_owned()
}

pub(crate) struct Missing(&'static str);

pub(crate) fn needs(tool: &'static str) -> Result<(), Missing> {
    find_tool(tool).map(|_| ()).ok_or(Missing(tool))
}

pub(crate) fn skip(name: &str, Missing(tool): &Missing) {
    writeln!(
        io::stderr().lock(),
        "{name}: skipped, {tool} is not on PATH"
    )
    .ok();
}

pub(crate) fn scratch(name: &str) -> PathBuf {
    env::temp_dir()
        .join("codemap-tests")
        .join(process::id().to_string())
        .join(name)
}

pub(crate) fn edit_file(root: &Path, path: &str, from: &str, to: &str) {
    let file = root.join(path);
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains(from), "{path} has no {from:?}");
    fs::write(&file, text.replacen(from, to, 1)).unwrap();
}

pub(crate) fn fixture(name: &str, bin: &Path, map: bool) -> PathBuf {
    let root = scratch(name).join("repo");
    fs::remove_dir_all(scratch(name)).ok();
    for (path, text) in FILES {
        let file_path = root.join(path);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(file_path, text).unwrap();
    }
    if map {
        for args in MAP {
            let (_, err, code) = codemap(bin, &root, args);
            assert_eq!(code, 0, "{args:?}: {err}");
        }
    }
    root
}

pub(crate) fn codemap(bin: &Path, root: &Path, args: &[&str]) -> (String, String, i32) {
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

fn rooted(text: &str, root: &Path) -> String {
    let root_text = root.display().to_string();
    text.replace("\r\n", "\n")
        .replace(&root_text, "<root>")
        .replace(&root_text.replace('\\', "/"), "<root>")
}

pub(crate) fn git_commit(root: &Path, message: &str) {
    if !root.join(".git").exists() {
        git(root, &["init", "-q"]);
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
}

pub(crate) fn git(root: &Path, args: &[&str]) -> String {
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

pub(crate) fn jj_commit(root: &Path, message: &str) {
    jj(root, &["git", "init"]);
    jj(root, &["commit", "-m", message]);
}

pub(crate) fn jj(root: &Path, args: &[&str]) -> String {
    let cfg = root.parent().unwrap().join("jj.toml");
    fs::write(
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

pub(crate) struct Transcript<'a> {
    pub bin: &'a Path,
    pub root: PathBuf,
    pub out: String,
}

impl Transcript<'_> {
    pub(crate) fn run(&mut self, args: &[&str]) -> i32 {
        let shown: Vec<String> = args
            .iter()
            .map(|arg| {
                if arg.contains(' ') || arg.is_empty() {
                    format!("\"{arg}\"")
                } else {
                    arg.to_string()
                }
            })
            .collect();
        writeln!(self.out, "$ codemap {}", shown.join(" ")).ok();
        let (out, err, code) = codemap(self.bin, &self.root, args);
        self.out.push_str(&out);
        for line in err.lines() {
            writeln!(self.out, "! {line}").ok();
        }
        if code != 0 {
            writeln!(self.out, "[exit {code}]").ok();
        }
        code
    }

    pub(crate) fn note(&mut self, message: &str) {
        writeln!(self.out, "# {message}").ok();
    }
}

pub(crate) fn ran(name: &str, played: Result<String, Missing>) {
    let output = match played {
        Ok(text) => text,
        Err(missing) => return skip(name, &missing),
    };
    let crashed: Vec<&str> = output
        .lines()
        .filter(|line| {
            line.contains("panicked at") || *line == "[exit 101]" || *line == "[exit -1]"
        })
        .collect();
    assert_eq!(
        crashed.len(),
        0,
        "{name}: codemap crashed:\n{}\n--- output in full ---\n{output}",
        crashed.join("\n")
    );
    let failed: Vec<&str> = output
        .lines()
        .filter(|line| line.starts_with("script:"))
        .collect();
    assert_eq!(
        failed.len(),
        0,
        "{name}: the script failed:\n{}",
        failed.join("\n")
    );
}

pub(crate) fn first_difference(left: &str, right: &str) -> usize {
    left.lines()
        .zip(right.lines())
        .position(|(from_left, from_right)| from_left != from_right)
        .unwrap_or(left.lines().count().min(right.lines().count()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FirstScreen {
    Workspace,
    Welcome,
}

pub(crate) fn gui(
    bin: &Path,
    root: &Path,
    name: &str,
    script: &str,
    first: FirstScreen,
    hook: &mut dyn FnMut(&str),
) -> String {
    let dir = root.parent().unwrap();
    let shots = dir.join("shots");
    fs::create_dir_all(&shots).unwrap();
    let file = dir.join("script.txt");
    fs::write(
        &file,
        script.replace("{shots}", &shots.display().to_string().replace('\\', "/")),
    )
    .unwrap();
    let mut command = Command::new(bin);
    command
        .arg(root)
        .env("PATH", tool_path())
        .env("CODEMAP_SCRIPT", &file)
        .env("JJ_CONFIG", dir.join("jj.toml"));
    if first == FirstScreen::Welcome {
        command.env("CODEMAP_OPENING", "board");
    }
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run codemap");
    let (tx, rx) = mpsc::channel();
    let err = child.stderr.take().unwrap();
    thread::spawn(move || {
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
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(_) if start.elapsed() > Duration::from_secs(90) => {
                child.kill().ok();
                panic!("{name}: the GUI did not quit within 90 s\n{out}");
            }
            Err(_) => {}
        }
    }
    let status = child.wait().unwrap();
    assert!(status.success(), "{name}: GUI exited with {status}\n{out}");
    rooted(&out, root)
}

pub(crate) fn gui_parity(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| !line.starts_with("DUMP frames") && !line.starts_with("screenshot:"))
        .fold(String::new(), |mut acc, line| {
            writeln!(acc, "{line}").ok();
            acc
        })
}
