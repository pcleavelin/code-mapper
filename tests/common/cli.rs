use super::{Missing, Transcript, edit_file, fixture, git_commit, jj, jj_commit, needs};
use std::path::Path;

#[macro_export]
macro_rules! cli_scenarios {
    ($m:ident) => {
        $m! { cli: read, edit, stale, vcs, git, merge }
    };
}

pub fn read(bin: &Path, name: &str) -> Result<String, Missing> {
    let mut t = Transcript {
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
        &["grep", r"store\.\w+\("],
        &["grep", "("],
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
        &["paths"],
        &["paths", "startup"],
        &["paths", "nope"],
        &["path", "startup"],
        &["path", "shapes"],
        &["path", "stats"],
        &["path", "c-lib"],
        &["uncovered"],
        &["uncovered", "shapes"],
        &["coverage"],
        &["stale"],
        &["check"],
        &["promote", "area"],
        &["path-add", "startup", "nope"],
        &["bogus-command"],
        &["tree"],
        &["roots", "many"],
    ] {
        t.run(args);
    }
    Ok(t.out)
}

pub fn edit(bin: &Path, name: &str) -> Result<String, Missing> {
    let mut t = Transcript {
        bin,
        root: fixture(name, bin, true),
        out: String::new(),
    };
    for args in [
        &["path-new", "scratch", "flow"][..],
        &["path-new", "scratch", "layer"],
        &["path-new", "bad", "kinda"],
        &["paths", "scratch"],
        &["path-add", "scratch", "src/main.rs:main", "-1"],
        &["path-add", "scratch", "fill"],
        &["path-add", "scratch", "describe", "0"],
        &["path-add", "scratch", "src/main.rs", "1", "3"],
        &["path-add", "scratch", "src/store.rs", "13", "14", "9"],
        &["path-add", "scratch", "src/store.rs", "30", "99"],
        &["path-add", "scratch", "src/store.rs", "13", "14", "1"],
        &["path-note", "scratch", "A note."],
        &["step-note", "scratch", "1", "fills"],
        &["step-note", "scratch", "9", "x"],
        &["note-edit", "scratch", "1", "fills", "fills it"],
        &["note-edit", "scratch", "-1", "A note", "The note"],
        &["note-edit", "scratch", "1", "zzz", "y"],
        &["paths", "scratch"],
        &["path-move", "scratch", "2", "1"],
        &["path-move", "scratch", "0", "1"],
        &["path-move", "scratch", "4", "-1"],
        &["path-swap", "scratch", "1", "2"],
        &["path-swap", "scratch", "1", "20"],
        &["paths", "scratch"],
        &["step-link", "scratch", "1", "startup"],
        &["step-link", "scratch", "1", "nope"],
        &["step-link", "scratch", "1", "scratch"],
        &["step-link", "scratch", "9", "startup"],
        &["step-unlink", "scratch", "2"],
        &["paths", "scratch"],
        &["path", "startup"],
        &["path", "scratch", "--expand"],
        &["path-rm", "startup"],
        &["path-rename", "startup", "boot"],
        &["paths", "scratch"],
        &["path-rename", "boot", "startup"],
        &["check"],
        &["path-group", "scratch", "flows/demo"],
        &["path-group", "startup", "flows"],
        &[
            "path-new",
            "grouped",
            "layer",
            "In a group.",
            "--group",
            " areas/ ",
        ],
        &["path-group", "nope", "x"],
        &["groups"],
        &["paths"],
        &["path", "grouped"],
        &["group-rename", "flows", "work/flows"],
        &["group-rename", "nope", "x"],
        &["groups"],
        &["path-group", "startup", ""],
        &["path-rm", "grouped"],
        &["groups"],
        &["path-pin", "scratch", "3", "src/main.rs", "7", "11"],
        &["path-pin", "scratch", "3", "src/main.rs", "0", "5"],
        &["path-pin", "scratch", "3", "src/main.rs", "13", "50"],
        &["path-rename", "scratch", "scratch2"],
        &["path-rename", "scratch2", "startup"],
        &["path", "scratch2"],
        &["path-rm", "scratch2", "1"],
        &["path-rm", "scratch2", "9"],
        &["paths", "scratch2"],
        &["promote", "src/main.rs:main", "2"],
        &["paths", "main"],
        &["promote", "src/main.rs:main", "2"],
        &["promote", "Store::add", "1", "adding"],
        &["paths", "adding"],
        &["path-rm", "scratch2"],
        &["path-rm", "nope"],
        &["paths"],
        &["coverage"],
    ] {
        t.run(args);
    }
    Ok(t.out)
}

pub fn stale(bin: &Path, name: &str) -> Result<String, Missing> {
    let root = fixture(name, bin, true);
    let mut t = Transcript {
        bin,
        root: root.clone(),
        out: String::new(),
    };
    t.run(&["check"]);
    t.note("two lines above impl Store: its steps move but stay current");
    edit_file(
        &root,
        "src/store.rs",
        "pub struct Store {",
        "// a comment\n// another\npub struct Store {",
    );
    t.run(&["stale"]);
    t.run(&["paths", "startup"]);
    t.note("check's body changes: its step goes stale");
    edit_file(&root, "src/store.rs", "< 1000", "< 2000");
    t.run(&["stale"]);
    t.run(&["check"]);
    t.run(&["path", "startup"]);
    t.run(&["uncovered", "check"]);
    t.note("a line inside the slice changes, and the absolute-lines step's lines move");
    edit_file(
        &root,
        "src/store.rs",
        "let mut sum = 0.0;",
        "let mut sum: f64 = 0.0;",
    );
    t.run(&["stale"]);
    t.note("fill is renamed: the symbol is gone");
    edit_file(&root, "src/main.rs", "fn fill(", "fn fill_up(");
    edit_file(
        &root,
        "src/main.rs",
        "    fill(&mut store);",
        "    fill_up(&mut store);",
    );
    t.run(&["stale"]);
    t.note("the text of report moves down unchanged in its file: stale lists where it went");
    t.run(&["path-add", "startup", "src/main.rs", "18", "19", "-1"]);
    edit_file(
        &root,
        "src/main.rs",
        "fn report(store: &Store) {",
        "// moved\nfn report(store: &Store) {",
    );
    t.run(&["stale"]);
    t.note("helpers.py is deleted: its file is gone");
    std::fs::remove_file(root.join("tools/helpers.py")).unwrap();
    t.run(&["stale"]);
    t.run(&["paths", "stats"]);
    t.note("re-pin what can be re-pinned by hand, delete the rest");
    t.run(&["path-pin", "startup", "3", "src/store.rs", "31", "33"]);
    t.run(&["path-pin", "startup", "1", "src/main.rs", "13", "16"]);
    t.run(&["path-pin", "startup", "5", "src/store.rs", "23", "27"]);
    t.run(&["path-pin", "startup", "7", "src/main.rs", "19", "20"]);
    t.run(&["path-rm", "stats", "1"]);
    t.run(&["stale"]);
    t.run(&["check"]);
    t.run(&["path", "startup"]);
    Ok(t.out)
}

pub fn vcs(bin: &Path, name: &str) -> Result<String, Missing> {
    needs("jj")?;
    let root = fixture(name, bin, true);
    let mut t = Transcript {
        bin,
        root: root.clone(),
        out: String::new(),
    };
    t.run(&["diff"]);
    t.note("the map and the source are committed as the parent revision");
    jj_commit(&root, "base");
    t.run(&["diff"]);
    t.run(&["step-note", "startup", "0", "A new entry note."]);
    t.run(&["path-add", "startup", "describe", "-1"]);
    t.run(&["path-rm", "shapes"]);
    t.run(&["path-new", "fresh", "layer", "Added in this revision."]);
    t.run(&["path-add", "fresh", "mean", "-1"]);
    t.run(&["path-pin", "c-lib", "0", "c/lib.c", "8", "9"]);
    t.run(&["path-note", "stats", "Changed."]);
    t.run(&["step-link", "stats", "0", "startup"]);
    t.run(&["diff"]);
    edit_file(
        &root,
        "tools/stats.py",
        "def main():",
        "def extra():
    return 1


def main():",
    );
    t.run(&["path-add", "fresh", "extra", "-1"]);
    t.note("source edits after the commit: repin follows each stale step from @-");
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
    std::fs::remove_file(root.join("tools/helpers.py")).unwrap();
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
    t.run(&["stale"]);
    t.run(&["repin"]);
    t.run(&["stale"]);
    t.run(&["repin", "@--"]);
    t.run(&["repin", "nonsense-rev"]);
    t.run(&["path", "startup"]);
    let _ = jj(
        &root,
        &["log", "-r", "@", "--no-graph", "-T", "description"],
    );
    Ok(t.out)
}

pub fn git(bin: &Path, name: &str) -> Result<String, Missing> {
    needs("git")?;
    let root = fixture(name, bin, true);
    let mut t = Transcript {
        bin,
        root: root.clone(),
        out: String::new(),
    };
    t.run(&["diff"]);
    t.note("the map and the source are committed as HEAD of a git repo");
    git_commit(&root, "base");
    t.run(&["diff"]);
    t.run(&["step-note", "startup", "0", "A new entry note."]);
    t.run(&["path-add", "startup", "describe", "-1"]);
    t.run(&["path-rm", "shapes"]);
    t.run(&["step-link", "stats", "0", "startup"]);
    t.run(&["path-group", "c-lib", "native"]);
    t.run(&["diff"]);
    t.note("source edits after the commit: repin follows each stale step from HEAD");
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
    t.run(&["stale"]);
    t.run(&["repin"]);
    t.run(&["check"]);
    Ok(t.out)
}

fn committed(root: &Path) -> String {
    jj(root, &["log", "-r", "@-", "--no-graph", "-T", "change_id"])
}

pub fn merge(bin: &Path, name: &str) -> Result<String, Missing> {
    needs("jj")?;
    let root = fixture(name, bin, true);
    let mut t = Transcript {
        bin,
        root: root.clone(),
        out: String::new(),
    };
    jj_commit(&root, "base");
    let base = committed(&root);
    t.note("side one edits a note of startup and adds a step to it");
    t.run(&["step-note", "startup", "0", "Side one's entry note."]);
    t.run(&["path-add", "startup", "describe", "-1"]);
    jj(&root, &["commit", "-m", "one"]);
    let one = committed(&root);
    jj(&root, &["new", &base]);
    t.note("side two edits another note of startup, adds a step to shapes and a new path");
    t.run(&["step-note", "startup", "5", "Side two's sum note."]);
    t.run(&["path-add", "shapes", "describe", "0"]);
    t.run(&["path-new", "fresh", "layer", "From side two."]);
    jj(&root, &["commit", "-m", "two"]);
    let two = committed(&root);
    t.note("the merge of the two has every change and no conflict");
    jj(&root, &["new", &one, &two]);
    t.run(&["check"]);
    t.run(&["paths", "startup"]);
    t.run(&["paths", "shapes"]);
    t.run(&["paths", "fresh"]);
    t.note("a third side also adds a step to startup: merged with side one, the two new steps sit apart in the file and merge");
    jj(&root, &["new", &base]);
    t.run(&["path-add", "startup", "mean", "-1"]);
    jj(&root, &["commit", "-m", "three"]);
    let three = committed(&root);
    jj(&root, &["new", &one, &three]);
    t.run(&["check"]);
    t.run(&["paths"]);
    t.note("a fourth side edits the note side one edited: that line conflicts, and nothing reads the map until it is resolved");
    jj(&root, &["new", &base]);
    t.run(&["step-note", "startup", "0", "Side four's entry note."]);
    jj(&root, &["commit", "-m", "four"]);
    let four = committed(&root);
    jj(&root, &["new", &one, &four]);
    t.run(&["check"]);
    t.run(&["path-note", "startup", "Written over a conflict."]);
    Ok(t.out)
}
