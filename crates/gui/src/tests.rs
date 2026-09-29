use std::path::Path as FsPath;

use domain::{
    Anchor, Author, Backend, Depth, FileText, Imports, Index, Line, Map, Path, PathKind, PathName,
    RelativePath, Root, SourceFile, Span, Step, StepId, StepOrder, Symbol, SymbolKind, SymbolName,
};
use features::Trigger;
use io_map::MapStore;
use ui::{Count, Label};

use crate::action::{Action, Fold, Hide};
use crate::app::App;
use crate::graph::Parentage;
use crate::graph::build::{Built, CellSize, Rank, StepInfo};
use crate::graph::{Button, Node};
use crate::ids::CONTROLS;
use crate::keys::Walk;
use crate::model::{Model, PathSlot, Readable, StepKey, StepSlot, Tab, ViewFlag};
use crate::nav::Scrolling;
use crate::panels::Direction;
use crate::theme::Cells;

#[test]
fn every_control_is_named_by_its_feature() {
    for control in CONTROLS {
        let named = control
            .feature()
            .spec()
            .triggers()
            .iter()
            .any(|trigger| match trigger {
                Trigger::Click(element) | Trigger::Type(element) | Trigger::Gesture(_, element) => {
                    *element == control.element()
                }
                Trigger::Command(_) | Trigger::Key(_) => false,
            });
        assert!(named, "{control:?} is not a trigger of its feature");
    }
}

#[test]
fn every_graph_button_is_named_by_its_feature() {
    for button in [
        Button::Preview,
        Button::Listing,
        Button::Callees,
        Button::Callers,
        Button::Above,
        Button::Below,
        Button::NoContext,
    ] {
        let named = button.feature().spec().triggers().iter().any(
            |trigger| matches!(trigger, Trigger::Click(element) if *element == button.element()),
        );
        assert!(named, "{button:?} is not a trigger of its feature");
    }
}

const SOURCE: &str = "fn main() {\n    fill();\n    report();\n}\nfn fill() {\n    add();\n}\nfn report() {}\nfn add() {}\n";

fn symbol(name: &str, start: u32, end: u32) -> Symbol {
    Symbol::new(
        SymbolName::new(name),
        SymbolKind::new("function"),
        Span::new(Line::new(start), Line::new(end)).unwrap(),
        Depth::new(0),
        None,
        Vec::new(),
    )
}

fn index() -> Index {
    let text = FileText::from(SOURCE);
    let highlights = vec![Vec::new(); text.all().len()];
    let hash = text.whole_hash();
    let mut index = Index::new(Root::new(FsPath::new("/nowhere")));
    index.push(SourceFile::new(
        RelativePath::new("src/main.rs"),
        text,
        highlights,
        vec![
            symbol("main", 0, 3),
            symbol("fill", 4, 6),
            symbol("report", 7, 7),
            symbol("add", 8, 8),
        ],
        Imports::new(),
        hash,
        Backend::TreeSitter,
    ));
    index
}

fn step(index: &Index, id: &str, order: u32, parent: Option<&str>, start: u32, end: u32) -> Step {
    let file = index
        .file(index.find_file(&RelativePath::new("src/main.rs")).unwrap())
        .unwrap();
    let span = Span::new(Line::new(start), Line::new(end)).unwrap();
    Step::new(
        StepId::new(id).unwrap(),
        StepOrder::new(order),
        parent.map(|parent| StepId::new(parent).unwrap()),
        Author::Agent,
        Anchor::at(file, span).unwrap(),
        None,
        None,
    )
}

fn model() -> Model {
    let index = index();
    let steps = vec![
        step(&index, "aaaaaa", 0, None, 0, 3),
        step(&index, "bbbbbb", 1, Some("aaaaaa"), 4, 6),
        step(&index, "cccccc", 2, Some("bbbbbb"), 8, 8),
        step(&index, "dddddd", 3, Some("aaaaaa"), 7, 7),
    ];
    let path = Path::new(
        PathName::new("startup").unwrap(),
        PathKind::Flow,
        Author::Agent,
        None,
        None,
        steps,
    )
    .unwrap();
    let mut map = Map::new(vec![path]).unwrap();
    map.resolve_all(&index);
    let store = MapStore::new(&Root::new(FsPath::new("/nowhere")));
    Model::new(index, map, store, Readable::Reads)
}

const PATH: PathSlot = PathSlot::new(0);

fn key(step: usize) -> StepKey {
    StepKey {
        path: PATH,
        step: StepSlot::new(step),
    }
}

#[test]
fn opening_a_path_selects_its_first_step_in_tree_order() {
    let mut model = model();
    model.select_path(PATH);
    assert_eq!(model.nav.step(), Some(StepSlot::new(0)));
    assert_eq!(model.nav.tab(), Tab::Path);
    let focus = model
        .nav
        .focus()
        .and_then(|symbol| model.index.symbol(symbol));
    assert_eq!(focus.map(|symbol| symbol.name().as_str()), Some("main"));
}

#[test]
fn walking_follows_the_tree_and_stops_at_the_ends() {
    let mut model = model();
    model.select_path(PATH);
    let mut seen = Vec::new();
    for _ in 0..5 {
        model.walk(Walk::Down);
        seen.push(model.nav.step().unwrap().get());
    }
    assert_eq!(seen, [1, 2, 3, 3, 3]);
    model.walk(Walk::Up);
    assert_eq!(model.nav.step(), Some(StepSlot::new(2)));
}

#[test]
fn back_and_forward_return_to_the_places_visited() {
    let mut model = model();
    model.select_path(PATH);
    model.track_navigation();
    model.select_step(key(2), Scrolling::Scroll);
    model.track_navigation();
    model.set_tab(Tab::Graph);
    model.track_navigation();
    assert!(model.nav.can_go_back());
    model.back();
    assert_eq!(model.nav.tab(), Tab::Path);
    assert_eq!(model.nav.step(), Some(StepSlot::new(2)));
    model.back();
    assert_eq!(model.nav.step(), Some(StepSlot::new(0)));
    assert!(!model.nav.can_go_back());
    model.forward();
    model.forward();
    assert_eq!(model.nav.tab(), Tab::Graph);
    assert!(!model.nav.can_go_forward());
}

#[test]
fn history_keeps_the_last_two_hundred_places() {
    let mut model = model();
    model.select_path(PATH);
    model.track_navigation();
    for round in 0..250 {
        model.select_step(key(round % 2), Scrolling::Scroll);
        model.track_navigation();
    }
    let mut steps_back = 0;
    while model.nav.can_go_back() {
        model.back();
        steps_back += 1;
    }
    assert_eq!(steps_back, 200);
}

#[test]
fn selecting_a_step_unfolds_its_ancestors() {
    let mut model = model();
    model.views.entry(key(0)).flags.set(ViewFlag::Folded, true);
    model.views.entry(key(1)).flags.set(ViewFlag::Folded, true);
    model.select_step(key(2), Scrolling::Stay);
    assert!(!model.views.get(key(0)).flags.has(ViewFlag::Folded));
    assert!(!model.views.get(key(1)).flags.has(ViewFlag::Folded));
    assert_eq!(model.nav.scroll_to_step(), None);
}

fn app() -> App {
    App::of_model(model(), &Root::new(FsPath::new("/nowhere")))
}

#[test]
fn hide_all_hides_every_step_and_show_all_unfolds_none() {
    let mut app = app();
    app.apply(Action::Toggle(key(1), ViewFlag::Folded));
    app.apply(Action::HideAll(PATH, Hide::Hide));
    for step in 0..4 {
        assert!(app.model.views.get(key(step)).flags.has(ViewFlag::Hidden));
    }
    assert!(app.model.views.get(key(1)).flags.has(ViewFlag::Folded));
    app.apply(Action::HideAll(PATH, Hide::Show));
    for step in 0..4 {
        let flags = app.model.views.get(key(step)).flags;
        assert!(!flags.has(ViewFlag::Hidden));
        assert!(!flags.has(ViewFlag::Folded));
    }
}

#[test]
fn fold_all_folds_only_steps_with_children() {
    let mut app = app();
    app.apply(Action::FoldAll(PATH, Fold::Fold));
    let folded: Vec<bool> = (0..4)
        .map(|step| app.model.views.get(key(step)).flags.has(ViewFlag::Folded))
        .collect();
    assert_eq!(folded, [true, true, false, false]);
    app.apply(Action::FoldAll(PATH, Fold::Unfold));
    assert!(!app.model.views.get(key(0)).flags.has(ViewFlag::Folded));
}

#[test]
fn removing_a_step_moves_later_views_up_and_clears_the_selection() {
    let mut app = app();
    app.apply(Action::SelectStep(key(1), Scrolling::Stay));
    app.apply(Action::Toggle(key(3), ViewFlag::Whole));
    app.apply(Action::RemoveStep(key(1)));
    assert_eq!(app.model.nav.step(), None);
    assert_eq!(app.model.step_count(PATH), Count::new(3));
    assert!(app.model.views.get(key(2)).flags.has(ViewFlag::Whole));
    assert!(!app.model.views.get(key(3)).flags.has(ViewFlag::Whole));
    assert_eq!(
        app.model.status.to_string(),
        "deleted step 1.1 fill (src/main.rs) from 'startup'; unsaved"
    );
}

#[test]
fn removing_the_path_forgets_it() {
    let mut app = app();
    app.apply(Action::OpenPath(PATH, Tab::Path));
    app.apply(Action::RemovePath(PATH));
    assert_eq!(app.model.nav.path(), None);
    assert!(app.model.map.paths().is_empty());
    assert_eq!(
        app.model.status.to_string(),
        "deleted path 'startup' (4 steps); unsaved"
    );
}

fn built(model: &Model, sizes: &[(i32, i32)]) -> Built {
    let symbols: Vec<_> = model.index.symbol_ids().collect();
    let mut built = Built::default();
    for (position, (wide, tall)) in sizes.iter().enumerate() {
        let node = Node {
            symbol: symbols[position],
            step: Some(StepSlot::new(position)),
        };
        built.nodes.push(node);
        built.rank.insert(
            node,
            if position == 0 {
                Rank::ZERO
            } else {
                Rank::ZERO.next()
            },
        );
        built.size.insert(
            node,
            CellSize {
                wide: Cells::new(*wide),
                tall: Cells::new(*tall),
            },
        );
        built.step.insert(
            node,
            StepInfo {
                order: Count::new(position),
                slot: StepSlot::new(position),
                number: Label::default(),
            },
        );
        if position > 0 {
            built.step_parent.insert(node, built.nodes[0]);
        }
    }
    built
}

#[test]
fn children_sit_together_in_one_box_beside_their_parent() {
    let model = model();
    let mut built = built(&model, &[(10, 4), (8, 3), (8, 3)]);
    built.place_nodes(&model.index);
    let at = |position: usize| {
        let point = built.position(built.nodes[position]).unwrap();
        (point.across.get(), point.down.get())
    };
    assert_eq!(at(0), (1, 2));
    assert_eq!(at(1), (21, 2));
    assert_eq!(at(2), (21, 6));
    let parentages: Vec<Parentage> = built
        .siblings
        .iter()
        .map(|siblings| siblings.parentage)
        .collect();
    assert_eq!(
        parentages,
        [Parentage::Path, Parentage::Callees(built.nodes[0])]
    );
    let bounds = built.bounds().unwrap();
    assert_eq!(
        (
            bounds.left.get(),
            bounds.top.get(),
            bounds.right.get(),
            bounds.bottom.get()
        ),
        (0, 0, 30, 10)
    );
}

#[test]
fn turned_the_children_sit_below_their_parent_side_by_side() {
    let model = model();
    let mut built = built(&model, &[(10, 4), (8, 3), (8, 3)]);
    built.direction = Direction::Down;
    built.place_nodes(&model.index);
    let at = |position: usize| {
        let point = built.position(built.nodes[position]).unwrap();
        (point.across.get(), point.down.get())
    };
    assert_eq!(at(0), (1, 2));
    assert_eq!(at(1), (1, 13));
    assert_eq!(at(2), (11, 13));
}

#[test]
fn a_path_linked_from_elsewhere_is_refused_with_the_linking_steps() {
    let index = index();
    let file = index
        .file(index.find_file(&RelativePath::new("src/main.rs")).unwrap())
        .unwrap();
    let anchor = Anchor::at(file, Span::new(Line::new(0), Line::new(3)).unwrap()).unwrap();
    let linking = Step::new(
        StepId::new("aaaaaa").unwrap(),
        StepOrder::new(0),
        None,
        Author::Agent,
        anchor,
        None,
        Some(PathName::new("target").unwrap()),
    );
    let path = |name: &str, steps: Vec<Step>| {
        Path::new(
            PathName::new(name).unwrap(),
            PathKind::Flow,
            Author::Agent,
            None,
            None,
            steps,
        )
        .unwrap()
    };
    let mut map = Map::new(vec![
        path("caller", vec![linking]),
        path("target", Vec::new()),
    ])
    .unwrap();
    map.resolve_all(&index);
    let store = MapStore::new(&Root::new(FsPath::new("/nowhere")));
    let model = Model::new(index, map, store, Readable::Reads);
    let mut app = App::of_model(model, &Root::new(FsPath::new("/nowhere")));
    app.apply(Action::RemovePath(PathSlot::new(1)));
    assert_eq!(app.model.path_count(), Count::new(2));
    assert_eq!(
        app.model.status.to_string(),
        "'target' is linked from caller[0]; unlink those steps first"
    );
}
