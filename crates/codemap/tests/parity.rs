#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::disallowed_methods,
    reason = "a test harness fails by panicking, builds fixtures on disk and runs the binary"
)]

mod common;

use common::FirstScreen;
use common::Missing;
use common::Outcome;
use common::gui::Scenario;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn base() -> PathBuf {
    PathBuf::from(env::var_os("CODEMAP_BASE_BIN").expect("CODEMAP_BASE_BIN=<old codemap binary>"))
}

fn out_dir(name: &str) -> PathBuf {
    let dir = common::scratch("parity").join(name);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn same(what: &str, old: &str, new: &str, fails: &mut Vec<String>) {
    if old != new {
        let dir = out_dir(what);
        fs::write(dir.join("old.txt"), old).unwrap();
        fs::write(dir.join("new.txt"), new).unwrap();
        fails.push(format!(
            "{what}: differs at line {} (see {})",
            common::first_difference(old, new) + 1,
            dir.display()
        ));
    }
}

fn pngs(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut shots: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| {
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        fs::read(entry.path()).unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    shots.sort();
    shots
}

struct Played {
    text: String,
    shots: Vec<(String, Vec<u8>)>,
}

fn play_gui(bin: &Path, name: &str, scenario: &Scenario, keep: &Path) -> Result<Played, Missing> {
    let (err, after) = common::gui::play(bin, name, scenario)?;
    let shots = pngs(&common::gui::shots(name));
    fs::remove_dir_all(keep).ok();
    fs::create_dir_all(keep).unwrap();
    for (shot_name, bytes) in &shots {
        fs::write(keep.join(shot_name), bytes).unwrap();
    }
    Ok(Played {
        text: common::gui_parity(&err) + &after,
        shots,
    })
}

fn compare_gui(name: &str, scenario: &Scenario, old: &Path, new: &Path, fails: &mut Vec<String>) {
    let played = play_gui(old, name, scenario, &out_dir(name).join("old")).and_then(|old_played| {
        Ok((
            old_played,
            play_gui(new, name, scenario, &out_dir(name).join("new"))?,
        ))
    });
    let (played_old, played_new) = match played {
        Ok(pair) => pair,
        Err(missing) => return common::skip(name, &missing),
    };
    same(name, &played_old.text, &played_new.text, fails);
    fails.extend(
        played_new
            .text
            .lines()
            .filter(|line| line.starts_with("script:"))
            .map(|line| format!("{name}: the new build's script failed: {line}")),
    );
    if played_old.shots.len() != played_new.shots.len()
        || played_old.shots.is_empty() && scenario.script.contains("shot ")
    {
        fails.push(format!(
            "{name}: {} screenshots old, {} new",
            played_old.shots.len(),
            played_new.shots.len()
        ));
    }
    for ((shot, old_bytes), (_, new_bytes)) in played_old.shots.iter().zip(&played_new.shots) {
        if old_bytes != new_bytes {
            fails.push(format!(
                "{name}: {shot} differs (both kept under {})",
                out_dir(name).display()
            ));
        }
    }
}

struct CliScenario {
    name: &'static str,
    run: fn(&Path, &str) -> Result<String, Missing>,
}

struct GuiScenario {
    name: &'static str,
    build: fn() -> Scenario,
}

macro_rules! cli_list {
    ($kind:ident: $($s:ident),* $(,)?) => {
        [$(CliScenario {
            name: concat!(stringify!($kind), "-", stringify!($s)),
            run: |bin: &Path, name: &str| common::$kind::$s(bin, name).outcome(),
        }),*]
    };
}

macro_rules! gui_list {
    ($kind:ident: $($s:ident),* $(,)?) => {
        [$(GuiScenario {
            name: concat!(stringify!($kind), "-", stringify!($s)),
            build: common::$kind::$s,
        }),*]
    };
}

#[test]
#[ignore = "needs CODEMAP_BASE_BIN (and CODEMAP_PARITY_REV for the repo scenario); run explicitly"]
fn old_and_new_agree() {
    let (old, new) = (base(), common::bin());
    let mut fails = Vec::new();
    let only = env::var("CODEMAP_PARITY_ONLY").unwrap_or_default();
    let cli = cli_scenarios!(cli_list);
    for scenario in cli.iter().filter(|scenario| scenario.name.contains(&only)) {
        let played = (scenario.run)(&old, scenario.name)
            .and_then(|old_text| Ok((old_text, (scenario.run)(&new, scenario.name)?)));
        match played {
            Ok((old_text, new_text)) => same(scenario.name, &old_text, &new_text, &mut fails),
            Err(missing) => common::skip(scenario.name, &missing),
        }
    }
    let gui = gui_scenarios!(gui_list);
    for scenario in gui.iter().filter(|scenario| scenario.name.contains(&only)) {
        compare_gui(scenario.name, &(scenario.build)(), &old, &new, &mut fails);
    }
    if let Some(rev) = env::var("CODEMAP_PARITY_REV")
        .ok()
        .filter(|_| "parity-repo".contains(only.as_str()))
    {
        repo_at(&rev, &old, &new, &mut fails);
    }
    assert_eq!(
        fails.len(),
        0,
        "{} differences:\n{}",
        fails.len(),
        fails.join("\n")
    );
}

fn snapshot(rev: &str, name: &str) -> PathBuf {
    let root = common::scratch(name).join("repo");
    fs::remove_dir_all(common::scratch(name)).ok();
    fs::create_dir_all(&root).unwrap();
    let tar = common::scratch(name).join("snap.tar");
    let mut git = Command::new("git");
    if let Some(store) = env::var_os("CODEMAP_PARITY_GIT_DIR") {
        git.arg(format!("--git-dir={}", store.to_string_lossy()));
    }
    let git_status = git
        .args(["archive", "--format=tar", "-o"])
        .arg(&tar)
        .arg(rev)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .status()
        .unwrap();
    assert!(git_status.success(), "git archive {rev}");
    let tar_status = Command::new("tar")
        .arg("-xf")
        .arg(&tar)
        .arg("-C")
        .arg(&root)
        .status()
        .unwrap();
    assert!(tar_status.success(), "tar");
    fs::remove_file(&tar).unwrap();
    root
}

fn repo_at(rev: &str, old: &Path, new: &Path, fails: &mut Vec<String>) {
    fn setup(_: &Path, name: &str) -> PathBuf {
        snapshot(&env::var("CODEMAP_PARITY_REV").unwrap(), name)
    }
    let transcript = |bin: &Path| -> String {
        let root = snapshot(rev, "parity-repo");
        let mut record = common::Transcript {
            bin,
            root: root.clone(),
            out: String::new(),
        };
        let (tours, _, _) = common::codemap(bin, &root, &["tours"]);
        let names: Vec<String> = tours
            .lines()
            .filter(|line| !line.starts_with(' '))
            .filter_map(|line| line.split(' ').next())
            .map(str::to_owned)
            .collect();
        for args in [
            &["files"][..],
            &["symbols"],
            &["roots"],
            &["coverage"],
            &["uncovered"],
            &["stale"],
            &["tours"],
            &["search", "fn [a-z_]+\\(&mut self"],
            &["notes", "stale"],
            &["tree", "frame", "3"],
            &["callers", "resolve"],
            &["callees", "exec"],
        ] {
            record.run(args);
        }
        for tour_name in &names {
            record.run(&["tour", tour_name]);
        }
        record.out
    };
    same("repo-cli", &transcript(old), &transcript(new), fails);
    let scenario = Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(setup(bin, name)),
        hook: |_, _, _| {},
        after: &[],
        script: "idle
dump
shot {shots}/document.png
hover-id document
wheel -2000
wait 3
dump
shot {shots}/document-scrolled.png
click-id tab@Graph
wait 5
dump
shot {shots}/graph.png
click-id graph-fit
wait 5
shot {shots}/graph-fit.png
click-id tab@Symbols
wait 3
shot {shots}/symbols.png
click-id tab@Files
wait 3
shot {shots}/files.png
click-id tab@Source
wait 3
shot {shots}/source.png
quit
",
    };
    compare_gui("parity-repo-gui", &scenario, old, new, fails);
}
