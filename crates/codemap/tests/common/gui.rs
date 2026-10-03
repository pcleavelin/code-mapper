use super::{
    FirstScreen, Missing, Transcript, codemap, edit_file, fixture, gui, jj_commit, needs, scratch,
};
use std::path::{Path, PathBuf};

#[macro_export]
macro_rules! gui_scenarios {
    ($m:ident) => {
        $m! { gui: document, peek, source, graph, panels, delete, diff, reload, layout, links, authoring, welcome, edit }
    };
}

pub(crate) struct Scenario {
    pub setup: fn(&Path, &str) -> Result<PathBuf, Missing>,
    pub script: &'static str,
    pub hook: fn(&Path, &Path, &str),
    pub after: &'static [&'static [&'static str]],
    pub first: FirstScreen,
}

fn mapped(bin: &Path, name: &str) -> PathBuf {
    fixture(name, bin, true)
}

fn with_parent(bin: &Path, name: &str) -> Result<PathBuf, Missing> {
    needs("jj")?;
    let root = fixture(name, bin, true);
    jj_commit(&root, "base");
    for args in [
        &["step-note", "startup", "0", "A changed note."][..],
        &["tour-add", "startup", "describe", "-1"],
        &["tour-rm", "c-lib"],
        &["tour-new", "fresh", "layer", "New here."],
        &["tour-add", "fresh", "mean", "-1"],
    ] {
        codemap(bin, &root, args);
    }
    Ok(root)
}

fn no_hook(_: &Path, _: &Path, _: &str) {}

fn reload_hook(bin: &Path, root: &Path, line: &str) {
    if line.starts_with("DUMP rect edit-source") {
        edit_file(root, "src/store.rs", "< 1000", "< 2000");
    }
    if line.starts_with("DUMP rect edit-map") {
        codemap(
            bin,
            root,
            &["tour-note", "shapes", "Edited while the window was open."],
        );
    }
}

const SETTLE: &str = "idle\nclick-id save\nclick-id tours/2\nwait 2\n";

pub(crate) fn document() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
        hook: no_hook,
        after: &[],
        script: "SETTLE
dump
shot {shots}/open.png
scroll document 2000
wait 2
click-id step/5
wait 2
dump
click-id whole/5
wait 2
shot {shots}/whole.png
click-id ctx-b/5
wait 2
click-id ctx-a/5
wait 2
dump
shot {shots}/context.png
click-id ctx0/5
wait 2
dump
scroll document 0
wait 2
click-id hide/1
wait 2
click-id collapse/1
wait 2
dump
shot {shots}/collapse.png
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
click-id doc-hide-code
wait 2
shot {shots}/code-hidden.png
click-id doc-show-code
wait 2
click-id doc-collapse
wait 2
dump
shot {shots}/collapsed-all.png
click-id doc-expand
wait 2
hover-id document
wheel -300
wait 3
dump
shot {shots}/scrolled.png
click-id steps/6
wait 3
dump
shot {shots}/steps.png
click-id doc-graph
wait 3
dump
key p ctrl
wait 2
text >collapse all
wait 2
dump
shot {shots}/palette-actions.png
key enter
wait 3
dump
key p ctrl
wait 2
text total_a
wait 2
dump
shot {shots}/palette-symbol.png
key enter
wait 3
dump
key p ctrl
wait 2
key escape
wait 2
dump
quit
",
    }
}

pub(crate) fn peek() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
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
rect doccode/4
mouse <<DUMP rect doccode/4|244,24>>
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

pub(crate) fn source() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
        hook: no_hook,
        after: &[&["tours", "startup"]],
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
click-id add-lines
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

pub(crate) fn graph() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
        hook: no_hook,
        after: &[],
        script: "SETTLE
click-id tab@Graph
wait 4
dump
dblclick <<DUMP node main|30,8>>
wait 2
dump
shot {shots}/graph.png
click <<DUMP button main 'callees>>
wait 4
dump
shot {shots}/revealed.png
click <<DUMP node new|30,8>>
wait 4
dump
shot {shots}/off-tour.png
key down
pause 300
wait 2
dump
key left
pause 300
wait 2
dump
key right
pause 300
wait 2
dump
click <<DUMP button main 'hide>>
wait 4
hover-id graph-canvas
wheel -400 shift
wait 4
mouse 5 5
wait 2
dump
click <<DUMP button new 'hide>>
wait 4
hover-id graph-canvas
wheel 400
wait 4
mouse 5 5
wait 2
dump
click <<DUMP parent 1 main>>
pause 300
wait 2
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
pinch -60
wait 4
dump
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
click <<DUMP button fill '[more-below]>>
wait 4
dump
shot {shots}/context.png
click <<DUMP button fill 'no context>>
wait 4
dump
click-id graph-turn
pause 300
wait 2
dump
shot {shots}/turned.png
click <<DUMP button fill 'source>>
wait 4
dump
quit
",
    }
}

pub(crate) fn panels() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
        hook: no_hook,
        after: &[&["tours"]],
        script: PANELS,
    }
}

const PANELS: &str = "SETTLE
click-id tab@Symbols
wait 3
click-id field@symbols
text area
wait 3
dump
shot {shots}/filtered.png
click-id sym@5:5
wait 3
dump
click-id tab@Files
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
click-id tab@Tours
wait 2
click-id field@tours
text SHAPE
wait 3
dump
shot {shots}/tours-filtered.png
key a ctrl
key backspace
wait 3
dump
text Circle
wait 3
dump
shot {shots}/tours-found-steps.png
click-id found@1:1
wait 3
dump
click-id field@tours
wait 2
key a ctrl
key backspace
wait 3
click-id tours/1
wait 3
dump
shot {shots}/shapes.png
click-id field@search
text self\\.\\w+
key enter
wait 3
dump
shot {shots}/search.png
click-id hit/2
wait 3
dump
click-id cmd
text tours startup
key enter
wait 3
shot {shots}/console.png
text tour-new fresh layer
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
click-id new-tour
wait 2
text handmade
click-id wizard-next
wait 2
click-id field@wizard-search
text add
wait 2
click-id wizard-sym/0
wait 2
dump
click-id wizard-next
wait 2
dump
click-id wizard-tick/1
wait 1
dump
click-id wizard-next
wait 2
click-id wizard-next
wait 2
click-id wizard-create
wait 3
dump
open src/store.rs 12
wait 3
click-id tab@Source
wait 2
click-id add-lines
wait 2
dump
key escape
key s ctrl
wait 3
dump
shot {shots}/saved.png
quit
";

const AUTHORING: &str = "SETTLE
click-id new-tour
wait 2
dump
click-id wizard-next
wait 2
dump
click-id field@wizard-name
text handmade
click-id wizard-kind@layer
wait 2
click-id field@wizard-group
text hand
wait 2
shot {shots}/form.png
click-id wizard-next
wait 2
click-id field@wizard-search
text main
wait 2
click-id wizard-sym/0
wait 2
click-id wizard-next
wait 2
dump
click-id wizard-tick/1
wait 1
click-id wizard-tick/2
wait 1
click-id wizard-tick/3
wait 1
dump
click-id wizard-next
wait 2
click-id wizard-next
wait 2
click-id wizard-create
wait 3
dump
shot {shots}/empty.png
click-id tab@Symbols
wait 2
click-id field@symbols
text main
wait 3
click-id sym@3:0
wait 2
click-id add-offer@Symbols
wait 2
dump
shot {shots}/symbols.png
click-id tab@Tours
wait 2
click-id steps/0
wait 3
hover-id xfrom/0
wait 1
click-id add-xfrom/0
wait 2
hover-id xfrom/1
wait 1
click-id add-xfrom/1
wait 2
dump
hover-id xfrom/1
wait 1
click-id add-xfrom/1
wait 2
dump
open src/store.rs 12
wait 3
rect lines
click <<DUMP rect lines|200,8>>
wait 2
click <<DUMP rect lines|200,40>> shift
wait 2
click-id target-top@Source
wait 2
dump
click-id add-lines
wait 2
dump
shot {shots}/source.png
click-id steps/0
wait 2
click-id tab@Graph
wait 4
dump
click <<DUMP button main 'callees>>
wait 4
dump
click-id graph-fit
wait 4
dump
shot {shots}/graph.png
click <<DUMP button new '+ step>>
wait 4
dump
click-id tab@Tour
wait 2
rect steps/3
rect steps/1
drag <<DUMP rect steps/3|20,10>> <<DUMP rect steps/1|20,10>>
wait 3
dump
shot {shots}/under.png
rect steps/3
rect steps/0
drag <<DUMP rect steps/3|20,10>> <<DUMP rect steps/0|20,1>>
wait 3
dump
rect steps/0
rect steps/3
drag <<DUMP rect steps/0|20,10>> <<DUMP rect steps/3|20,10>>
wait 3
dump
shot {shots}/moved.png
click-id promote-focus
wait 3
dump
key s ctrl
wait 3
dump
quit
";

pub(crate) fn authoring() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
        hook: no_hook,
        after: &[&["tours", "handmade"]],
        script: AUTHORING,
    }
}

pub(crate) fn delete() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
        hook: no_hook,
        after: &[&["tours"]],
        script: "SETTLE
scroll document 2000
wait 2
click-id del/6
wait 3
dump
shot {shots}/step-deleted.png
scroll document 0
wait 2
click-id del/0
wait 3
dump
click-id doc-delete
wait 3
dump
shot {shots}/tour-deleted.png
click-id tours/1
wait 3
dump
key s ctrl
wait 3
dump
quit
",
    }
}

pub(crate) fn diff() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
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
shot {shots}/changed-tour.png
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

pub(crate) fn reload() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
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

pub(crate) fn layout() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
        hook: no_hook,
        after: &[],
        script: "SETTLE
dump
hover-id divider/5
wait 1
down
wait 1
mouse 600 500
wait 1
up
wait 2
dump
shot {shots}/resized.png
hover-id doc-delete
wait 1
click-id split-right/1
wait 3
click-id field@views
text sou
wait 3
dump
shot {shots}/picker.png
key enter
wait 3
dump
click-id split-down/7
wait 3
click-id view/8
wait 3
dump
shot {shots}/split.png
hover-id doc-delete
wait 1
rect tour-buttons
dump
hover-id tab@Console
wait 1
down
wait 1
mouse 20 400
wait 2
shot {shots}/moving.png
up
wait 2
dump
shot {shots}/moved.png
hover-id tab@Tours
wait 1
down
wait 1
up
wait 2
dump
hover-id tab@Symbols
wait 1
down
wait 1
mouse 1200 10
wait 2
up
wait 2
dump
click-id close-panel/7
wait 3
dump
click-id close-tab@Diff
wait 3
dump
tab source
wait 3
dump
shot {shots}/revealed.png
quit
",
    }
}

pub(crate) fn links() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| {
            let root = mapped(bin, name);
            for args in [
                &["tour-group", "stats", "tools"][..],
                &["step-link", "startup", "5", "shapes"],
            ] {
                codemap(bin, &root, args);
            }
            Ok(root)
        },
        hook: no_hook,
        after: &[],
        script: "SETTLE
dump
absent tours/3
dump
click-id group@tools
wait 2
click-id tours/3
wait 3
dump
click-id group@tools
wait 2
absent tours/3
dump
click-id tours/2
wait 3
dump
click-id steps/5
wait 3
dump
click-id inline/5
wait 3
rect doccode/5
shot {shots}/inlined.png
click <<DUMP rect doccode/5|158,120>>
wait 3
dump
shot {shots}/nested.png
click-id from/0
wait 3
dump
click-id inline/5
wait 3
rect doccode/5
shot {shots}/not-inlined.png
click <<DUMP rect doccode/5|158,120>>
wait 3
dump
click-id link/5
wait 3
dump
quit
",
    }
}

pub(crate) fn play(
    bin: &Path,
    name: &str,
    scenario: &Scenario,
) -> Result<(String, String), Missing> {
    let script = scenario.script.replace("SETTLE\n", SETTLE);
    let mut lines: Vec<String> = script.lines().map(str::to_owned).collect();
    for line_index in 0..lines.len() {
        while let Some(open) = lines[line_index].find("<<") {
            let close = open + lines[line_index][open..].find(">>").expect("unclosed <<");
            let (prefix, offset) = match lines[line_index][open + 2..close].split_once('|') {
                Some((raw_prefix, raw_offset)) => {
                    (raw_prefix.to_owned(), Some(raw_offset.to_owned()))
                }
                None => (lines[line_index][open + 2..close].to_owned(), None),
            };
            let probe = lines[..line_index].join("\n") + "\ndump\nquit\n";
            let root = (scenario.setup)(bin, name)?;
            let err = gui(bin, &root, name, &probe, scenario.first, &mut |line| {
                (scenario.hook)(bin, &root, line);
            });
            let (left, top, width, height) = last_rect(&err, &prefix).unwrap_or_else(|| {
                panic!(
                    "{name}: no rectangle for '{prefix}' before line {}\n{err}",
                    line_index + 1
                )
            });
            let (point_x, point_y) = match offset {
                Some(spec) => {
                    let (dx, dy) = spec.split_once(',').expect("dx,dy");
                    (
                        left + dx.trim().parse::<i32>().unwrap(),
                        top + dy.trim().parse::<i32>().unwrap(),
                    )
                }
                None => (left + width / 2, top + height / 2),
            };
            lines[line_index].replace_range(open..close + 2, &format!("{point_x} {point_y}"));
        }
    }
    let root = (scenario.setup)(bin, name)?;
    let err = gui(
        bin,
        &root,
        name,
        &(lines.join("\n") + "\n"),
        scenario.first,
        &mut |line| {
            (scenario.hook)(bin, &root, line);
        },
    );
    let mut after = Transcript {
        bin,
        root,
        out: String::new(),
    };
    for args in scenario.after {
        after.run(args);
    }
    Ok((err, after.out))
}

fn last_rect(stderr: &str, prefix: &str) -> Option<(i32, i32, i32, i32)> {
    let line = stderr.lines().rfind(|line| line.starts_with(prefix))?;
    let rect_text = line.split("Rect { ").nth(1)?;
    let field = |key: &str| -> Option<i32> {
        rect_text
            .split(&format!("{key}: "))
            .nth(1)?
            .split([',', ' ', '}'])
            .next()?
            .parse()
            .ok()
    };
    Some((field("x")?, field("y")?, field("w")?, field("h")?))
}

pub(crate) fn welcome() -> Scenario {
    Scenario {
        first: FirstScreen::Welcome,
        setup: |bin, name| Ok(mapped(bin, name)),
        hook: no_hook,
        after: &[&["tours", "startup-hand"]],
        script: WELCOME,
    }
}

const WELCOME: &str = "idle
wait 2
dump
shot {shots}/start.png
hover-id start-ungrouped
key p ctrl
wait 1
text build a tour
wait 1
key enter
wait 2
hover-id field@wizard-name
click-id wizard-cancel
wait 2
absent wizard-cancel
hover-id start-ungrouped
click-id build-tour
wait 2
click-id field@wizard-name
text startup-hand
click-id wizard-kind@layer
wait 1
click-id wizard-kind@flow
wait 1
click-id field@wizard-group
text flows
click-id wizard-next
wait 2
dump
shot {shots}/wizard-start.png
click-id tab@Symbols
wait 2
click-id sym@3:0
wait 2
idle
wait 2
click-id field@wizard-search
text fill
wait 2
click-id wizard-sym/0
wait 2
dump
click-id wizard-use-focus
wait 2
dump
click-id tab@Source
wait 2
dump
shot {shots}/source-strip.png
click-id wizard-return@Source
wait 2
click-id wizard-next
wait 2
dump
click-id wizard-open/1
wait 2
click-id wizard-open/2
wait 2
dump
click-id wizard-tick/3
wait 1
click-id wizard-tick/3
wait 1
dump
shot {shots}/wizard-steps.png
click-id wizard-next
wait 2
click-id field@wizard-note
text Hand-built from main, three calls down to Store::check.
click-id wizard-next
wait 2
dump
shot {shots}/wizard-create.png
hover-id wizard-create
click-id wizard-create
wait 2
absent wizard-create
dump
click-id save
wait 2
dump
shot {shots}/workspace.png
hover-id tour-from-here
click-id tour-from-here
wait 2
hover-id wizard-tick/0
dump
click-id wizard-back
wait 2
hover-id field@wizard-search
click-id tab@Tours
wait 2
click-id tours/0
wait 2
absent wizard-back
dump
key p ctrl
wait 1
text start page
wait 1
key enter
wait 2
dump
shot {shots}/start-again.png
hover-id start-ungrouped
key left alt
wait 2
absent start-ungrouped
quit
";

pub(crate) fn edit() -> Scenario {
    Scenario {
        first: FirstScreen::Workspace,
        setup: |bin, name| Ok(mapped(bin, name)),
        hook: no_hook,
        after: &[&["tour", "startup-edited"]],
        script: EDIT,
    }
}

const EDIT: &str = "SETTLE
key p ctrl
wait 1
text startup
wait 1
key enter
wait 2
click-id doc-edit
wait 3
dump
shot {shots}/edit-open.png
click-id field@wizard-name
key u ctrl
text startup-edited
click-id wizard-kind@layer
wait 1
click-id edit-note/2
text Stores one shape and checks the size.
key escape
wait 1
hover-id edit-apply
dump
rect wizard-tick/6
click <<DUMP rect wizard-tick/6|160,8>>
wait 1
hover-id edit-note/6
click-id wizard-tick/6
wait 1
absent edit-note/6
click-id edit-more/0
wait 2
click-id wizard-open/5
wait 2
click-id wizard-tick/6
wait 1
dump
scroll wizard 400
wait 2
shot {shots}/edit-changes.png
click-id edit-apply
wait 2
absent edit-apply
dump
shot {shots}/edit-applied.png
key s ctrl
wait 3
dump
key p ctrl
wait 1
text edit tour
wait 1
key enter
wait 3
click-id wizard-kind@data
wait 1
key escape
wait 1
hover-id edit-apply
dump
shot {shots}/edit-held.png
click-id edit-cancel
wait 2
absent edit-cancel
dump
quit
";

pub(crate) fn shots(name: &str) -> PathBuf {
    scratch(name).join("shots")
}
