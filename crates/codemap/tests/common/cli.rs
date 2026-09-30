use super::{Missing, Transcript, edit_file, fixture, git_commit, jj, jj_commit, needs};
use std::fs;
use std::path::Path;

#[macro_export]
macro_rules! cli_scenarios {
    ($m:ident) => {
        $m! { cli: read, edit, stale, vcs, git, merge }
    };
}

pub(crate) fn read(bin: &Path, name: &str) -> String {
    let mut transcript = Transcript {
        bin,
        root: fixture(name, bin, true),
        out: String::new(),
    };
    for args in [
        &["help"][..],
        &["files"],
        &["files", "tools"],
        &["symbols"],
        &["symbols", "store"],
        &["show", "src/main.rs", "7", "11"],
        &["show", "main.rs", "23"],
        &["show", "nope.rs"],
        &["search", r"store\.\w+\("],
        &["search", "("],
        &["callers", "check"],
        &["callees", "main"],
        &["callees", "area"],
        &["callers", "Store::len"],
        &["callers", "store:len"],
        &["callers", "shapes:Circle::area"],
        &["callers", "nothing_here"],
        &["refs", "total_area"],
        &["tree", "main"],
        &["tree", "main", "1"],
        &["tree", "main.py"],
        &["roots"],
        &["roots", "2"],
        &["notes", "store"],
        &["notes", "^Entry"],
        &["notes", "["],
        &["tours"],
        &["tours", "startup"],
        &["tours", "nope"],
        &["tour", "startup"],
        &["tour", "shapes"],
        &["tour", "stats"],
        &["tour", "c-lib"],
        &["uncovered"],
        &["uncovered", "shapes"],
        &["coverage"],
        &["index", "shapes"],
        &["index", "nothing_here"],
        &["index"],
        &["stale"],
        &["check"],
        &["promote", "area"],
        &["tour-add", "startup", "nope"],
        &["bogus-command"],
        &["tree"],
        &["roots", "many"],
    ] {
        transcript.run(args);
    }
    transcript.out
}

pub(crate) fn edit(bin: &Path, name: &str) -> String {
    let mut transcript = Transcript {
        bin,
        root: fixture(name, bin, true),
        out: String::new(),
    };
    for args in [
        &["tour-new", "scratch", "flow"][..],
        &["tour-new", "scratch", "layer"],
        &["tour-new", "bad", "kinda"],
        &["tours", "scratch"],
        &["tour-add", "scratch", "src/main.rs:main", "-1"],
        &["tour-add", "scratch", "fill"],
        &["tour-add", "scratch", "describe", "0"],
        &["tour-add", "scratch", "src/main.rs", "1", "3"],
        &["tour-add", "scratch", "src/store.rs", "13", "14", "9"],
        &["tour-add", "scratch", "src/store.rs", "30", "99"],
        &["tour-add", "scratch", "src/store.rs", "13", "14", "1"],
        &["tour-note", "scratch", "A note."],
        &["step-note", "scratch", "1", "fills"],
        &["step-note", "scratch", "9", "x"],
        &["note-edit", "scratch", "1", "fills", "fills it"],
        &["note-edit", "scratch", "-1", "A note", "The note"],
        &["note-edit", "scratch", "1", "zzz", "y"],
        &["tours", "scratch"],
        &["tour-move", "scratch", "2", "1"],
        &["tour-move", "scratch", "0", "1"],
        &["tour-move", "scratch", "4", "-1"],
        &["tour-swap", "scratch", "1", "2"],
        &["tour-swap", "scratch", "1", "20"],
        &["tours", "scratch"],
        &["step-link", "scratch", "1", "startup"],
        &["step-link", "scratch", "1", "nope"],
        &["step-link", "scratch", "1", ""],
        &["step-link", "scratch", "1", "scratch"],
        &["step-link", "scratch", "9", "startup"],
        &["step-unlink", "scratch", "2"],
        &["tours", "scratch"],
        &["tour", "startup"],
        &["tour", "scratch", "--inline"],
        &["tour-rm", "startup"],
        &["tour-rename", "startup", "boot"],
        &["tours", "scratch"],
        &["tour-rename", "boot", "startup"],
        &["check"],
        &["tour-group", "scratch", "flows/demo"],
        &["tour-group", "startup", "flows"],
        &[
            "tour-new",
            "grouped",
            "layer",
            "In a group.",
            "--group",
            " areas/ ",
        ],
        &["tour-group", "nope", "x"],
        &["groups"],
        &["tours"],
        &["tour", "grouped"],
        &["group-rename", "flows", "work/flows"],
        &["group-rename", "nope", "x"],
        &["groups"],
        &["tour-group", "startup", ""],
        &["tour-rm", "grouped"],
        &["groups"],
        &["tour-pin", "scratch", "3", "src/main.rs", "7", "11"],
        &["tour-pin", "scratch", "3", "src/main.rs", "0", "5"],
        &["tour-pin", "scratch", "3", "src/main.rs", "13", "50"],
        &["tour-rename", "scratch", "scratch2"],
        &["tour-rename", "scratch2", "startup"],
        &["tour", "scratch2"],
        &["tour-rm", "scratch2", "1"],
        &["tour-rm", "scratch2", "9"],
        &["tours", "scratch2"],
        &["promote", "src/main.rs:main", "2"],
        &["tours", "main"],
        &["promote", "src/main.rs:main", "2"],
        &["promote", "Store::add", "1", "adding"],
        &["tours", "adding"],
        &["promote", "src/main.rs:main", "2", "whole", "--all"],
        &["tours", "whole"],
        &["tour-rm", "whole"],
        &["tour-rm", "scratch2"],
        &["tour-rm", "nope"],
        &["tours"],
        &["coverage"],
    ] {
        transcript.run(args);
    }
    transcript.out
}

pub(crate) fn stale(bin: &Path, name: &str) -> String {
    let root = fixture(name, bin, true);
    let mut transcript = Transcript {
        bin,
        root: root.clone(),
        out: String::new(),
    };
    transcript.run(&["check"]);
    transcript.note("two lines above impl Store: its steps move but stay current");
    edit_file(
        &root,
        "src/store.rs",
        "pub struct Store {",
        "// a comment\n// another\npub struct Store {",
    );
    transcript.run(&["stale"]);
    transcript.run(&["tours", "startup"]);
    transcript.note("check's body changes: its step goes stale");
    edit_file(&root, "src/store.rs", "< 1000", "< 2000");
    transcript.run(&["stale"]);
    transcript.run(&["check"]);
    transcript.run(&["tour", "startup"]);
    transcript.run(&["uncovered", "check"]);
    transcript.note("a line inside the slice changes, and the absolute-lines step's lines move");
    edit_file(
        &root,
        "src/store.rs",
        "let mut sum = 0.0;",
        "let mut sum: f64 = 0.0;",
    );
    transcript.run(&["stale"]);
    transcript.note("fill is renamed: the symbol is gone");
    edit_file(&root, "src/main.rs", "fn fill(", "fn fill_up(");
    edit_file(
        &root,
        "src/main.rs",
        "    fill(&mut store);",
        "    fill_up(&mut store);",
    );
    transcript.run(&["stale"]);
    transcript
        .note("the text of report moves down unchanged in its file: stale lists where it went");
    transcript.run(&["tour-add", "startup", "src/main.rs", "18", "19", "-1"]);
    edit_file(
        &root,
        "src/main.rs",
        "fn report(store: &Store) {",
        "// moved\nfn report(store: &Store) {",
    );
    transcript.run(&["stale"]);
    transcript.note("helpers.py is deleted: its file is gone");
    fs::remove_file(root.join("tools/helpers.py")).unwrap();
    transcript.run(&["stale"]);
    transcript.run(&["tours", "stats"]);
    transcript.note("re-pin what can be re-pinned by hand, delete the rest");
    transcript.run(&["tour-pin", "startup", "3", "src/store.rs", "31", "33"]);
    transcript.run(&["tour-pin", "startup", "1", "src/main.rs", "13", "16"]);
    transcript.run(&["tour-pin", "startup", "5", "src/store.rs", "23", "27"]);
    transcript.run(&["tour-pin", "startup", "7", "src/main.rs", "19", "20"]);
    transcript.run(&["tour-rm", "stats", "1"]);
    transcript.run(&["stale"]);
    transcript.run(&["check"]);
    transcript.run(&["tour", "startup"]);
    transcript.out
}

pub(crate) fn vcs(bin: &Path, name: &str) -> Result<String, Missing> {
    needs("jj")?;
    let root = fixture(name, bin, true);
    let mut transcript = Transcript {
        bin,
        root: root.clone(),
        out: String::new(),
    };
    transcript.run(&["diff"]);
    transcript.note("the map and the source are committed as the parent revision");
    jj_commit(&root, "base");
    transcript.run(&["diff"]);
    transcript.run(&["step-note", "startup", "0", "A new entry note."]);
    transcript.run(&["tour-add", "startup", "describe", "-1"]);
    transcript.run(&["tour-rm", "shapes"]);
    transcript.run(&["tour-new", "fresh", "layer", "Added in this revision."]);
    transcript.run(&["tour-add", "fresh", "mean", "-1"]);
    transcript.run(&["tour-pin", "c-lib", "0", "c/lib.c", "8", "9"]);
    transcript.run(&["tour-note", "stats", "Changed."]);
    transcript.run(&["step-link", "stats", "0", "startup"]);
    transcript.run(&["diff"]);
    edit_file(
        &root,
        "tools/stats.py",
        "def main():",
        "def extra():
    return 1


def main():",
    );
    transcript.run(&["tour-add", "fresh", "extra", "-1"]);
    transcript.note("source edits after the commit: repin follows each stale step from @-");
    edit_file(
        &root,
        "src/store.rs",
        "    fn check(&self) {",
        "    fn unrelated(&self) {}\n\n    fn check(&self) {",
    );
    edit_file(&root, "src/store.rs", "< 1000", "< 2000");
    edit_file(
        &root,
        "src/store.rs",
        "        let mut sum = 0.0;\n",
        "        let mut sum = 0.0;\n        let extra = 0.0;\n",
    );
    edit_file(
        &root,
        "src/main.rs",
        "    log_line(\"done\");",
        "    log_line(\"done\");\n    log_line(\"really\");",
    );
    edit_file(
        &root,
        "src/main.rs",
        "fn fill(store: &mut Store) {\n    store.add(Box::new(Circle { r: 1.0 }));\n    store.add(Box::new(Square { side: 2.0 }));\n}",
        "fn fill(store: &mut Store) {\n    let _ = store;\n}",
    );
    fs::remove_file(root.join("tools/helpers.py")).unwrap();
    edit_file(
        &root,
        "src/main.rs",
        "fn log_line(msg: &str) {
	eprintln!(\"{msg}\");
}
",
        "",
    );
    edit_file(
        &root,
        "src/shapes.rs",
        "/// Nothing calls this.",
        "fn log_line(msg: &str) {
	eprintln!(\"{msg}\");
}

/// Nothing calls this.",
    );
    edit_file(
        &root,
        "src/shapes.rs",
        "pub fn describe(s: &dyn Shape) -> String {
    format!(",
        "pub fn describe_shape(s: &dyn Shape) -> String {
    format!(\"shape: \" + ",
    );
    edit_file(
        &root,
        "tools/stats.py",
        "def extra():",
        "def extra_renamed():",
    );
    transcript.run(&["stale"]);
    transcript.run(&["repin"]);
    transcript.run(&["stale"]);
    transcript.run(&["repin", "@--"]);
    transcript.run(&["repin", "nonsense-rev"]);
    transcript.run(&["tour", "startup"]);
    let _ = jj(
        &root,
        &["log", "-r", "@", "--no-graph", "-T", "description"],
    );
    Ok(transcript.out)
}

pub(crate) fn git(bin: &Path, name: &str) -> Result<String, Missing> {
    needs("git")?;
    let root = fixture(name, bin, true);
    let mut transcript = Transcript {
        bin,
        root: root.clone(),
        out: String::new(),
    };
    transcript.run(&["diff"]);
    transcript.note("the map and the source are committed as HEAD of a git repo");
    git_commit(&root, "base");
    transcript.run(&["diff"]);
    transcript.run(&["step-note", "startup", "0", "A new entry note."]);
    transcript.run(&["tour-add", "startup", "describe", "-1"]);
    transcript.run(&["tour-rm", "shapes"]);
    transcript.run(&["step-link", "stats", "0", "startup"]);
    transcript.run(&["tour-group", "c-lib", "native"]);
    transcript.run(&["diff"]);
    transcript.note("source edits after the commit: repin follows each stale step from HEAD");
    edit_file(
        &root,
        "src/store.rs",
        "    fn check(&self) {",
        "    fn unrelated(&self) {}\n\n    fn check(&self) {",
    );
    edit_file(
        &root,
        "src/store.rs",
        "let mut sum = 0.0;",
        "let mut sum = 0.0_f64;",
    );
    transcript.run(&["stale"]);
    transcript.run(&["repin"]);
    transcript.run(&["check"]);
    Ok(transcript.out)
}

fn committed(root: &Path) -> String {
    jj(root, &["log", "-r", "@-", "--no-graph", "-T", "change_id"])
}

pub(crate) fn merge(bin: &Path, name: &str) -> Result<String, Missing> {
    needs("jj")?;
    let root = fixture(name, bin, true);
    let mut transcript = Transcript {
        bin,
        root: root.clone(),
        out: String::new(),
    };
    jj_commit(&root, "base");
    let base = committed(&root);
    transcript.note("side one edits a note of startup and adds a step to it");
    transcript.run(&["step-note", "startup", "0", "Side one's entry note."]);
    transcript.run(&["tour-add", "startup", "describe", "-1"]);
    jj(&root, &["commit", "-m", "one"]);
    let one = committed(&root);
    jj(&root, &["new", &base]);
    transcript.note("side two edits another note of startup, adds a step to shapes and a new tour");
    transcript.run(&["step-note", "startup", "5", "Side two's sum note."]);
    transcript.run(&["tour-add", "shapes", "describe", "0"]);
    transcript.run(&["tour-new", "fresh", "layer", "From side two."]);
    jj(&root, &["commit", "-m", "two"]);
    let two = committed(&root);
    transcript.note("the merge of the two has every change and no conflict");
    jj(&root, &["new", &one, &two]);
    transcript.run(&["check"]);
    transcript.run(&["tours", "startup"]);
    transcript.run(&["tours", "shapes"]);
    transcript.run(&["tours", "fresh"]);
    transcript.note("a third side also adds a step to startup: merged with side one, the two new steps sit apart in the file and merge");
    jj(&root, &["new", &base]);
    transcript.run(&["tour-add", "startup", "mean", "-1"]);
    jj(&root, &["commit", "-m", "three"]);
    let three = committed(&root);
    jj(&root, &["new", &one, &three]);
    transcript.run(&["check"]);
    transcript.run(&["tours"]);
    transcript.note("a fourth side edits the note side one edited: that line conflicts, and nothing reads the map until it is resolved");
    jj(&root, &["new", &base]);
    transcript.run(&["step-note", "startup", "0", "Side four's entry note."]);
    jj(&root, &["commit", "-m", "four"]);
    let four = committed(&root);
    jj(&root, &["new", &one, &four]);
    transcript.run(&["check"]);
    transcript.run(&["tour-note", "startup", "Written over a conflict."]);
    Ok(transcript.out)
}
