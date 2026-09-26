mod common;

use common::Missing;
use common::gui::Scenario;
use std::path::{Path, PathBuf};

fn base() -> PathBuf {
    PathBuf::from(
        std::env::var_os("CODEMAP_BASE_BIN").expect("CODEMAP_BASE_BIN=<old codemap binary>"),
    )
}

fn out_dir(name: &str) -> PathBuf {
    let d = common::scratch("parity").join(name);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn same(what: &str, old: &str, new: &str, fails: &mut Vec<String>) {
    if old != new {
        let d = out_dir(what);
        std::fs::write(d.join("old.txt"), old).unwrap();
        std::fs::write(d.join("new.txt"), new).unwrap();
        fails.push(format!(
            "{what}: differs at line {} (see {})",
            common::first_difference(old, new) + 1,
            d.display()
        ));
    }
}

fn pngs(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| {
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        std::fs::read(e.path()).unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn play_gui(
    bin: &Path,
    name: &str,
    s: &Scenario,
    keep: &Path,
) -> Result<(String, Vec<(String, Vec<u8>)>), Missing> {
    let (err, after) = common::gui::play(bin, name, s)?;
    let shots = pngs(&common::gui::shots(name));
    let _ = std::fs::remove_dir_all(keep);
    std::fs::create_dir_all(keep).unwrap();
    for (n, b) in &shots {
        std::fs::write(keep.join(n), b).unwrap();
    }
    Ok((common::gui_parity(&err) + &after, shots))
}

fn compare_gui(name: &str, s: &Scenario, old: &Path, new: &Path, fails: &mut Vec<String>) {
    let played = play_gui(old, name, s, &out_dir(name).join("old"))
        .and_then(|o| Ok((o, play_gui(new, name, s, &out_dir(name).join("new"))?)));
    let ((o, oshots), (n, nshots)) = match played {
        Ok(p) => p,
        Err(m) => return common::skip(name, &m),
    };
    same(name, &o, &n, fails);
    if oshots.len() != nshots.len() || oshots.is_empty() && s.script.contains("shot ") {
        fails.push(format!(
            "{name}: {} screenshots old, {} new",
            oshots.len(),
            nshots.len()
        ));
    }
    for ((shot, a), (_, b)) in oshots.iter().zip(&nshots) {
        if a != b {
            fails.push(format!(
                "{name}: {shot} differs (both kept under {})",
                out_dir(name).display()
            ));
        }
    }
}

macro_rules! list {
    ($kind:ident: $($s:ident),* $(,)?) => {
        [$((concat!(stringify!($kind), "-", stringify!($s)), common::$kind::$s)),*]
    };
}

#[test]
#[ignore]
fn old_and_new_agree() {
    let (old, new) = (base(), common::bin());
    let mut fails = Vec::new();
    let only = std::env::var("CODEMAP_PARITY_ONLY").unwrap_or_default();
    let cli: &[(&str, fn(&Path, &str) -> Result<String, Missing>)] = &cli_scenarios!(list);
    for (name, run) in cli.iter().filter(|(n, _)| n.contains(&only)) {
        match run(&old, name).and_then(|o| Ok((o, run(&new, name)?))) {
            Ok((o, n)) => same(name, &o, &n, &mut fails),
            Err(m) => common::skip(name, &m),
        }
    }
    let gui: &[(&str, fn() -> Scenario)] = &gui_scenarios!(list);
    for (name, s) in gui.iter().filter(|(n, _)| n.contains(&only)) {
        compare_gui(name, &s(), &old, &new, &mut fails);
    }
    if let Some(rev) = std::env::var("CODEMAP_PARITY_REV")
        .ok()
        .filter(|_| "parity-repo".contains(only.as_str()))
    {
        repo_at(&rev, &old, &new, &mut fails);
    }
    assert!(
        fails.is_empty(),
        "{} differences:\n{}",
        fails.len(),
        fails.join("\n")
    );
}

fn snapshot(rev: &str, name: &str) -> PathBuf {
    let root = common::scratch(name).join("repo");
    let _ = std::fs::remove_dir_all(common::scratch(name));
    std::fs::create_dir_all(&root).unwrap();
    let tar = common::scratch(name).join("snap.tar");
    let ok = std::process::Command::new("git")
        .args(["archive", "--format=tar", "-o"])
        .arg(&tar)
        .arg(rev)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .unwrap();
    assert!(ok.success(), "git archive {rev}");
    let ok = std::process::Command::new("tar")
        .arg("-xf")
        .arg(&tar)
        .arg("-C")
        .arg(&root)
        .status()
        .unwrap();
    assert!(ok.success(), "tar");
    std::fs::remove_file(&tar).unwrap();
    root
}

fn repo_at(rev: &str, old: &Path, new: &Path, fails: &mut Vec<String>) {
    let transcript = |bin: &Path| -> String {
        let root = snapshot(rev, "parity-repo");
        let mut t = common::Transcript {
            bin,
            root: root.clone(),
            out: String::new(),
        };
        let (paths, _, _) = common::codemap(bin, &root, &["paths"]);
        let names: Vec<String> = paths
            .lines()
            .filter(|l| !l.starts_with(' '))
            .filter_map(|l| l.split(' ').next())
            .map(str::to_owned)
            .collect();
        for args in [
            &["files"][..],
            &["symbols"],
            &["roots"],
            &["coverage"],
            &["uncovered"],
            &["stale"],
            &["paths"],
            &["grep", "fn [a-z_]+\\(&mut self"],
            &["notes", "stale"],
            &["tree", "frame", "3"],
            &["callers", "resolve"],
            &["callees", "exec"],
        ] {
            t.run(args);
        }
        for n in &names {
            t.run(&["path", n]);
        }
        t.out
    };
    same("repo-cli", &transcript(old), &transcript(new), fails);
    fn setup(_: &Path, name: &str) -> Result<PathBuf, Missing> {
        Ok(snapshot(
            &std::env::var("CODEMAP_PARITY_REV").unwrap(),
            name,
        ))
    }
    let s = Scenario {
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
    compare_gui("parity-repo-gui", &s, old, new, fails);
}
