//! GUI scenarios: scripts played against a fresh fixture (see CLAUDE.md for the commands).
//!
//! A script may aim at a rectangle a previous `dump` or `rect` printed: `<<prefix>>` is the
//! centre of the last stderr line starting with `prefix`, `<<prefix|dx,dy>>` a point offset from
//! its top-left corner. Each is resolved by a probe run of the script up to that line, on a fresh
//! fixture, so the final run starts from the same state. Offsets are pixels: the scripts assume
//! the owner's display (16 px rows at 100% scale).
//!
//! Every script settles with `idle` and a save before its first dump: with several servers
//! missing, the status line otherwise says whichever failure arrived last.

use super::{Missing, Transcript, codemap, edit_file, fixture, gui, jj_commit, needs, scratch};
use std::path::{Path, PathBuf};

/// Every GUI scenario, handed to `$m` as `gui: <fn>, ...`. Each is a `fn() -> Scenario` below,
/// and its golden is `gui-<fn>`.
#[macro_export]
macro_rules! gui_scenarios {
    ($m:ident) => {
        $m! { gui: document, peek, listing, graph, panels, delete, diff, reload, dock }
    };
}

pub struct Scenario {
    /// Builds the fixture for a scenario name; `Err` when it needs a tool that is missing.
    pub setup: fn(&Path, &str) -> Result<PathBuf, Missing>,
    pub script: &'static str,
    /// Sees every stderr line of the run; a `rect <marker>` line in the script is the cue.
    pub hook: fn(&Path, &Path, &str),
    /// CLI commands run after the GUI quits, their output appended to the golden.
    pub after: &'static [&'static [&'static str]],
}

fn mapped(bin: &Path, name: &str) -> Result<PathBuf, Missing> {
    Ok(fixture(name, bin, true))
}

fn with_parent(bin: &Path, name: &str) -> Result<PathBuf, Missing> {
    needs("jj")?;
    let root = fixture(name, bin, true);
    jj_commit(&root, "base");
    for args in [
        &["step-note", "startup", "0", "A changed note."][..],
        &["path-add", "startup", "describe", "-1"],
        &["path-rm", "c-lib"],
        &["path-new", "fresh", "layer", "New here."],
        &["path-add", "fresh", "mean", "-1"],
    ] {
        codemap(bin, &root, args);
    }
    Ok(root)
}

fn no_hook(_: &Path, _: &Path, _: &str) {}

/// The reload scenario's cues: a source edit, then a map edit through the CLI.
fn reload_hook(bin: &Path, root: &Path, line: &str) {
    if line.starts_with("DUMP rect edit-source") {
        edit_file(root, "src/store.rs", "< 1000", "< 2000");
    }
    if line.starts_with("DUMP rect edit-map") {
        codemap(
            bin,
            root,
            &["path-note", "shapes", "Edited while the window was open."],
        );
    }
}

// the scripts read startup, which is third in the paths list: c-lib, shapes, startup, stats
const SETTLE: &str = "idle\nclick-id save\nclick-id paths/2\nwait 2\n";

pub fn document() -> Scenario {
    Scenario {
        setup: mapped,
        hook: no_hook,
        after: &[],
        script: "SETTLE
dump
shot {shots}/open.png
click-id step/5
wait 2
dump
click-id whole/5
wait 2
shot {shots}/whole.png
click-id ctx-a/5
wait 2
click-id ctx-b/5
wait 2
dump
shot {shots}/context.png
click-id ctx0/5
wait 2
click-id hide/1
wait 2
click-id fold/1
wait 2
dump
shot {shots}/fold.png
key down
wait 2
key down
wait 2
dump
key up
wait 2
dump
click-id crumb/0
wait 2
dump
click-id doc-collapse
wait 2
shot {shots}/collapsed.png
click-id doc-expand
wait 2
click-id doc-fold
wait 2
dump
shot {shots}/folded-all.png
click-id doc-unfold
wait 2
hover-id document
wheel -300
wait 3
dump
shot {shots}/scrolled.png
click-id outline/6
wait 3
dump
shot {shots}/outline.png
click-id doc-graph
wait 3
dump
quit
",
    }
}

pub fn peek() -> Scenario {
    Scenario {
        setup: mapped,
        hook: no_hook,
        after: &[],
        script: "SETTLE
rect doccode/1
mouse <<DUMP rect doccode/1|132,24>>
wait 3
dump
shot {shots}/tooltip.png
click <<DUMP rect doccode/1|132,24>> alt
wait 3
dump
shot {shots}/peek.png
mouse 5 5
wait 2
click-id peek-go
wait 3
dump
click-id peek-x
wait 2
click <<DUMP rect doccode/1|132,24>> ctrl
wait 3
dump
shot {shots}/jumped.png
rect doccode/4
dblclick <<DUMP rect doccode/4|244,24>>
wait 3
dump
rect doccode/4
click <<DUMP rect doccode/4|108,56>> alt
wait 3
dump
shot {shots}/peek-line.png
click-id xfrom/0
wait 3
dump
click-id xto/0
wait 3
dump
quit
",
    }
}

pub fn listing() -> Scenario {
    Scenario {
        setup: mapped,
        hook: no_hook,
        after: &[&["paths", "startup"]],
        script: "SETTLE
open src/store.rs 12
wait 3
dump
shot {shots}/open.png
rect lines
click <<DUMP rect lines|200,8>>
wait 2
click <<DUMP rect lines|200,40>> shift
wait 2
dump
shot {shots}/selected.png
click-id pin
wait 2
dump
click-id field@goto-line
text 30
key enter
wait 3
dump
text 999
key enter
wait 2
dump
text abc
key enter
wait 2
dump
key escape
rect lines
mouse <<DUMP rect lines|204,8>>
wait 3
dump
shot {shots}/tooltip.png
click <<DUMP rect lines|204,8>> ctrl
wait 3
dump
click-id back
wait 2
dump
click-id forward
wait 2
dump
key left alt
wait 2
dump
key right ctrl
wait 2
dump
click-id save
wait 2
quit
",
    }
}

pub fn graph() -> Scenario {
    Scenario {
        setup: mapped,
        hook: no_hook,
        after: &[],
        script: "SETTLE
click-id tab@Graph
wait 4
dump
shot {shots}/graph.png
click <<DUMP button main 'callees>>
wait 4
dump
shot {shots}/expanded.png
click <<DUMP node new|30,8>>
wait 4
dump
shot {shots}/off-path.png
click <<DUMP button main 'hide>>
wait 4
dump
click <<DUMP button new 'hide>>
wait 4
dump
drag <<DUMP node 1.2 report|30,8>> <<DUMP node 1.2 report|130,108>>
wait 4
dump
shot {shots}/dragged.png
click-id graph-auto
wait 4
hover-id graph-canvas
wheel 120 ctrl
wait 4
dump
shot {shots}/zoomed.png
click-id graph-fit
wait 4
dump
shot {shots}/fit.png
click-id graph-1to1
wait 4
dump
drag <<DUMP graph zoom|100,300>> <<DUMP graph zoom|300,300>>
hover-id graph-canvas
wheel -200
wait 4
dump
click-id graph-fit
wait 4
dump
click <<DUMP button fill '▼>>
wait 4
dump
shot {shots}/context.png
click <<DUMP button fill 'no context>>
wait 4
click <<DUMP button fill 'listing>>
wait 4
dump
quit
",
    }
}

pub fn panels() -> Scenario {
    Scenario {
        setup: mapped,
        hook: no_hook,
        after: &[&["paths"]],
        script: "SETTLE
click-id left@Symbols
wait 3
shot {shots}/symbols.png
click-id field@symbols
text area
wait 3
dump
shot {shots}/filtered.png
click-id sym@5:5
wait 3
dump
click-id left@Files
wait 3
shot {shots}/files.png
click-id dir@src
wait 3
dump
shot {shots}/files-closed.png
click-id dir@src
wait 2
click-id file/5
wait 3
dump
click-id left@Paths
wait 2
click-id paths/1
wait 3
dump
shot {shots}/shapes.png
click-id field@search
text self\\.\\w+
key enter
wait 3
dump
shot {shots}/results.png
click-id hit/2
wait 3
dump
click-id cmd
text paths startup
key enter
wait 3
shot {shots}/output.png
text path-new fresh layer
key enter
wait 3
dump
shot {shots}/dirty.png
text bogus
key enter
wait 3
dump
text clear
key enter
wait 2
click-id field@new-path
text handmade
key enter
wait 3
dump
click-id tab@Listing
wait 2
click-id pin
wait 2
dump
key escape
key s ctrl
wait 3
dump
shot {shots}/saved.png
quit
",
    }
}

pub fn delete() -> Scenario {
    Scenario {
        setup: mapped,
        hook: no_hook,
        after: &[&["paths"]],
        script: "SETTLE
click-id del/6
wait 3
dump
shot {shots}/step-deleted.png
click-id del/0
wait 3
dump
click-id doc-delete
wait 3
dump
shot {shots}/path-deleted.png
click-id paths/1
wait 3
dump
key s ctrl
wait 3
dump
quit
",
    }
}

pub fn diff() -> Scenario {
    Scenario {
        setup: with_parent,
        hook: no_hook,
        after: &[],
        script: "SETTLE
click-id tab@Diff
wait 3
dump
shot {shots}/diff.png
click-id diffrow/2
wait 3
dump
click-id tab@Diff
wait 2
click-id diffrow/0
wait 3
dump
shot {shots}/changed-path.png
click-id tab@Diff
wait 2
click-id diffrow/4
wait 3
dump
click-id diff-refresh
idle
wait 2
dump
quit
",
    }
}

pub fn reload() -> Scenario {
    Scenario {
        setup: mapped,
        hook: reload_hook,
        after: &[],
        script: "SETTLE
dump
rect edit-source
pause 2500
idle
wait 3
dump
shot {shots}/source-edited.png
rect edit-map
pause 2500
idle
wait 3
dump
shot {shots}/map-reloaded.png
quit
",
    }
}

pub fn dock() -> Scenario {
    Scenario {
        setup: mapped,
        hook: no_hook,
        after: &[],
        script: "SETTLE
dump
hover-id split@nav
wait 1
down
wait 1
mouse 600 500
wait 1
up
wait 2
dump
shot {shots}/resized.png
hover-id grip@xrefs
wait 1
down
wait 1
mouse 300 400
wait 2
shot {shots}/moving.png
up
wait 2
dump
hover-id grip@output
wait 1
down
wait 1
mouse 1700 300
wait 2
up
wait 2
dump
shot {shots}/moved.png
hover-id grip@nav
wait 1
down
wait 1
up
wait 2
dump
hover-id grip@nav
wait 1
down
wait 1
mouse 1200 10
wait 2
up
wait 2
dump
quit
",
    }
}

/// Plays scenario `name` with `bin`: its stderr, and the transcript of its `after` commands.
/// The screenshots land in `shots(name)`.
pub fn play(bin: &Path, name: &str, s: &Scenario) -> Result<(String, String), Missing> {
    let script = s.script.replace("SETTLE\n", SETTLE);
    let mut lines: Vec<String> = script.lines().map(str::to_owned).collect();
    for k in 0..lines.len() {
        while let Some(open) = lines[k].find("<<") {
            let close = open + lines[k][open..].find(">>").expect("unclosed <<");
            let (prefix, offset) = match lines[k][open + 2..close].split_once('|') {
                Some((p, o)) => (p.to_owned(), Some(o.to_owned())),
                None => (lines[k][open + 2..close].to_owned(), None),
            };
            let probe = lines[..k].join("\n") + "\nquit\n";
            let root = (s.setup)(bin, name)?;
            let err = gui(bin, &root, name, &probe, &mut |l| (s.hook)(bin, &root, l));
            let (x, y, w, h) = last_rect(&err, &prefix).unwrap_or_else(|| {
                panic!(
                    "{name}: no rectangle for '{prefix}' before line {}\n{err}",
                    k + 1
                )
            });
            let (px, py) = match offset {
                Some(o) => {
                    let (dx, dy) = o.split_once(',').expect("dx,dy");
                    (
                        x + dx.trim().parse::<i32>().unwrap(),
                        y + dy.trim().parse::<i32>().unwrap(),
                    )
                }
                None => (x + w / 2, y + h / 2),
            };
            lines[k].replace_range(open..close + 2, &format!("{px} {py}"));
        }
    }
    let root = (s.setup)(bin, name)?;
    let err = gui(bin, &root, name, &(lines.join("\n") + "\n"), &mut |l| {
        (s.hook)(bin, &root, l)
    });
    let mut after = Transcript {
        bin,
        root,
        out: String::new(),
    };
    for args in s.after {
        after.run(args);
    }
    Ok((err, after.out))
}

/// The last rectangle printed on a line starting with `prefix`: (x, y, w, h).
fn last_rect(stderr: &str, prefix: &str) -> Option<(i32, i32, i32, i32)> {
    let l = stderr.lines().rfind(|l| l.starts_with(prefix))?;
    let r = l.split("Rect { ").nth(1)?;
    let num = |k: &str| -> Option<i32> {
        r.split(&format!("{k}: "))
            .nth(1)?
            .split([',', ' ', '}'])
            .next()?
            .parse()
            .ok()
    };
    Some((num("x")?, num("y")?, num("w")?, num("h")?))
}

pub fn shots(name: &str) -> PathBuf {
    scratch(name).join("shots")
}
