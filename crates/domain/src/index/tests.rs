use std::path::Path as FsPath;

use super::*;
use crate::text::{ByteOffset, FileText, TextHash};

fn span(start: u32, end: u32) -> Span {
    Span::new(Line::new(start), Line::new(end)).unwrap()
}

fn symbol(name: &str, start: u32, end: u32, depth: u32, owner: Option<&str>) -> Symbol {
    Symbol::new(
        SymbolName::new(name),
        SymbolKind::new("function"),
        span(start, end),
        Depth::new(depth),
        owner.map(TypeName::new),
        Vec::new(),
    )
}

fn file(path: &str, text: &str, symbols: Vec<Symbol>) -> SourceFile {
    let text = FileText::from(text);
    let highlights = vec![Vec::new(); text.all().len()];
    let hash = text.whole_hash();
    SourceFile::new(
        RelativePath::new(path),
        text,
        highlights,
        symbols,
        Imports::new(),
        hash,
        Backend::TreeSitter,
    )
}

fn fixture() -> Index {
    let mut index = Index::new(Root::new(FsPath::new(".")));
    index.push(file(
        "src/a.rs",
        "struct A;\nimpl A {\n    fn new() {}\n    fn go() {}\n}\nfn helper() {}\n",
        vec![
            symbol("A", 0, 0, 0, None),
            symbol("impl A", 1, 4, 0, Some("A")),
            symbol("new", 2, 2, 1, Some("A")),
            symbol("go", 3, 3, 1, Some("A")),
            symbol("helper", 5, 5, 0, None),
        ],
    ));
    index.push(file(
        "src/b.rs",
        "fn run() {\n    new();\n}\nfn new() {}\nfn helper() {}\n",
        vec![
            symbol("run", 0, 2, 0, None),
            symbol("new", 3, 3, 0, Some("B")),
            symbol("helper", 4, 4, 0, None),
        ],
    ));
    index
}

fn id(index: &Index, path: &str, name: &str) -> SymbolId {
    index
        .by_key(&SymbolKey {
            file: RelativePath::new(path),
            name: SymbolName::new(name),
        })
        .unwrap()
}

fn names(index: &Index, ids: &[SymbolId]) -> Vec<String> {
    ids.iter()
        .map(|id| {
            format!(
                "{}:{}",
                index.file(id.file()).unwrap().path(),
                index.symbol(*id).unwrap().name()
            )
        })
        .collect()
}

fn find(index: &Index, query: &str) -> Vec<String> {
    names(index, &index.find_symbols(&SymbolQuery::from(query)))
}

#[test]
fn find_symbols_reads_every_qualifier_form() {
    let index = fixture();
    assert_eq!(find(&index, "new"), ["src/a.rs:new", "src/b.rs:new"]);
    assert_eq!(find(&index, "A::new"), ["src/a.rs:new"]);
    assert_eq!(find(&index, "B::new"), ["src/b.rs:new"]);
    assert_eq!(find(&index, "a::new"), ["src/a.rs:new"]);
    assert_eq!(find(&index, "b:new"), ["src/b.rs:new"]);
    assert_eq!(find(&index, "a:A::new"), ["src/a.rs:new"]);
    assert!(find(&index, "b:A::new").is_empty());
    assert_eq!(find(&index, "src\\a.rs:helper"), ["src/a.rs:helper"]);
    assert_eq!(find(&index, "a.rs:helper"), ["src/a.rs:helper"]);
    assert!(find(&index, "missing").is_empty());
}

#[test]
fn lookups_pick_the_innermost_symbol() {
    let index = fixture();
    let path = RelativePath::new("src/a.rs");
    assert_eq!(
        names(&index, &[index.by_line(&path, Line::new(2)).unwrap()]),
        ["src/a.rs:new"]
    );
    assert_eq!(
        names(&index, &[index.by_line(&path, Line::new(1)).unwrap()]),
        ["src/a.rs:impl A"]
    );
    assert_eq!(index.by_line(&path, Line::new(6)), None);
    let go = id(&index, "src/a.rs", "go");
    assert_eq!(index.by_key(&index.symbol_key(go).unwrap()), Some(go));
    assert_eq!(index.find_file(&RelativePath::new("src/c.rs")), None);
}

#[test]
fn roots_sort_by_callee_count_and_keep_ties_in_place() {
    let mut index = fixture();
    let run = id(&index, "src/b.rs", "run");
    let go = id(&index, "src/a.rs", "go");
    let helper_a = id(&index, "src/a.rs", "helper");
    let new_a = id(&index, "src/a.rs", "new");
    let new_b = id(&index, "src/b.rs", "new");
    let helper_b = id(&index, "src/b.rs", "helper");
    let edge = |from, to| Edge { from, to };
    index.connect(&[
        edge(go, new_a),
        edge(helper_a, helper_b),
        edge(run, new_a),
        edge(run, new_b),
        edge(run, helper_b),
    ]);
    assert_eq!(index.roots(), [run, go, helper_a]);
    assert_eq!(index.symbol(helper_b).unwrap().callers(), [helper_a, run]);
}

#[test]
fn call_tree_is_depth_limited_pre_order_visiting_each_symbol_once() {
    let mut index = fixture();
    let run = id(&index, "src/b.rs", "run");
    let new_a = id(&index, "src/a.rs", "new");
    let new_b = id(&index, "src/b.rs", "new");
    let helper_b = id(&index, "src/b.rs", "helper");
    let edge = |from, to| Edge { from, to };
    index.connect(&[
        edge(run, new_a),
        edge(run, new_b),
        edge(new_a, helper_b),
        edge(new_a, run),
        edge(new_b, helper_b),
    ]);
    let tree = |deepest| {
        index
            .call_tree(run, Depth::new(deepest))
            .into_iter()
            .map(|entry| (entry.symbol, entry.depth.value()))
            .collect::<Vec<_>>()
    };
    assert_eq!(tree(0), [(run, 0)]);
    assert_eq!(tree(1), [(run, 0), (new_a, 1), (new_b, 1)]);
    assert_eq!(tree(5), [(run, 0), (new_a, 1), (helper_b, 2), (new_b, 1)]);
}

#[test]
fn call_sites_skip_comments_strings_and_partial_words() {
    let text = FileText::from(
        "    x.foo(); // foo\n    let s = \"foo\";\n    foobar();\n    // foo()\n    call(foo);\n",
    );
    let highlight = |start, end, class| Highlight {
        start: ByteOffset::new(start),
        end: ByteOffset::new(end),
        class,
    };
    let highlights = vec![
        vec![highlight(13, 19, HighlightClass::Comment)],
        vec![highlight(12, 17, HighlightClass::String)],
        Vec::new(),
        vec![highlight(4, 12, HighlightClass::Comment)],
        vec![highlight(4, 8, HighlightClass::Function)],
    ];
    let hash = text.whole_hash();
    let source = SourceFile::new(
        RelativePath::new("c.rs"),
        text,
        highlights,
        Vec::new(),
        Imports::new(),
        hash,
        Backend::TreeSitter,
    );
    let foo = SymbolName::new("foo");
    let at = |line| source.call_site(Line::new(line), &foo);
    assert!(at(0));
    assert!(!at(1));
    assert!(!at(2));
    assert!(!at(3));
    assert!(at(4));
    assert!(!at(9));
}

#[test]
fn languages_come_from_the_last_dotted_part() {
    let of = |path: &str| Language::of(&RelativePath::new(path));
    assert_eq!(of("src/main.rs"), Some(Language::Rust));
    assert_eq!(of("web/app.tsx"), Some(Language::Javascript));
    assert_eq!(of("inc/x.h"), Some(Language::Clang));
    assert_eq!(of("Makefile"), None);
    assert_eq!(Language::Python.program().as_str(), "pyright-langserver");
    assert_eq!(
        Language::Javascript
            .arguments()
            .iter()
            .map(|argument| argument.as_str())
            .collect::<Vec<_>>(),
        ["--stdio"]
    );
    assert!(Language::Rust.arguments().is_empty());
}

#[test]
fn pending_files_group_by_language_until_given_up() {
    let mut index = fixture();
    index.push(file("x.py", "", Vec::new()));
    for source in index.files_mut() {
        source.set_readiness(Readiness::Pending);
    }
    let batches = index.pending();
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].language, Language::Rust);
    assert_eq!(batches[0].files.len(), 2);
    assert_eq!(batches[1].language, Language::Python);
    index.give_up(Language::Rust);
    assert_eq!(index.pending().len(), 1);
}

#[test]
fn replaced_text_keeps_one_highlight_row_per_line() {
    let mut source = file("a.rs", "a\nb", Vec::new());
    source.set_text(FileText::from("a\nb\nc\n"));
    assert_eq!(source.highlights().len(), 3);
    assert!(source.highlights_on(Line::new(7)).is_empty());
    assert_eq!(source.hash(), TextHash::of(b"a\nb"));
}

#[test]
fn edges_move_between_indexes_of_the_same_files() {
    let mut index = fixture();
    let run = id(&index, "src/b.rs", "run");
    let new_a = id(&index, "src/a.rs", "new");
    let mut linked = index.symbols_only();
    assert!(linked.file(run.file()).unwrap().text().all().is_empty());
    linked.connect(&[Edge {
        from: run,
        to: new_a,
    }]);
    index.take_edges(&linked);
    assert_eq!(index.symbol(run).unwrap().callees(), [new_a]);
    index.push(file("more.rs", "", Vec::new()));
    let mut other = index.symbols_only();
    other.connect(&[]);
    index.take_edges(&linked);
    assert_eq!(index.symbol(run).unwrap().callees(), [new_a]);
}
