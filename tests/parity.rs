//! Old against new: every CLI and GUI scenario played with a base build and with this one, and
//! the transcripts, the GUI's dump lines and the screenshots compared byte for byte. A
//! behaviour-preserving change passes; anything that moves a pixel or a word fails.
//!
//! ```text
//! CODEMAP_BASE_BIN=<old codemap.exe> [CODEMAP_PARITY_REV=<rev>] cargo test --release --test parity -- --ignored
//! ```
//!
//! With CODEMAP_PARITY_REV the repo itself at that revision (its real map included) is played
//! as a second fixture; CODEMAP_PARITY_ONLY=<substring> plays only the scenarios whose name has
//! it. Differences are written under the scratch directory `parity/`.

mod common;

use std::path::{Path, PathBuf};

fn base() -> PathBuf {
    PathBuf::from(std::env::var_os("CODEMAP_BASE_BIN").expect("CODEMAP_BASE_BIN=<old codemap binary>"))
}

fn out_dir(name: &str) -> PathBuf {
    let d = common::scratch("parity").join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Compare and, on a difference, keep both sides for a diff tool.
fn same(what: &str, old: &str, new: &str, fails: &mut Vec<String>) {
    if old != new {
        let d = out_dir(what);
        std::fs::write(d.join("old.txt"), old).unwrap();
        std::fs::write(d.join("new.txt"), new).unwrap();
        let line = old.lines().zip(new.lines()).position(|(a, b)| a != b).unwrap_or(old.lines().count().min(new.lines().count()));
        fails.push(format!("{what}: differs at line {} (see {})", line + 1, d.display()));
    }
}

fn pngs(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap())).collect()).unwrap_or_default();
    v.sort();
    v
}

fn play_gui(bin: &Path, s: &common::gui::Scenario, keep: &Path) -> (String, Vec<(String, Vec<u8>)>) {
    let (err, after) = common::gui::play(bin, s);
    let shots = pngs(&common::gui::shots(s.name));
    let _ = std::fs::remove_dir_all(keep);
    std::fs::create_dir_all(keep).unwrap();
    for (n, b) in &shots {
        std::fs::write(keep.join(n), b).unwrap();
    }
    (common::gui_parity(&err) + &after, shots)
}

fn compare_gui(s: &common::gui::Scenario, old: &Path, new: &Path, fails: &mut Vec<String>) {
    let (o, oshots) = play_gui(old, s, &out_dir(s.name).join("old"));
    let (n, nshots) = play_gui(new, s, &out_dir(s.name).join("new"));
    same(s.name, &o, &n, fails);
    if oshots.len() != nshots.len() || oshots.is_empty() && s.script.contains("shot ") {
        fails.push(format!("{}: {} screenshots old, {} new", s.name, oshots.len(), nshots.len()));
    }
    for ((name, a), (_, b)) in oshots.iter().zip(&nshots) {
        if a != b {
            fails.push(format!("{}: {name} differs (both kept under {})", s.name, out_dir(s.name).display()));
        }
    }
}

#[test]
#[ignore]
fn old_and_new_agree() {
    let (old, new) = (base(), common::bin());
    let mut fails = Vec::new();
    let only = std::env::var("CODEMAP_PARITY_ONLY").unwrap_or_default();
    for (name, run) in common::cli::SCENARIOS.iter().filter(|(n, _)| n.contains(&only)) {
        let (o, n) = (run(&old), run(&new));
        same(name, &o, &n, &mut fails);
    }
    for s in common::gui::SCENARIOS.iter().filter(|s| s.name.contains(&only)) {
        compare_gui(s, &old, &new, &mut fails);
    }
    if let Some(rev) = std::env::var("CODEMAP_PARITY_REV").ok().filter(|_| "parity-repo".contains(only.as_str())) {
        repo_at(&rev, &old, &new, &mut fails);
    }
    assert!(fails.is_empty(), "{} differences:\n{}", fails.len(), fails.join("\n"));
}

/// This repo at `rev` as a fixture: a real codebase with a real map of 38 paths.
fn snapshot(rev: &str, name: &str) -> PathBuf {
    let root = common::scratch(name).join("repo");
    let _ = std::fs::remove_dir_all(common::scratch(name));
    std::fs::create_dir_all(&root).unwrap();
    let tar = common::scratch(name).join("snap.tar");
    let ok = std::process::Command::new("git").args(["archive", "--format=tar", "-o"]).arg(&tar).arg(rev).current_dir(env!("CARGO_MANIFEST_DIR")).status().unwrap();
    assert!(ok.success(), "git archive {rev}");
    let ok = std::process::Command::new("tar").arg("-xf").arg(&tar).arg("-C").arg(&root).status().unwrap();
    assert!(ok.success(), "tar");
    std::fs::remove_file(&tar).unwrap();
    root
}

fn repo_at(rev: &str, old: &Path, new: &Path, fails: &mut Vec<String>) {
    let transcript = |bin: &Path| -> String {
        let root = snapshot(rev, "parity-repo");
        let mut t = common::Transcript { bin, root: root.clone(), out: String::new() };
        let (paths, _, _) = common::codemap(bin, &root, &["paths"]);
        let names: Vec<String> = paths.lines().filter(|l| !l.starts_with(' ')).filter_map(|l| l.split(' ').next()).map(str::to_owned).collect();
        for args in [&["files"][..], &["symbols"], &["roots"], &["coverage"], &["uncovered"], &["stale"], &["paths"], &["grep", "fn [a-z_]+\\(&mut self"], &["notes", "stale"], &["tree", "frame", "3"], &["callers", "resolve"], &["callees", "exec"]] {
            t.run(args);
        }
        for n in &names {
            t.run(&["path", n]);
        }
        t.out
    };
    same("repo-cli", &transcript(old), &transcript(new), fails);
    fn setup(_: &Path, _: &str) -> PathBuf {
        snapshot(&std::env::var("CODEMAP_PARITY_REV").unwrap(), "parity-repo-gui")
    }
    let s = common::gui::Scenario {
        name: "parity-repo-gui",
        setup,
        hook: |_, _, _| {},
        after: &[],
        script: "SETTLE
dump
shot {shots}/document.png
click-id paths/25
wait 3
dump
shot {shots}/reader-gui.png
hover-id document
wheel -2000
wait 3
shot {shots}/reader-gui-scrolled.png
click-id tab@Graph
wait 5
dump
shot {shots}/graph.png
click-id graph-fit
wait 5
shot {shots}/graph-fit.png
click-id left@Symbols
wait 3
shot {shots}/symbols.png
click-id left@Files
wait 3
shot {shots}/files.png
click-id tab@Listing
wait 3
shot {shots}/listing.png
quit
",
    };
    compare_gui(&s, old, new, fails);
}
