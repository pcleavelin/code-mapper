use std::cell::RefCell;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::rc::Rc;

use domain::{
    Backend, Call, Depth, HighlightClass, Imports, Index, Line, Location, Qualifier, Readiness,
    RelativePath, Root, Scope, SymbolId, SymbolName, SymbolQuery, TextHash,
};
use io_lsp::{Character, DocumentPosition, Outline, OutlineKind, Position, RangeEnd, Reply};

use crate::parse::{call_order, callee, record_import};
use crate::{
    Contents, FileVersion, IndexQueries, Parsers, ServerFile, ServerNotice, StartError, apply,
    build, index_files, link,
};

const LEGACY_CACHE: &[u8] = include_bytes!("tests/fixture.cache");

fn scratch(name: &str) -> PathBuf {
    let root = env::temp_dir().join(format!("index_{name}_{}", process::id()));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(&root).unwrap();
    root
}

fn put(root: &Path, path: &str, bytes: &[u8]) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(full)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

fn fixture(name: &str) -> PathBuf {
    let root = scratch(name);
    for (path, text) in FILES {
        put(&root, path, text.as_bytes());
    }
    root
}

fn linked_fixture(name: &str) -> (PathBuf, Index) {
    let root = fixture(name);
    let index = build(&Root::new(&root)).index;
    (root, index)
}

fn parsed(files: &[(&str, &str)]) -> Index {
    let mut parsers = Parsers::default();
    let mut index = Index::new(Root::new(Path::new(".")));
    for (path, text) in files {
        index.push(parsers.parse(RelativePath::new(path), &Contents::new(text)));
    }
    link(&mut index);
    index
}

fn find(index: &Index, query: &str) -> Vec<SymbolId> {
    index.find_symbols(&SymbolQuery::from(query))
}

fn describe(index: &Index, id: SymbolId) -> String {
    let file = index.file(id.file()).unwrap();
    let symbol = index.symbol(id).unwrap();
    format!(
        "{} {}:{}-{}",
        symbol.name(),
        file.path(),
        symbol.span().start().number(),
        symbol.span().end().number()
    )
}

fn names(index: &Index, ids: &[SymbolId]) -> Vec<String> {
    ids.iter().map(|id| describe(index, *id)).collect()
}

fn callee_names(index: &Index, name: &str, file: &str) -> Vec<String> {
    let id = find(index, name)
        .into_iter()
        .find(|id| index.file(id.file()).unwrap().path().as_str() == file)
        .unwrap();
    index
        .symbol(id)
        .unwrap()
        .callees()
        .iter()
        .map(|to| {
            format!(
                "{}:{}",
                index.file(to.file()).unwrap().path(),
                index.symbol(*to).unwrap().name()
            )
        })
        .collect()
}

fn listing(
    index: &Index,
    ids: &[SymbolId],
    each: impl Fn(SymbolId) -> Vec<SymbolId>,
) -> Vec<String> {
    let mut out = Vec::new();
    for id in ids {
        out.push(describe(index, *id));
        for other in each(*id) {
            out.push(format!("  {}", describe(index, other)));
        }
    }
    out
}

#[test]
fn impl_names_match_the_server() {
    let src = "pub struct S<R, RA>;\nimpl<R, RA> S<R, RA>\nwhere\n    R: Repo,\n{\n    fn new() {}\n}\nimpl<R, RA> Service<Form<Long>, (A, B, C)> for S<R, RA> {\n    fn go() {}\n}\n";
    let file = Parsers::default().parse(RelativePath::new("x.rs"), &Contents::new(src));
    let names: Vec<&str> = file
        .symbols()
        .map(|symbol| symbol.name().as_str())
        .collect();
    assert_eq!(
        names,
        [
            "S",
            "impl S<R, RA>",
            "new",
            "impl Service<Form<Long>, (A, B, C)> for S<R, RA>",
            "go"
        ]
    );
}

fn call(name: &str, qualifier: Qualifier) -> Call {
    Call {
        name: SymbolName::new(name),
        qualifier,
    }
}

fn named(scope: &str) -> Qualifier {
    Qualifier::Named(Scope::new(scope))
}

#[test]
fn parses_callees() {
    assert_eq!(callee("foo"), Some(call("foo", Qualifier::Plain)));
    assert_eq!(
        callee("self.graph.focus"),
        Some(call("focus", named("graph")))
    );
    assert_eq!(
        callee("Self::new"),
        Some(call("new", Qualifier::SelfReference))
    );
    assert_eq!(
        callee("Vec::<u8>::with_capacity"),
        Some(call("with_capacity", named("Vec")))
    );
    assert_eq!(callee("println!"), Some(call("println", Qualifier::Plain)));
    assert_eq!(callee("pkg.proc"), Some(call("proc", named("pkg"))));
    assert_eq!(callee("(a)"), None);
}

#[test]
fn records_imports() {
    let mut imports = Imports::new();
    record_import("use crate::index::{build, Index as Idx};", &mut imports);
    record_import("use std::collections::HashMap;", &mut imports);
    record_import("import \"../util\"", &mut imports);
    record_import("import fmt \"core:fmt\"", &mut imports);
    record_import("from index import build, link", &mut imports);
    record_import("import { thing } from './mod.js'", &mut imports);
    assert_eq!(imports.get("build"), Some("index"));
    assert_eq!(imports.get("Idx"), Some("index"));
    assert_eq!(imports.get("HashMap"), Some("collections"));
    assert_eq!(imports.get("util"), Some("util"));
    assert_eq!(imports.get("fmt"), Some("fmt"));
    assert_eq!(imports.get("link"), Some("index"));
    assert_eq!(imports.get("thing"), Some("mod"));
}

#[test]
fn highlights_keywords_and_strings() {
    let src = "fn a() { let s = \"hi\"; } // c\n";
    let file = Parsers::default().parse(RelativePath::new("x.rs"), &Contents::new(src));
    let line = &file.highlights()[0];
    let classes: Vec<HighlightClass> = line.iter().map(|span| span.class).collect();
    assert!(classes.contains(&HighlightClass::Keyword), "{classes:?}");
    assert!(classes.contains(&HighlightClass::String), "{classes:?}");
    assert!(classes.contains(&HighlightClass::Comment), "{classes:?}");
    assert!(
        line.windows(2).all(|pair| pair[0].end <= pair[1].start),
        "overlap: {line:?}"
    );
}

#[test]
fn calls_sort_as_their_legacy_debug_spelling() {
    let mut calls = vec![
        call("f", named("a")),
        call("f", named("a ")),
        call("f", Qualifier::SelfReference),
        call("f", Qualifier::Plain),
        call("e", named("z")),
    ];
    calls.sort_by(call_order);
    assert_eq!(
        calls,
        [
            call("e", named("z")),
            call("f", Qualifier::Plain),
            call("f", Qualifier::SelfReference),
            call("f", named("a ")),
            call("f", named("a")),
        ]
    );
}

#[test]
fn links_calls_by_name() {
    let src = "fn a() { b(); c::d(); }\nfn b() {}\nstruct C;\nimpl C { fn d() { b() } }\n";
    let index = parsed(&[("x.rs", src)]);
    let file = index.files().next().unwrap();
    let names: Vec<&str> = file
        .symbols()
        .map(|symbol| symbol.name().as_str())
        .collect();
    assert_eq!(names, ["a", "b", "C", "impl C", "d"]);
    assert_eq!(
        file.symbols()
            .nth(4)
            .unwrap()
            .owner()
            .map(domain::TypeName::as_str),
        Some("C")
    );
    let first = find(&index, "a")[0];
    let callees: Vec<&str> = index
        .symbol(first)
        .unwrap()
        .callees()
        .iter()
        .map(|id| index.symbol(*id).unwrap().name().as_str())
        .collect();
    assert_eq!(callees, ["b", "d"]);
    assert_eq!(
        index.symbol(find(&index, "b")[0]).unwrap().callers().len(),
        2
    );
    assert_eq!(index.roots(), [first]);
    assert_eq!(index.call_tree(first, Depth::new(5)).len(), 3);
}

#[test]
fn resolves_by_owner_module_and_import() {
    let source_a = "pub struct A;\nimpl A { pub fn new() -> A { A } fn go(&self) { self.step(); Self::new(); } fn step(&self) {} }\npub fn helper() {}\n";
    let source_b = "use crate::a::helper;\npub struct B;\nimpl B { pub fn new() -> B { B } fn step(&self) {} }\nfn run(x: &B) { A::new(); B::new(); x.step(); helper(); a::helper(); }\nfn helper() {}\n";
    let index = parsed(&[("src/a.rs", source_a), ("src/b.rs", source_b)]);
    assert_eq!(
        callee_names(&index, "go", "src/a.rs"),
        ["src/a.rs:new", "src/a.rs:step"]
    );
    let run = callee_names(&index, "run", "src/b.rs");
    for expected in [
        "src/a.rs:new",
        "src/b.rs:new",
        "src/b.rs:step",
        "src/b.rs:helper",
        "src/a.rs:helper",
    ] {
        assert!(run.contains(&expected.to_owned()), "{run:?}");
    }
    let b_file = index
        .file(index.find_file(&RelativePath::new("src/b.rs")).unwrap())
        .unwrap();
    assert_eq!(b_file.imports().get("helper"), Some("a"));
}

#[test]
fn odin_package_calls() {
    let main = "package main\nimport \"core\"\nimport \"util\"\nimport \"core:log\"\nS :: struct { commands: int }\nmain :: proc() {\n    core.init_bookmarks(nil)\n    util.make_static_list(int, 4)\n    helper()\n    make([]int, 4)\n    s := S{ commands = make(int) }\n    log.error(\"x\")\n}\nhelper :: proc() {}\n";
    let core =
        "package core\ninit_bookmarks :: proc(b: rawptr) {}\nerror :: proc(msg: string) {}\n";
    let util = "package util\nmake_static_list :: proc($T: typeid, n: int) {}\nmake :: proc() {}\n";
    let index = parsed(&[
        ("src/main.odin", main),
        ("src/core/bookmarks.odin", core),
        ("src/util/list.odin", util),
    ]);
    let main_id = find(&index, "main")[0];
    assert!(
        index
            .symbol(main_id)
            .unwrap()
            .calls()
            .contains(&call("init_bookmarks", named("core")))
    );
    let mut got = callee_names(&index, "main", "src/main.odin");
    got.sort();
    assert_eq!(
        got,
        [
            "src/core/bookmarks.odin:init_bookmarks",
            "src/main.odin:helper",
            "src/util/list.odin:make_static_list"
        ]
    );
}

#[test]
fn the_fixture_writes_the_legacy_cache() {
    let root = fixture("cache");
    let indexed = build(&Root::new(&root));
    assert_eq!(fs::read(root.join(".codemap-cache")).unwrap(), LEGACY_CACHE);
    let paths: Vec<&str> = indexed
        .index
        .files()
        .map(|file| file.path().as_str())
        .collect();
    assert_eq!(
        paths,
        [
            "README.md",
            "c/lib.c",
            "c/lib.h",
            "src/main.rs",
            "src/shapes.rs",
            "src/store.rs",
            "tools/helpers.py",
            "tools/stats.py"
        ]
    );
    let pending: Vec<bool> = indexed
        .index
        .files()
        .map(domain::SourceFile::is_pending)
        .collect();
    assert_eq!(pending, [false, true, true, true, true, true, true, true]);
    assert_eq!(indexed.stamps.iter().count(), 8);
    assert!(!indexed.stamps.changed(&Root::new(&root)));
    drop(fs::remove_dir_all(&root));
}

#[test]
fn a_cache_hit_keeps_the_cached_file_and_reads_the_text() {
    let root = fixture("hit");
    put(&root, ".codemap-cache", LEGACY_CACHE);
    let first = build(&Root::new(&root)).index;
    let again = build(&Root::new(&root)).index;
    assert_eq!(first, again);
    let main = first
        .file(first.find_file(&RelativePath::new("src/main.rs")).unwrap())
        .unwrap();
    assert_eq!(main.text().all().len(), 26);
    assert_eq!(main.highlights().len(), 26);
    assert_eq!(fs::read(root.join(".codemap-cache")).unwrap(), LEGACY_CACHE);
    drop(fs::remove_dir_all(&root));
}

#[test]
fn the_fixture_symbols_are_the_legacy_ones() {
    let (root, index) = linked_fixture("symbols");
    let symbols: Vec<String> = index
        .symbol_ids()
        .map(|id| {
            let symbol = index.symbol(id).unwrap();
            format!(
                "{}:{}-{} {} {}{} ({} calls, {} callers)",
                index.file(id.file()).unwrap().path(),
                symbol.span().start().number(),
                symbol.span().end().number(),
                symbol.kind(),
                "  ".repeat(symbol.depth().value() as usize),
                symbol.name(),
                symbol.callees().len(),
                symbol.callers().len()
            )
        })
        .collect();
    assert_eq!(
        symbols,
        [
            "c/lib.c:1-1 preproc_include #include \"lib.h\" (0 calls, 0 callers)",
            "c/lib.c:3-5 function_definition static int square(int x) (0 calls, 0 callers)",
            "c/lib.c:7-13 function_definition int sum_squares(int n) (0 calls, 0 callers)",
            "c/lib.h:1-1 declaration int sum_squares(int n); (0 calls, 0 callers)",
            "src/main.rs:7-11 function_item main (3 calls, 0 callers)",
            "src/main.rs:13-16 function_item fill (1 calls, 1 callers)",
            "src/main.rs:18-22 function_item report (2 calls, 1 callers)",
            "src/main.rs:24-26 function_item log_line (0 calls, 1 callers)",
            "src/shapes.rs:1-4 trait_item Shape (0 calls, 0 callers)",
            "src/shapes.rs:2-2 function_signature_item   area (0 calls, 0 callers)",
            "src/shapes.rs:3-3 function_signature_item   name (0 calls, 0 callers)",
            "src/shapes.rs:6-8 struct_item Circle (0 calls, 0 callers)",
            "src/shapes.rs:10-12 struct_item Square (0 calls, 0 callers)",
            "src/shapes.rs:14-21 impl_item impl Shape for Circle (0 calls, 0 callers)",
            "src/shapes.rs:15-17 function_item   area (0 calls, 1 callers)",
            "src/shapes.rs:18-20 function_item   name (0 calls, 0 callers)",
            "src/shapes.rs:23-30 impl_item impl Shape for Square (0 calls, 0 callers)",
            "src/shapes.rs:24-26 function_item   area (0 calls, 0 callers)",
            "src/shapes.rs:27-29 function_item   name (0 calls, 0 callers)",
            "src/shapes.rs:33-35 function_item describe (0 calls, 0 callers)",
            "src/store.rs:3-5 struct_item Store (0 calls, 0 callers)",
            "src/store.rs:7-32 impl_item impl Store (0 calls, 0 callers)",
            "src/store.rs:8-10 function_item   new (0 calls, 1 callers)",
            "src/store.rs:12-15 function_item   add (1 calls, 1 callers)",
            "src/store.rs:17-19 function_item   len (1 calls, 1 callers)",
            "src/store.rs:21-27 function_item   total_area (1 calls, 1 callers)",
            "src/store.rs:29-31 function_item   check (0 calls, 1 callers)",
            "tools/helpers.py:1-2 function_definition mean (0 calls, 2 callers)",
            "tools/stats.py:5-7 function_definition spread (1 calls, 1 callers)",
            "tools/stats.py:10-15 class_definition Summary (0 calls, 0 callers)",
            "tools/stats.py:11-12 function_definition   __init__ (0 calls, 0 callers)",
            "tools/stats.py:14-15 function_definition   show (2 calls, 1 callers)",
            "tools/stats.py:18-19 function_definition main (1 calls, 0 callers)",
        ]
    );
    assert_eq!(
        names(&index, &index.roots()),
        ["main src/main.rs:7-11", "main tools/stats.py:18-19"]
    );
    drop(fs::remove_dir_all(&root));
}

#[test]
fn the_fixture_callees_and_callers_are_the_legacy_ones() {
    let (root, index) = linked_fixture("lists");
    let callees_of = |query: &str| {
        listing(&index, &find(&index, query), |id| {
            index.symbol(id).unwrap().callees().to_vec()
        })
    };
    let incoming_of = |query: &str| {
        listing(&index, &find(&index, query), |id| {
            index.symbol(id).unwrap().callers().to_vec()
        })
    };
    assert_eq!(
        callees_of("main"),
        [
            "main src/main.rs:7-11",
            "  fill src/main.rs:13-16",
            "  report src/main.rs:18-22",
            "  new src/store.rs:8-10",
            "main tools/stats.py:18-19",
            "  show tools/stats.py:14-15",
        ]
    );
    assert_eq!(
        callees_of("report"),
        [
            "report src/main.rs:18-22",
            "  log_line src/main.rs:24-26",
            "  total_area src/store.rs:21-27",
        ]
    );
    assert_eq!(
        callees_of("fill"),
        ["fill src/main.rs:13-16", "  add src/store.rs:12-15"]
    );
    assert_eq!(
        callees_of("add"),
        ["add src/store.rs:12-15", "  check src/store.rs:29-31"]
    );
    assert_eq!(
        callees_of("total_area"),
        [
            "total_area src/store.rs:21-27",
            "  area src/shapes.rs:15-17"
        ]
    );
    assert_eq!(
        callees_of("show"),
        [
            "show tools/stats.py:14-15",
            "  mean tools/helpers.py:1-2",
            "  spread tools/stats.py:5-7",
        ]
    );
    assert_eq!(
        callees_of("spread"),
        ["spread tools/stats.py:5-7", "  mean tools/helpers.py:1-2"]
    );
    assert_eq!(
        incoming_of("mean"),
        [
            "mean tools/helpers.py:1-2",
            "  spread tools/stats.py:5-7",
            "  show tools/stats.py:14-15",
        ]
    );
    assert_eq!(
        incoming_of("log_line"),
        ["log_line src/main.rs:24-26", "  report src/main.rs:18-22"]
    );
    assert_eq!(
        incoming_of("check"),
        ["check src/store.rs:29-31", "  add src/store.rs:12-15"]
    );
    assert_eq!(
        incoming_of("area"),
        [
            "area src/shapes.rs:2-2",
            "area src/shapes.rs:15-17",
            "  total_area src/store.rs:21-27",
            "area src/shapes.rs:24-26",
        ]
    );
    assert_eq!(
        incoming_of("new"),
        ["new src/store.rs:8-10", "  main src/main.rs:7-11"]
    );
    assert_eq!(
        incoming_of("len"),
        ["len src/store.rs:17-19", "  len src/store.rs:17-19"]
    );
    drop(fs::remove_dir_all(&root));
}

#[test]
fn the_fixture_call_trees_are_the_legacy_ones() {
    let (root, index) = linked_fixture("tree");
    let tree = |query: &str, depth: u32| -> Vec<String> {
        find(&index, query)
            .into_iter()
            .flat_map(|start| index.call_tree(start, Depth::new(depth)))
            .map(|entry| {
                format!(
                    "{}{}",
                    "  ".repeat(entry.depth.value() as usize),
                    describe(&index, entry.symbol)
                )
            })
            .collect()
    };
    assert_eq!(
        tree("main", 4),
        [
            "main src/main.rs:7-11",
            "  fill src/main.rs:13-16",
            "    add src/store.rs:12-15",
            "      check src/store.rs:29-31",
            "  report src/main.rs:18-22",
            "    log_line src/main.rs:24-26",
            "    total_area src/store.rs:21-27",
            "      area src/shapes.rs:15-17",
            "  new src/store.rs:8-10",
            "main tools/stats.py:18-19",
            "  show tools/stats.py:14-15",
            "    mean tools/helpers.py:1-2",
            "    spread tools/stats.py:5-7",
        ]
    );
    assert_eq!(
        tree("src/main.rs:main", 2),
        [
            "main src/main.rs:7-11",
            "  fill src/main.rs:13-16",
            "    add src/store.rs:12-15",
            "  report src/main.rs:18-22",
            "    log_line src/main.rs:24-26",
            "    total_area src/store.rs:21-27",
            "  new src/store.rs:8-10",
        ]
    );
    drop(fs::remove_dir_all(&root));
}

#[test]
fn a_background_link_gives_the_same_edges() {
    let (root, index) = linked_fixture("background");
    let only = index.symbols_only();
    let mut linked = only.clone();
    link(&mut linked);
    let mut taken = build(&Root::new(&root)).index;
    taken.take_edges(&linked);
    assert_eq!(taken, index);
    drop(fs::remove_dir_all(&root));
}

#[test]
fn a_server_answer_replaces_symbols_when_the_hash_matches() {
    let mut index = parsed(&[("x.rs", "fn a() { b(); }\nfn b() {}\n")]);
    let file = index.files().next().unwrap();
    let hash = file.hash();
    let symbols: Vec<_> = file.symbols().take(1).cloned().collect();
    apply(
        &mut index,
        ServerFile {
            path: RelativePath::new("x.rs"),
            hash: domain::TextHash::new(hash.value() + 1),
            symbols: symbols.clone(),
        },
    );
    assert_eq!(index.files().next().unwrap().symbols().count(), 2);
    apply(
        &mut index,
        ServerFile {
            path: RelativePath::new("x.rs"),
            hash,
            symbols,
        },
    );
    let answered = index.files().next().unwrap();
    assert_eq!(answered.symbols().count(), 1);
    assert_eq!(answered.backend(), Backend::Server);
    assert_eq!(answered.readiness(), Readiness::Ready);
}

#[test]
fn server_files_whose_targets_changed_are_pending_again() {
    let root = fixture("repend");
    let mut index = build(&Root::new(&root)).index;
    for path in ["src/main.rs", "tools/stats.py"] {
        let id = index.find_file(&RelativePath::new(path)).unwrap();
        let file = index.file_mut(id).unwrap();
        file.set_backend(Backend::Server);
        let target = if path == "src/main.rs" {
            "src/store.rs"
        } else {
            "tools/helpers.py"
        };
        for symbol in file.symbols_mut() {
            symbol.set_targets(vec![domain::Location {
                file: RelativePath::new(target),
                line: domain::Line::new(0),
            }]);
        }
    }
    crate::save_cache(&index).unwrap();
    put(&root, "src/store.rs", b"pub struct Store;\n");
    let again = build(&Root::new(&root)).index;
    let state: Vec<(String, Backend, bool)> = again
        .files()
        .map(|file| (file.path().to_string(), file.backend(), file.is_pending()))
        .collect();
    assert_eq!(
        state,
        [
            ("README.md".to_owned(), Backend::TreeSitter, false),
            ("c/lib.c".to_owned(), Backend::TreeSitter, true),
            ("c/lib.h".to_owned(), Backend::TreeSitter, true),
            ("src/main.rs".to_owned(), Backend::Server, true),
            ("src/shapes.rs".to_owned(), Backend::TreeSitter, true),
            ("src/store.rs".to_owned(), Backend::TreeSitter, true),
            ("tools/helpers.py".to_owned(), Backend::TreeSitter, true),
            ("tools/stats.py".to_owned(), Backend::Server, false),
        ]
    );
    let main = again
        .find_symbols(&SymbolQuery::from("src/main.rs:main"))
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(
        names(&again, again.symbol(main).unwrap().callees()),
        [
            "fill src/main.rs:13-16",
            "report src/main.rs:18-22",
            "Store src/store.rs:1-1"
        ]
    );
    let mut given_up = again.clone();
    given_up.give_up(domain::Language::Rust);
    assert!(
        given_up
            .pending()
            .iter()
            .all(|batch| batch.language != domain::Language::Rust)
    );
    drop(fs::remove_dir_all(&root));
}

fn notice_text(notice: &ServerNotice) -> String {
    match notice {
        ServerNotice::ToIndex { program, files } => format!("{program}: {files} files to index"),
        ServerNotice::Unavailable(error) => {
            let reason = match error {
                StartError::Missing(program) => format!("{program} not on PATH"),
                StartError::Failed(program) => format!("{program} would not start"),
            };
            format!("{reason}: its files keep the tree-sitter resolver")
        }
    }
}

#[test]
#[ignore = "needs rust-analyzer on PATH"]
fn rust_analyzer_indexes_the_files_a_command_touches() {
    let root = fixture("server");
    put(
        &root,
        "Cargo.toml",
        b"[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    let mut index = build(&Root::new(&root)).index;
    let notices = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&notices);
    let mut servers = crate::Servers::new(&Root::new(&root), move |notice| {
        seen.borrow_mut().push(notice_text(notice));
    });
    servers.index(&mut index, &[RelativePath::new("src/main.rs")]);
    assert_eq!(
        notices.borrow().as_slice(),
        ["rust-analyzer: 1 files to index"]
    );
    let main_file = index
        .file(index.find_file(&RelativePath::new("src/main.rs")).unwrap())
        .unwrap();
    assert_eq!(main_file.backend(), Backend::Server);
    assert!(!main_file.is_pending());
    let main = find(&index, "src/main.rs:main")[0];
    let callees = names(&index, index.symbol(main).unwrap().callees());
    assert!(
        callees.contains(&"fill src/main.rs:13-16".to_owned()),
        "{callees:?}"
    );
    let check = find(&index, "check")[0];
    let incoming = servers.incoming_calls(&index, check).unwrap();
    assert_eq!(incoming.len(), 1, "{incoming:?}");
    let references = servers.references(&index, check).unwrap();
    assert_ne!(references.len(), 0, "{references:?}");
    drop(servers);
    drop(fs::remove_dir_all(&root));
}

const FILES: &[(&str, &str)] = &[
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

struct RefusingCallsIn(RelativePath);

impl IndexQueries for RefusingCallsIn {
    type Item = RelativePath;

    fn outlines(&mut self, files: &[RelativePath]) -> Vec<Reply<Vec<Outline>>> {
        files
            .iter()
            .map(|file| {
                if file.as_str() == "empty.rs" {
                    return Reply::Given(Vec::new());
                }
                if file.as_str() == "lost.rs" {
                    return Reply::Unanswered;
                }
                Reply::Given(vec![Outline {
                    name: SymbolName::new("go"),
                    kind: OutlineKind::new(12),
                    selection: Position {
                        line: Line::new(0),
                        character: Character::new(3),
                    },
                    end: Line::new(2),
                    range_end: RangeEnd::Inside,
                    children: Vec::new(),
                }])
            })
            .collect()
    }

    fn call_items(&mut self, positions: &[DocumentPosition]) -> Vec<Reply<Vec<RelativePath>>> {
        positions
            .iter()
            .map(|position| Reply::Given(vec![position.file.clone()]))
            .collect()
    }

    fn outgoing_targets(&mut self, items: &[RelativePath]) -> Vec<Reply<Vec<Location>>> {
        items
            .iter()
            .map(|file| {
                if *file == self.0 {
                    Reply::Unanswered
                } else {
                    Reply::Given(vec![Location {
                        file: RelativePath::new("empty.rs"),
                        line: Line::new(0),
                    }])
                }
            })
            .collect()
    }
}

#[test]
fn a_file_with_an_unanswered_request_is_left_out_so_it_stays_pending() {
    let files: Vec<FileVersion> = ["a.rs", "refused.rs", "lost.rs", "empty.rs"]
        .iter()
        .map(|path| FileVersion {
            path: RelativePath::new(path),
            hash: TextHash::of(path.as_bytes()),
        })
        .collect();
    let answered = index_files(
        &mut RefusingCallsIn(RelativePath::new("refused.rs")),
        &files,
    );
    let paths: Vec<&str> = answered.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["a.rs", "empty.rs"]);
    let targets: Vec<usize> = answered
        .iter()
        .map(|file| {
            file.symbols
                .iter()
                .map(|symbol| symbol.targets().len())
                .sum()
        })
        .collect();
    assert_eq!(targets, [1, 0]);
}
