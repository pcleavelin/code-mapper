use std::path::Path as FsPath;

use super::*;
use crate::id::{Key, Position};
use crate::index::{Backend, Edge, Imports, Root, SourceFile, Symbol, SymbolKind, SymbolName};
use crate::text::{FileText, Line, LineCount, LineOffset, SourceLine, TextHash};

fn span(start: u32, end: u32) -> Span {
    Span::new(Line::new(start), Line::new(end)).unwrap()
}

fn name(text: &str) -> PathName {
    PathName::new(text).unwrap()
}

fn symbol(text: &str, start: u32, end: u32) -> Symbol {
    Symbol::new(
        SymbolName::new(text),
        SymbolKind::new("fn"),
        span(start, end),
        Depth::new(0),
        None,
        Vec::new(),
    )
}

fn source(lines: &[&str], symbols: Vec<Symbol>) -> SourceFile {
    let text = FileText::from(lines.join("\n").as_str());
    let highlights = vec![Vec::new(); lines.len()];
    let hash = text.whole_hash();
    SourceFile::new(
        RelativePath::new("a.rs"),
        text,
        highlights,
        symbols,
        Imports::new(),
        hash,
        Backend::TreeSitter,
    )
}

fn one_file() -> Index {
    let mut index = Index::new(Root::new(FsPath::new(".")));
    index.push(source(
        &["fn a() {", "  1", "}", "fn b() {", "  2", "}"],
        vec![symbol("a", 0, 2), symbol("b", 3, 5)],
    ));
    index
}

fn first_file() -> FileId {
    FileId::at(Position::new(0))
}

fn file(text: &str) -> RelativePath {
    RelativePath::new(text)
}

fn address(path: &str, step: &StepId) -> StepAddress {
    StepAddress {
        path: name(path),
        step: step.clone(),
    }
}

fn reloaded(map: &Map) -> Map {
    let paths = map
        .paths()
        .iter()
        .map(|path| {
            let steps = path
                .steps()
                .iter()
                .map(|step| {
                    Step::new(
                        step.id().clone(),
                        step.order(),
                        step.parent().cloned(),
                        step.author(),
                        step.anchor().clone(),
                        step.note().cloned(),
                        step.link().cloned(),
                    )
                })
                .collect();
            Path::new(
                path.name().clone(),
                path.kind(),
                path.author(),
                path.group().cloned(),
                path.note().cloned(),
                steps,
            )
            .unwrap()
        })
        .collect();
    Map::new(paths).unwrap()
}

#[test]
fn fresh_ids_match_the_legacy_generator() {
    assert_eq!(
        StepId::fresh("anchor\nsrc/map.rs\nfresh_id\n0\n14\n", |_| false).as_str(),
        "lp72jd"
    );
    assert_eq!(
        StepId::fresh("map-file\nsrc/map.rs\nAuthor\n0\n3\nuivdyq", |_| false).as_str(),
        "42292h"
    );
    let first = StepId::fresh("x", |_| false);
    let second = StepId::fresh("x", |id| id == &first);
    assert_ne!(first, second);
    assert!(StepId::new(second.as_str()).is_some());
    assert!(StepId::new("ABCDEF").is_none());
    assert!(StepId::new("abcde").is_none());
}

#[test]
fn names_follow_the_legacy_rules() {
    assert!(matches!(PathName::new(""), Err(MapError::InvalidName(_))));
    assert!(PathName::new(".hidden").is_err());
    assert!(PathName::new("a b").is_err());
    assert!(PathName::new("a.b_c-d9").is_ok());
    let from = |text: &str| PathName::from_symbol(&SymbolName::new(text)).to_string();
    assert_eq!(from("impl Foo<T>"), "impl-Foo-T");
    assert_eq!(from("::"), "path");
    assert_eq!(from(".x."), "x");
    assert_eq!(
        GroupName::new(" /flows//http/ ").unwrap().as_str(),
        "flows/http"
    );
    assert_eq!(GroupName::new(" / "), None);
    assert_eq!(Note::new(""), None);
}

#[test]
fn resolve_follows_symbols_and_marks_stale_steps() {
    let index = one_file();
    let mut map = Map::default();
    let path = name("p");
    let _added = map
        .add_path(path.clone(), PathKind::Type, Author::Agent)
        .unwrap();
    let id = map
        .add_step(&index, &path, first_file(), span(4, 4), Author::Agent, None)
        .unwrap();
    assert_eq!(id.as_str(), "rjpl22");
    let _noted = map
        .set_step_note(&path, &id, Note::new("the middle"))
        .unwrap();
    let step = map.step(&path, &id).unwrap();
    assert_eq!(step.symbol().unwrap().as_str(), "b");
    assert_eq!(step.anchor().start(), LineOffset::new(1));
    assert!(!step.is_stale());

    let mut loaded = reloaded(&map);
    assert!(loaded.step(&path, &id).unwrap().is_stale());
    assert_eq!(loaded.path(&path).unwrap().kind(), PathKind::Type);

    let lines = [
        "// x", "// y", "fn a() {", "  1", "}", "fn b() {", "  2", "}",
    ];
    let mut shifted = Index::new(Root::new(FsPath::new(".")));
    shifted.push(source(&lines, vec![symbol("a", 2, 4), symbol("b", 5, 7)]));
    loaded.resolve_all(&shifted);
    let shifted_step = loaded.step(&path, &id).unwrap();
    assert_eq!(shifted_step.span(), span(6, 6));
    assert!(!shifted_step.is_stale());

    let mut edited = lines;
    edited[6] = "  3";
    let mut changed = Index::new(Root::new(FsPath::new(".")));
    changed.push(source(&edited, vec![symbol("a", 2, 4), symbol("b", 5, 7)]));
    loaded.resolve_all(&changed);
    let edited_step = loaded.step(&path, &id).unwrap();
    assert!(edited_step.is_stale());
    assert_eq!(edited_step.span(), span(6, 6));
    assert!(edited_step.resolved_symbol().is_some());
    assert!(!loaded.coverage().covers(&file("a.rs"), span(6, 6)));

    let _pinned = loaded
        .pin(
            &changed,
            &path,
            &id,
            first_file(),
            span(6, 6),
            Author::Human,
        )
        .unwrap();
    let pinned_step = loaded.step(&path, &id).unwrap();
    assert!(!pinned_step.is_stale());
    assert_eq!(pinned_step.note().unwrap().as_str(), "the middle");
    assert_eq!(pinned_step.author(), Author::Human);
    assert_eq!(pinned_step.id(), &id);
    assert!(loaded.coverage().covers(&file("a.rs"), span(5, 7)));
    assert!(!loaded.coverage().covers(&file("a.rs"), span(0, 2)));

    let mut renamed = Index::new(Root::new(FsPath::new(".")));
    renamed.push(source(&edited, vec![symbol("a", 2, 4), symbol("c", 5, 7)]));
    loaded.resolve_all(&renamed);
    let renamed_step = loaded.step(&path, &id).unwrap();
    assert!(renamed_step.is_stale());
    assert_eq!(renamed_step.resolved_symbol(), None);
    assert_eq!(renamed_step.span(), span(6, 6));

    loaded.resolve_all(&Index::new(Root::new(FsPath::new("."))));
    assert!(loaded.step(&path, &id).unwrap().is_stale());
}

#[test]
fn resolve_prefers_the_candidate_whose_text_still_matches() {
    let lines = ["fn f() {", "  1", "}", "fn f() {", "  2", "}"];
    let mut index = Index::new(Root::new(FsPath::new(".")));
    index.push(source(&lines, vec![symbol("f", 0, 2), symbol("f", 3, 5)]));
    let anchor = Anchor::new(
        file("a.rs"),
        Some(SymbolName::new("f")),
        LineOffset::new(1),
        LineOffset::new(1),
        TextHash::of(b"  2\n"),
    );
    let resolution = anchor.resolve(&index).unwrap();
    assert_eq!(resolution.span, span(4, 4));
    assert_eq!(resolution.freshness, Freshness::Current);
    let absolute = Anchor::new(
        file("a.rs"),
        None,
        LineOffset::new(5),
        LineOffset::new(9),
        TextHash::default(),
    );
    assert_eq!(absolute.resolve(&index), None);
}

#[test]
fn tree_edits_keep_parents() {
    let index = one_file();
    let mut map = Map::default();
    let path = name("t");
    let _added = map
        .add_path(path.clone(), PathKind::Flow, Author::Human)
        .unwrap();
    let add = |target: &mut Map, start, end, parent: Option<&StepId>| {
        target
            .add_step(
                &index,
                &path,
                first_file(),
                span(start, end),
                Author::Human,
                parent,
            )
            .unwrap()
    };
    let root = add(&mut map, 0, 2, None);
    let middle = add(&mut map, 3, 5, Some(&root));
    let leaf = add(&mut map, 1, 1, Some(&middle));
    let order = |target: &Map| {
        target
            .path(&path)
            .unwrap()
            .tree_order()
            .into_iter()
            .map(|placed| (placed.step, placed.depth.value()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        order(&map),
        [(root.clone(), 0), (middle.clone(), 1), (leaf.clone(), 2)]
    );
    let numbers: Vec<String> = map
        .path(&path)
        .unwrap()
        .numbered(&index)
        .into_iter()
        .map(|numbered| numbered.number.to_string())
        .collect();
    assert_eq!(numbers, ["1", "1.1", "1.1.1"]);
    let the_path = map.path(&path).unwrap();
    assert_eq!(the_path.descendants(&root).len(), 2);
    assert!(the_path.descendants(&leaf).is_empty());
    assert_eq!(
        the_path.parent_label(&leaf),
        Some(ParentLabel::Symbol(SymbolName::new("b")))
    );
    assert_eq!(the_path.parent_label(&root), Some(ParentLabel::TopLevel));
    assert_eq!(
        the_path
            .steps()
            .iter()
            .map(|step| step.order().value())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );

    let removed = map.remove_step(&path, &middle).unwrap();
    assert_eq!(removed.id(), &middle);
    assert_eq!(map.path(&path).unwrap().steps().len(), 2);
    assert_eq!(map.step(&path, &leaf).unwrap().parent(), Some(&root));
    assert_eq!(order(&map), [(root.clone(), 0), (leaf.clone(), 1)]);

    assert_eq!(
        map.reparent(&path, &root, Some(&leaf)),
        Err(MapError::UnderItself)
    );
    assert_eq!(
        map.reparent(&path, &root, Some(&root)),
        Err(MapError::UnderItself)
    );
    let _moved = map.reparent(&path, &leaf, None).unwrap();
    assert_eq!(order(&map), [(root.clone(), 0), (leaf.clone(), 0)]);
    assert_eq!(
        map.path(&path).unwrap().parent_label(&leaf),
        Some(ParentLabel::TopLevel)
    );
}

#[test]
fn numbering_orders_children_by_where_the_parent_calls_them() {
    let lines = [
        "fn main() {",
        "  let x = 1;",
        "  early();",
        "  late();",
        "}",
        "fn late() {}",
        "fn early() {}",
        "fn other() {}",
    ];
    let mut index = Index::new(Root::new(FsPath::new(".")));
    index.push(source(
        &lines,
        vec![
            symbol("main", 0, 4),
            symbol("late", 5, 5),
            symbol("early", 6, 6),
            symbol("other", 7, 7),
        ],
    ));
    let mut map = Map::default();
    let path = name("n");
    let _added = map
        .add_path(path.clone(), PathKind::Flow, Author::Agent)
        .unwrap();
    let add = |target: &mut Map, line, parent: Option<&StepId>| {
        target
            .add_step(
                &index,
                &path,
                first_file(),
                span(line, line),
                Author::Agent,
                parent,
            )
            .unwrap()
    };
    let main = map
        .add_step(&index, &path, first_file(), span(0, 4), Author::Agent, None)
        .unwrap();
    let other = add(&mut map, 7, Some(&main));
    let late = add(&mut map, 5, Some(&main));
    let early = add(&mut map, 6, Some(&main));
    let numbered: Vec<(StepId, String)> = map
        .path(&path)
        .unwrap()
        .numbered(&index)
        .into_iter()
        .map(|numbered| (numbered.step, numbered.number.to_string()))
        .collect();
    assert_eq!(
        numbered,
        [
            (main, "1".to_owned()),
            (early, "1.1".to_owned()),
            (late, "1.2".to_owned()),
            (other, "1.3".to_owned()),
        ]
    );
}

#[test]
fn swaps_trade_orders_and_keep_parents() {
    let index = one_file();
    let mut map = Map::default();
    let path = name("s");
    let _added = map
        .add_path(path.clone(), PathKind::Layer, Author::Agent)
        .unwrap();
    let first = map
        .add_step(&index, &path, first_file(), span(0, 2), Author::Agent, None)
        .unwrap();
    let second = map
        .add_step(&index, &path, first_file(), span(3, 5), Author::Agent, None)
        .unwrap();
    let child = map
        .add_step(
            &index,
            &path,
            first_file(),
            span(4, 4),
            Author::Agent,
            Some(&second),
        )
        .unwrap();
    let _swapped = map.swap(&path, &first, &second).unwrap();
    let steps = map.path(&path).unwrap().steps();
    assert_eq!(steps[0].id(), &second);
    assert_eq!(steps[0].order().value(), 0);
    assert_eq!(steps[1].id(), &first);
    assert_eq!(steps[1].order().value(), 1);
    assert_eq!(map.step(&path, &child).unwrap().parent(), Some(&second));
    let reloaded = reloaded(&map);
    let ids: Vec<&StepId> = reloaded
        .path(&path)
        .unwrap()
        .steps()
        .iter()
        .map(Step::id)
        .collect();
    assert_eq!(ids, [&second, &first, &child]);
}

#[test]
fn place_hangs_a_step_and_puts_it_before_a_sibling() {
    let index = one_file();
    let mut map = Map::default();
    let path = name("p");
    let _added = map
        .add_path(path.clone(), PathKind::Layer, Author::Agent)
        .unwrap();
    let mut add = |start: u32, parent: Option<&StepId>| {
        map.add_step(
            &index,
            &path,
            first_file(),
            span(start, start),
            Author::Human,
            parent,
        )
        .unwrap()
    };
    let root = add(0, None);
    let one = add(1, Some(&root));
    let two = add(2, Some(&root));
    let three = add(3, None);
    let ids = |placed: &Map| -> Vec<StepId> {
        placed
            .path(&path)
            .unwrap()
            .steps()
            .iter()
            .map(|step| step.id().clone())
            .collect()
    };
    let _placed = map.place(&path, &three, Some(&root), Some(&one)).unwrap();
    assert_eq!(
        ids(&map),
        [root.clone(), three.clone(), one.clone(), two.clone()]
    );
    assert_eq!(map.step(&path, &three).unwrap().parent(), Some(&root));
    let orders: Vec<u32> = map
        .path(&path)
        .unwrap()
        .steps()
        .iter()
        .map(|step| step.order().value())
        .collect();
    assert_eq!(orders, [0, 1, 2, 3]);
    let _hung = map.place(&path, &one, Some(&two), None).unwrap();
    assert_eq!(
        ids(&map),
        [root.clone(), three.clone(), two.clone(), one.clone()]
    );
    assert_eq!(map.step(&path, &one).unwrap().parent(), Some(&two));
    assert_eq!(
        map.place(&path, &root, Some(&one), None),
        Err(MapError::UnderItself)
    );
    assert_eq!(ids(&reloaded(&map)), ids(&map));
}

#[test]
fn links_follow_renames_and_hold_removal() {
    let index = one_file();
    let mut map = Map::default();
    let flow = name("flow");
    let shared = name("shared");
    let _flow = map
        .add_path(flow.clone(), PathKind::Flow, Author::Agent)
        .unwrap();
    let _shared = map
        .add_path(shared.clone(), PathKind::Layer, Author::Agent)
        .unwrap();
    let step = map
        .add_step(&index, &flow, first_file(), span(0, 2), Author::Agent, None)
        .unwrap();
    let _other = map
        .add_step(
            &index,
            &shared,
            first_file(),
            span(3, 5),
            Author::Agent,
            None,
        )
        .unwrap();
    assert_eq!(
        map.set_link(&flow, &step, Some(name("nope"))),
        Err(MapError::NoSuchPath(name("nope")))
    );
    assert_eq!(
        map.set_link(&flow, &step, Some(flow.clone())),
        Err(MapError::LinkToOwnPath)
    );
    let _linked = map.set_link(&flow, &step, Some(shared.clone())).unwrap();
    assert_eq!(map.links_to(&shared), [address("flow", &step)]);

    let mut map = reloaded(&map);
    assert_eq!(map.step(&flow, &step).unwrap().link(), Some(&shared));
    let common = name("common");
    let _renamed = map.rename(&shared, common.clone()).unwrap();
    assert_eq!(map.step(&flow, &step).unwrap().link(), Some(&common));
    assert_eq!(
        map.remove_path(&common),
        Err(MapError::LinkedFrom {
            path: common.clone(),
            steps: vec![address("flow", &step)],
        })
    );
    let _pinned = map
        .pin(
            &index,
            &flow,
            &step,
            first_file(),
            span(1, 1),
            Author::Agent,
        )
        .unwrap();
    assert_eq!(map.step(&flow, &step).unwrap().link(), Some(&common));

    let broken: Vec<Path> = map
        .paths()
        .iter()
        .map(|path| {
            let steps = path
                .steps()
                .iter()
                .map(|old| {
                    Step::new(
                        old.id().clone(),
                        old.order(),
                        old.parent().cloned(),
                        old.author(),
                        old.anchor().clone(),
                        old.note().cloned(),
                        old.link().map(|_| name("gone")),
                    )
                })
                .collect();
            Path::new(
                path.name().clone(),
                path.kind(),
                path.author(),
                None,
                None,
                steps,
            )
            .unwrap()
        })
        .collect();
    let mut broken_map = Map::new(broken).unwrap();
    assert_eq!(broken_map.dangling_links(), [address("flow", &step)]);
    let _unlinked = broken_map.set_link(&flow, &step, None).unwrap();
    assert!(broken_map.dangling_links().is_empty());
    assert!(broken_map.remove_path(&common).is_ok());
    assert_eq!(
        broken_map.remove_path(&common),
        Err(MapError::NoSuchPath(common))
    );
}

#[test]
fn names_clash_by_letter_case() {
    let mut map = Map::default();
    let _added = map
        .add_path(name("Flow"), PathKind::Flow, Author::Agent)
        .unwrap();
    let _again = map
        .add_path(name("Flow"), PathKind::Type, Author::Human)
        .unwrap();
    assert_eq!(map.paths().len(), 1);
    assert_eq!(map.paths()[0].kind(), PathKind::Flow);
    assert_eq!(
        map.add_path(name("flow"), PathKind::Flow, Author::Agent),
        Err(MapError::CaseClash {
            name: name("flow"),
            other: name("Flow"),
        })
    );
    let _other = map
        .add_path(name("other"), PathKind::Flow, Author::Agent)
        .unwrap();
    assert_eq!(
        map.rename(&name("other"), name("Flow")),
        Err(MapError::NameTaken(name("Flow")))
    );
    assert!(map.rename(&name("Flow"), name("FLOW")).is_ok());
    assert_eq!(
        Map::new(vec![
            Path::new(
                name("x"),
                PathKind::Flow,
                Author::Agent,
                None,
                None,
                Vec::new()
            )
            .unwrap(),
            Path::new(
                name("x"),
                PathKind::Flow,
                Author::Agent,
                None,
                None,
                Vec::new()
            )
            .unwrap(),
        ]),
        Err(MapError::NameTaken(name("x")))
    );
}

#[test]
fn groups_nest_and_rename() {
    let mut map = Map::default();
    for (path, group) in [
        ("top", ""),
        ("a", "flows/http"),
        ("b", "areas"),
        ("c", "flows"),
        ("d", " /flows//http/ "),
    ] {
        let _added = map
            .add_path(name(path), PathKind::Flow, Author::Agent)
            .unwrap();
        let _grouped = map.set_group(&name(path), GroupName::new(group)).unwrap();
    }
    assert_eq!(map.paths()[4].group().unwrap().as_str(), "flows/http");
    let group = |text: &str, depth, paths| Row::Group {
        group: GroupName::new(text).unwrap(),
        depth: Depth::new(depth),
        paths: PathCount::new(paths),
    };
    let row = |text: &str, depth| Row::Path {
        name: name(text),
        depth: Depth::new(depth),
    };
    assert_eq!(
        map.rows(),
        [
            group("areas", 0, 1),
            row("b", 1),
            group("flows", 0, 3),
            group("flows/http", 1, 2),
            row("a", 2),
            row("d", 2),
            row("c", 1),
            row("top", 0),
        ]
    );
    assert_eq!(
        map.rows_where(|path| matches!(path.name().as_str(), "d" | "top")),
        [
            group("flows", 0, 1),
            group("flows/http", 1, 1),
            row("d", 2),
            row("top", 0),
        ]
    );

    let group_of = |target: &Map, path: &str| {
        target
            .path(&name(path))
            .unwrap()
            .group()
            .map(|found| found.as_str().to_owned())
    };
    let flows = GroupName::new("flows").unwrap();
    let work_flows = GroupName::new("work/flows").unwrap();
    assert_eq!(
        map.rename_group(Some(&flows), Some(&work_flows)),
        Ok(PathCount::new(3))
    );
    assert_eq!(group_of(&map, "a").as_deref(), Some("work/flows/http"));
    assert_eq!(group_of(&map, "c").as_deref(), Some("work/flows"));
    let flow = GroupName::new("flow").unwrap();
    assert_eq!(
        map.rename_group(Some(&flow), GroupName::new("x").as_ref()),
        Err(MapError::NoSuchGroup(flow))
    );
    assert_eq!(map.rename_group(None, None), Err(MapError::NoGroupGiven));
    let work = GroupName::new("work").unwrap();
    assert_eq!(map.rename_group(Some(&work), None), Ok(PathCount::new(3)));
    assert_eq!(group_of(&map, "c").as_deref(), Some("flows"));
    let _cleared = map.set_group(&name("c"), None).unwrap();
    assert_eq!(group_of(&map, "c"), None);
}

#[test]
fn note_edits_replace_the_first_match() {
    let index = one_file();
    let mut map = Map::default();
    let path = name("n");
    let _added = map
        .add_path(path.clone(), PathKind::Flow, Author::Agent)
        .unwrap();
    let _noted = map.set_path_note(&path, Note::new("one two one")).unwrap();
    let edited = map
        .edit_path_note(
            &path,
            &TextFragment::new("one"),
            &TextFragment::new("three"),
        )
        .unwrap();
    assert_eq!(edited.unwrap().as_str(), "three two one");
    assert_eq!(
        map.edit_path_note(&path, &TextFragment::new("four"), &TextFragment::new("x")),
        Err(MapError::NoteLacks(TextFragment::new("four")))
    );
    let step = map
        .add_step(&index, &path, first_file(), span(0, 2), Author::Agent, None)
        .unwrap();
    let _step_noted = map.set_step_note(&path, &step, Note::new("gone")).unwrap();
    let cleared = map
        .edit_step_note(
            &path,
            &step,
            &TextFragment::new("gone"),
            &TextFragment::new(""),
        )
        .unwrap();
    assert_eq!(cleared, None);
    assert_eq!(map.step(&path, &step).unwrap().note(), None);
    let missing = StepId::fresh("missing", |_| false);
    assert_eq!(
        map.set_step_note(&path, &missing, None),
        Err(MapError::NoSuchStep(address("n", &missing)))
    );
    assert_eq!(
        map.add_step(
            &index,
            &path,
            first_file(),
            span(0, 2),
            Author::Agent,
            Some(&missing)
        ),
        Err(MapError::NoSuchParent)
    );
    assert_eq!(
        map.add_step(&index, &path, first_file(), span(5, 6), Author::Agent, None),
        Err(MapError::OutsideFile)
    );
}

#[test]
fn promote_adds_only_symbols_the_path_does_not_pin_whole() {
    let mut index = one_file();
    let root = index.by_line(&file("a.rs"), Line::new(0)).unwrap();
    let child = index.by_line(&file("a.rs"), Line::new(3)).unwrap();
    index.connect(&[Edge {
        from: root,
        to: child,
    }]);
    let mut map = Map::default();
    let path = map
        .promote(
            &index,
            root,
            Depth::new(5),
            None,
            Author::Agent,
            Pruning::All,
        )
        .unwrap()
        .name;
    assert_eq!(path, name("a"));
    let tree: Vec<(Option<String>, u32)> = map
        .path(&path)
        .unwrap()
        .tree_order()
        .into_iter()
        .map(|placed| {
            let step = map.step(&path, &placed.step).unwrap();
            (
                step.symbol().map(|symbol| symbol.as_str().to_owned()),
                placed.depth.value(),
            )
        })
        .collect();
    assert_eq!(tree, [(Some("a".to_owned()), 0), (Some("b".to_owned()), 1)]);
    let again = map
        .promote(
            &index,
            root,
            Depth::new(5),
            None,
            Author::Agent,
            Pruning::All,
        )
        .unwrap()
        .name;
    assert_eq!(again, path);
    assert_eq!(map.path(&path).unwrap().steps().len(), 2);

    let b_step = map.path(&path).unwrap().steps()[1].id().clone();
    let _pinned = map
        .pin(
            &index,
            &path,
            &b_step,
            first_file(),
            span(4, 4),
            Author::Agent,
        )
        .unwrap();
    let _again = map
        .promote(
            &index,
            root,
            Depth::new(5),
            None,
            Author::Agent,
            Pruning::All,
        )
        .unwrap()
        .name;
    assert_eq!(map.path(&path).unwrap().steps().len(), 3);

    let shallow = map
        .promote(
            &index,
            root,
            Depth::new(0),
            Some(name("shallow")),
            Author::Human,
            Pruning::All,
        )
        .unwrap()
        .name;
    assert_eq!(map.path(&shallow).unwrap().steps().len(), 1);
}

#[test]
fn diff_reports_each_kind_of_change() {
    let index = one_file();
    let mut map = Map::default();
    let kept = name("kept");
    let gone = name("gone");
    let _kept = map
        .add_path(kept.clone(), PathKind::Flow, Author::Agent)
        .unwrap();
    let _gone = map
        .add_path(gone.clone(), PathKind::Flow, Author::Agent)
        .unwrap();
    let add = |target: &mut Map, start, end| {
        target
            .add_step(
                &index,
                &kept,
                first_file(),
                span(start, end),
                Author::Agent,
                None,
            )
            .unwrap()
    };
    let moved = add(&mut map, 0, 2);
    let noted = add(&mut map, 3, 5);
    let linked = add(&mut map, 4, 4);
    let removed = add(&mut map, 1, 1);
    let same = add(&mut map, 5, 5);
    let base = map.clone();
    assert!(
        map.diff(&base)
            .iter()
            .all(|diff| diff.change() == Change::Same)
    );

    let _pinned = map
        .pin(
            &index,
            &kept,
            &moved,
            first_file(),
            span(0, 1),
            Author::Agent,
        )
        .unwrap();
    let _noted = map.set_step_note(&kept, &noted, Note::new("new")).unwrap();
    let added = name("added");
    let _added = map
        .add_path(added.clone(), PathKind::Flow, Author::Agent)
        .unwrap();
    let _added_step = map
        .add_step(
            &index,
            &added,
            first_file(),
            span(0, 0),
            Author::Agent,
            None,
        )
        .unwrap();
    let _linked = map.set_link(&kept, &linked, Some(added.clone())).unwrap();
    let _removed = map.remove_step(&kept, &removed).unwrap();
    let fresh = add(&mut map, 2, 2);
    let _grouped = map.set_group(&kept, GroupName::new("g")).unwrap();
    let _removed_path = map.remove_path(&gone).unwrap();

    let diffs = map.diff(&base);
    assert_eq!(
        diffs
            .iter()
            .map(|diff| (diff.name().as_str(), diff.change()))
            .collect::<Vec<_>>(),
        [
            ("kept", Change::Changed),
            ("added", Change::Added),
            ("gone", Change::Removed)
        ]
    );
    let kept_diff = &diffs[0];
    assert!(kept_diff.note_changed());
    let changes: Vec<(StepId, Option<StepChange>)> = kept_diff
        .steps()
        .iter()
        .map(|step| (step.step.clone(), step.change))
        .collect();
    assert_eq!(
        changes,
        [
            (moved, Some(StepChange::Repinned)),
            (noted, Some(StepChange::NoteEdited)),
            (linked.clone(), Some(StepChange::Relinked)),
            (same, None),
            (fresh, Some(StepChange::Added)),
        ]
    );
    let removed_ids: Vec<&StepId> = kept_diff.removed().iter().map(Step::id).collect();
    assert_eq!(removed_ids, [&removed]);
    assert_eq!(diffs[1].steps()[0].change, Some(StepChange::Added));
    assert!(!diffs[1].note_changed());
    assert!(diffs[2].steps().is_empty());
    assert!(diffs[2].removed().is_empty());
}

#[test]
fn diff_reports_a_changed_link_alone() {
    let index = one_file();
    let mut map = Map::default();
    let (from, to) = (name("from"), name("to"));
    for path in [&from, &to] {
        let _added = map
            .add_path(path.clone(), PathKind::Flow, Author::Agent)
            .unwrap();
    }
    let step = map
        .add_step(&index, &from, first_file(), span(0, 2), Author::Agent, None)
        .unwrap();
    let base = map.clone();
    let _linked = map.set_link(&from, &step, Some(to.clone())).unwrap();
    let diffs = map.diff(&base);
    assert!(!diffs[0].note_changed());
    assert_eq!(diffs[0].change(), Change::Changed);
    assert_eq!(diffs[0].steps()[0].change, Some(StepChange::Relinked));
    assert_eq!(diffs[1].change(), Change::Same);
}

#[test]
fn slices_follow_the_diff() {
    let lines = |texts: &[&str]| {
        texts
            .iter()
            .map(|text| SourceLine::new(text))
            .collect::<Vec<_>>()
    };
    let old = lines(&[
        "fn a() {",
        "    let x = 1;",
        "    let y = 2;",
        "    x + y",
        "}",
        "fn b() {",
        "    0",
        "}",
    ]);
    let new = lines(&[
        "mod m {",
        "  fn b() {",
        "      0",
        "  }",
        "}",
        "fn a() {",
        "    let x = 1;",
        "    log();",
        "    let y = 3;",
        "    x + y",
        "}",
    ]);
    let at = |start: u32, end: u32, low: usize, high: usize| {
        follow(&old, span(start, end), &new[low..=high]).map(|followed| {
            (
                low + followed.span.start().value() as usize,
                low + followed.span.end().value() as usize,
                followed.kept,
            )
        })
    };
    assert_eq!(at(5, 7, 1, 3), Some((1, 3, LineCount::new(3))));
    assert_eq!(at(0, 4, 5, 10), Some((5, 10, LineCount::new(4))));
    assert_eq!(at(1, 3, 5, 10), Some((6, 9, LineCount::new(2))));
    assert_eq!(at(0, 2, 5, 10), Some((5, 8, LineCount::new(2))));
    assert_eq!(at(2, 2, 5, 10), None);

    let followed = follow(&old, span(0, 4), &new).unwrap();
    assert_eq!(followed.alignment.get(Line::new(0)), Some(Line::new(5)));
    assert_eq!(followed.alignment.get(Line::new(2)), None);
    assert_eq!(followed.alignment.get(Line::new(40)), None);
}

#[test]
fn promote_stops_at_code_another_path_covers_and_names_the_path_to_link() {
    let mut index = one_file();
    let first = index.by_line(&file("a.rs"), Line::new(0)).unwrap();
    let second = index.by_line(&file("a.rs"), Line::new(3)).unwrap();
    index.connect(&[Edge {
        from: first,
        to: second,
    }]);
    let mut map = Map::default();
    let promote = |into: &mut Map, root, called: &str, pruning| {
        into.promote(
            &index,
            root,
            Depth::new(2),
            Some(name(called)),
            Author::Agent,
            pruning,
        )
        .unwrap()
    };
    let links = |from: &Map, promoted: &Promoted| -> Vec<(usize, String)> {
        let steps = from.path(&promoted.name).unwrap().steps();
        promoted
            .links
            .iter()
            .map(|link| {
                (
                    steps
                        .iter()
                        .position(|step| step.id() == &link.step)
                        .unwrap(),
                    link.target.to_string(),
                )
            })
            .collect()
    };
    let all = promote(&mut map, first, "a-covering", Pruning::All);
    assert!(all.links.is_empty() && all.stopped.is_empty());
    let only = promote(&mut map, first, "one", Pruning::Pruned);
    assert_eq!(
        only.stopped.values().copied().collect::<Vec<_>>(),
        [Stop::Mapped]
    );
    assert_eq!(links(&map, &only), [(1, "a-covering".to_owned())]);
    let _rooted = promote(&mut map, second, "z-rooted", Pruning::Pruned);
    let rooted = promote(&mut map, first, "two", Pruning::Pruned);
    assert_eq!(links(&map, &rooted), [(1, "z-rooted".to_owned())]);
    let _removed = map.remove_path(&name("z-rooted")).unwrap();
    let ambiguous = promote(&mut map, first, "three", Pruning::Pruned);
    assert_eq!(
        ambiguous.stopped.values().copied().collect::<Vec<_>>(),
        [Stop::Mapped]
    );
    assert!(links(&map, &ambiguous).is_empty());
}
