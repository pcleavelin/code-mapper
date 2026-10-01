use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU32, Ordering};

use domain::{Line, Map, Note, Revision, Root, TourCount, TourName};

use crate::{Fault, MapLoadError, MapStore, MapText, Origin, ParseError};

static SCRATCH_COUNT: AtomicU32 = AtomicU32::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let count = SCRATCH_COUNT.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("io-map-{label}-{}-{count}", process::id()));
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn root(&self) -> Root {
        Root::new(&self.0)
    }

    fn map_file(&self, name: &str) -> PathBuf {
        self.0.join(".codemap").join(name)
    }

    fn put(&self, name: &str, text: &str) {
        let file = self.map_file(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        io_store::write(&file, text).unwrap();
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.0.join(".codemap"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

const FIRST: &str = "codemap 9
tour first
kind flow
author ai
group tools/cli
note A note with a line break\\nand a backslash \\\\ and a return\\r.

step 0aaaaa
order 1
parent zzzzzz
author human
file src/main.rs
symbol main
lines 0 4
hash 00000000deadbeef
link second
note Step note.

step zzzzzz
order 0
author ai
file src/lib.rs
lines -2 3
hash ffffffffffffffff
";

const SECOND: &str = "codemap 9
tour second
kind layer
author human
";

fn parse(text: &str) -> Result<Map, ParseError> {
    MapStore::base(&MapText::new(text), &Revision::new("@-"))
}

fn fault(text: &str) -> (Option<u32>, Fault) {
    let error = parse(text).unwrap_err();
    assert_eq!(error.origin, Origin::Revision(Revision::new("@-")));
    (error.line.map(Line::number), error.fault)
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

#[test]
fn every_committed_map_file_renders_to_its_own_bytes() {
    let root = repository_root();
    let mut store = MapStore::new(&Root::new(&root));
    let map = store.load().unwrap();
    assert_ne!(map.tours().len(), 0);
    for tour in map.tours() {
        let file = root
            .join(".codemap")
            .join(format!("{}.cmap", tour.name().as_str()));
        let on_disk = fs::read_to_string(&file).unwrap();
        assert_eq!(MapText::of(tour).as_str(), on_disk, "{}", file.display());
    }
}

#[test]
fn a_committed_map_saves_to_identical_bytes() {
    let root = repository_root();
    let scratch = Scratch::new("copy");
    for entry in fs::read_dir(root.join(".codemap")).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        scratch.put(&name, &fs::read_to_string(entry.path()).unwrap());
    }
    let mut store = MapStore::new(&scratch.root());
    let map = store.load().unwrap();
    let mut fresh = MapStore::new(&scratch.root());
    fresh.save(&map).unwrap();
    for name in scratch.names() {
        assert_eq!(
            fs::read(scratch.map_file(&name)).unwrap(),
            fs::read(root.join(".codemap").join(&name)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn a_tour_round_trips_with_escapes_and_steps_in_id_order() {
    let map = parse(FIRST).unwrap();
    let tour = map.tour(&TourName::new("first").unwrap()).unwrap();
    assert_eq!(
        tour.note().unwrap().as_str(),
        "A note with a line break\nand a backslash \\ and a return\r."
    );
    assert_eq!(tour.group().unwrap().as_str(), "tools/cli");
    let ids: Vec<&str> = tour.steps().iter().map(|step| step.id().as_str()).collect();
    assert_eq!(ids, ["zzzzzz", "0aaaaa"]);
    assert_eq!(MapText::of(tour).as_str(), FIRST);
}

#[test]
fn the_base_holds_concatenated_files_sorted_by_name() {
    let map = parse(&format!("{SECOND}{FIRST}")).unwrap();
    let names: Vec<&str> = map
        .tours()
        .iter()
        .map(|tour| tour.name().as_str())
        .collect();
    assert_eq!(names, ["first", "second"]);
}

#[test]
fn every_malformed_line_names_its_line_and_reason() {
    assert_eq!(
        fault("codemap 9\ntour a\n<<<<<<< conflict\n"),
        (Some(3), Fault::Conflict)
    );
    assert!(
        matches!(fault("codemap 7\n"), (Some(1), Fault::Version(line)) if line.as_str() == "codemap 7")
    );
    assert_eq!(fault("tour a\n"), (Some(1), Fault::NoVersion));
    assert!(
        matches!(fault("codemap 9\nauthor robot\n"), (Some(2), Fault::UnknownAuthor(value)) if value.as_str() == "robot")
    );
    assert!(
        matches!(fault("codemap 9\nkind tree\n"), (Some(2), Fault::UnknownKind(value)) if value.as_str() == "tree")
    );
    assert!(
        matches!(fault("codemap 9\ncolour red\n"), (Some(2), Fault::UnknownTourField(key)) if key.as_str() == "colour")
    );
    assert!(
        matches!(fault("codemap 9\ntour a\n\nstep aaaaaa\ncolour red\n"), (Some(5), Fault::UnknownStepField(key)) if key.as_str() == "colour")
    );
    assert!(
        matches!(fault("codemap 9\ntour a\n\nstep aaaaaa\n\nstep aaaaaa\n"), (Some(6), Fault::SecondStep(value)) if value.as_str() == "aaaaaa")
    );
    assert!(
        matches!(fault("codemap 9\ntour .a\n"), (Some(2), Fault::InvalidName(name)) if name.as_str() == ".a")
    );
    assert_eq!(fault("codemap 9\nkind flow\n"), (None, Fault::NoTourLine));
}

#[test]
fn step_values_parse_strictly() {
    let step = |field: &str| {
        format!(
            "codemap 9\ntour a\n\nstep aaaaaa\norder 0\nfile f.rs\nlines 0 1\nhash 0123456789abcdef\n{field}\n"
        )
    };
    assert!(parse(&step("note fine")).is_ok());
    for lines in [
        "lines 1",
        "lines 1 2 3",
        "lines a 1",
        "lines 1  2",
        "lines ",
    ] {
        assert_eq!(fault(&step(lines)), (Some(9), Fault::Lines), "{lines}");
    }
    for hash in [
        "hash abc",
        "hash 0123456789abcdef0",
        "hash 0123456789abcdeg",
        "hash +123456789abcdef",
    ] {
        assert_eq!(fault(&step(hash)), (Some(9), Fault::Hash), "{hash}");
    }
    for order in ["order x", "order -1", "order "] {
        assert_eq!(fault(&step(order)), (Some(9), Fault::Order), "{order}");
    }
    assert!(
        matches!(fault("codemap 9\ntour a\n\nstep ABC\n"), (Some(4), Fault::InvalidStepId(value)) if value.as_str() == "ABC")
    );
    assert!(
        matches!(fault("codemap 9\ntour a\n\nstep aaaaaa\norder 0\nfile f.rs\nlines 0 1\n"), (Some(4), Fault::MissingField(key)) if key.as_str() == "hash")
    );
    assert!(
        matches!(fault(&step("link .bad")), (Some(9), Fault::InvalidName(name)) if name.as_str() == ".bad")
    );
}

#[test]
fn a_parent_that_is_not_a_step_names_the_parent_line() {
    let text = "codemap 9\ntour a\n\nstep aaaaaa\norder 0\nparent bbbbbb\nfile f.rs\nlines 0 1\nhash 0123456789abcdef\n";
    match fault(text) {
        (Some(6), Fault::UnknownParent { tour, step, parent }) => {
            assert_eq!(tour.as_str(), "a");
            assert_eq!(step.as_str(), "aaaaaa");
            assert_eq!(parent.as_str(), "bbbbbb");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_missing_directory_is_an_empty_map() {
    let scratch = Scratch::new("missing");
    let mut store = MapStore::new(&scratch.root());
    assert_eq!(store.load().unwrap().tours().len(), 0);
    assert!(store.stamp().is_none());
}

#[test]
fn a_file_must_hold_one_tour_named_as_its_stem() {
    let scratch = Scratch::new("files");
    scratch.put("first.cmap", &format!("{FIRST}{SECOND}"));
    let mut store = MapStore::new(&scratch.root());
    assert!(
        matches!(store.load(), Err(MapLoadError::OneTourPerFile(file)) if file == scratch.map_file("first.cmap"))
    );
    scratch.put("first.cmap", SECOND);
    match store.load() {
        Err(MapLoadError::Misplaced { file, tour }) => {
            assert_eq!(file, scratch.map_file("first.cmap"));
            assert_eq!(tour.as_str(), "second");
        }
        other => panic!("{other:?}"),
    }
    scratch.put("first.cmap", "codemap 9\ntour first\n=======\n");
    match store.load() {
        Err(MapLoadError::Parse(error)) => {
            assert_eq!(error.origin, Origin::File(scratch.map_file("first.cmap")));
            assert_eq!(error.line.map(Line::number), Some(3));
            assert_eq!(error.fault, Fault::Conflict);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_old_single_file_map_is_refused() {
    let scratch = Scratch::new("single");
    io_store::write(&scratch.0.join(".codemap"), "codemap 7\n").unwrap();
    let mut store = MapStore::new(&scratch.root());
    assert!(
        matches!(store.load(), Err(MapLoadError::OldFormat(file)) if file == scratch.0.join(".codemap"))
    );
}

#[test]
fn save_writes_only_changed_tours_and_removes_gone_ones() {
    let scratch = Scratch::new("save");
    scratch.put("first.cmap", FIRST);
    scratch.put("second.cmap", SECOND);
    scratch.put("notes.txt", "kept");
    let mut store = MapStore::new(&scratch.root());
    let mut map = store.load().unwrap();
    assert_eq!(store.stamp().unwrap().files, TourCount::new(2));
    io_store::remove(&scratch.map_file("first.cmap")).unwrap();
    let second = TourName::new("second").unwrap();
    assert!(map.set_tour_note(&second, Note::new("Changed.")).is_ok());
    store.save(&map).unwrap();
    assert_eq!(scratch.names(), ["notes.txt", "second.cmap"]);
    assert_eq!(
        fs::read_to_string(scratch.map_file("second.cmap")).unwrap(),
        format!("{SECOND}note Changed.\n")
    );
    map.remove_tour(&TourName::new("first").unwrap()).unwrap();
    map.remove_tour(&second).unwrap();
    store.save(&map).unwrap();
    assert_eq!(scratch.names(), ["notes.txt"]);
}

#[test]
fn saves_of_two_stores_keep_each_others_tours() {
    let scratch = Scratch::new("two");
    scratch.put("first.cmap", FIRST);
    scratch.put("second.cmap", SECOND);
    let mut one = MapStore::new(&scratch.root());
    let mut other = MapStore::new(&scratch.root());
    let mut one_map = one.load().unwrap();
    let mut other_map = other.load().unwrap();
    let first = TourName::new("first").unwrap();
    let second = TourName::new("second").unwrap();
    assert!(
        one_map
            .set_tour_note(&first, Note::new("From one."))
            .is_ok()
    );
    assert!(
        other_map
            .set_tour_note(&second, Note::new("From other."))
            .is_ok()
    );
    one.save(&one_map).unwrap();
    other.save(&other_map).unwrap();
    let merged = MapStore::new(&scratch.root()).load().unwrap();
    assert_eq!(
        merged.tour(&first).unwrap().note().unwrap().as_str(),
        "From one."
    );
    assert_eq!(
        merged.tour(&second).unwrap().note().unwrap().as_str(),
        "From other."
    );
}

#[test]
fn a_first_save_creates_the_directory() {
    let scratch = Scratch::new("create");
    let map = parse(FIRST).unwrap();
    let mut store = MapStore::new(&scratch.root());
    store.save(&map).unwrap();
    assert_eq!(
        fs::read_to_string(scratch.map_file("first.cmap")).unwrap(),
        FIRST
    );
    assert_eq!(store.stamp().unwrap().files, TourCount::new(1));
}
