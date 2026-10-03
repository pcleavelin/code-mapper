use std::path::Path as FsPath;

use domain::{
    Anchor, Author, Backend, Cut, Depth, Draft, Edge, EditChange, FileText, Imports, Index, Line,
    Map, Note, RelativePath, Root, Row, SourceFile, Span, Step, StepId, StepOrder, Stop, Symbol,
    SymbolId, SymbolKind, SymbolName, Tour, TourKind, TourName, TreeEntry, Verdict,
};
use features::{Feature, Trigger};
use io_map::MapStore;
use strum::VariantArray;
use ui::{Count, Input, Key, Label, Mods, Press, Px};

use crate::action::{Action, Collapse, Hide};
use crate::app::App;
use crate::authoring::Authoring;
use crate::field::{Attention, FieldRoom, Fields, Which};
use crate::graph::Parentage;
use crate::graph::build::{Built, CellSize, Rank, StepInfo};
use crate::graph::{Button, GraphState, Node};
use crate::ids::{self, CONTROLS};
use crate::keys::{LineGesture, Walk};
use crate::model::{LineSelection, Model, Readable, StepKey, StepSlot, Tab, TourSlot, ViewFlag};
use crate::nav::Scrolling;
use crate::palette::{Palette, commands};
use crate::panels::{Direction, View};
use crate::status::{Held, Status};
use crate::theme::Cells;
use crate::wizard::{self, BranchId, Expander, Page, Tick, WizardAct};

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
                Trigger::Command(_) | Trigger::Key(_) | Trigger::Palette(..) => false,
            });
        assert!(named, "{control:?} is not a trigger of its feature");
    }
}

#[test]
fn every_graph_button_is_named_by_its_feature() {
    for button in [
        Button::Preview,
        Button::Source,
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
    let tour = Tour::new(
        TourName::new("startup").unwrap(),
        TourKind::Flow,
        Author::Agent,
        None,
        None,
        steps,
    )
    .unwrap();
    let mut map = Map::new(vec![tour]).unwrap();
    map.resolve_all(&index);
    let store = MapStore::new(&Root::new(FsPath::new("/nowhere")));
    Model::new(index, map, store, Readable::Reads)
}

const TOUR: TourSlot = TourSlot::new(0);

fn key(step: usize) -> StepKey {
    StepKey {
        tour: TOUR,
        step: StepSlot::new(step),
    }
}

#[test]
fn opening_a_tour_selects_its_first_step_in_tree_order() {
    let mut model = model();
    model.select_tour(TOUR);
    assert_eq!(model.nav.step(), Some(StepSlot::new(0)));
    assert_eq!(model.nav.tab(), Tab::Tour);
    let focus = model
        .nav
        .focus()
        .and_then(|symbol| model.index.symbol(symbol));
    assert_eq!(focus.map(|symbol| symbol.name().as_str()), Some("main"));
}

#[test]
fn walking_follows_the_tree_and_stops_at_the_ends() {
    let mut model = model();
    model.select_tour(TOUR);
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
fn back_and_forward_return_to_the_places_visited_as_they_were_left() {
    let mut model = model();
    model.select_tour(TOUR);
    model.track_navigation();
    model.select_step(key(2), Scrolling::Scroll);
    model.track_navigation();
    model.set_tab(Tab::Graph);
    model.track_navigation();
    model.select_step(key(1), Scrolling::Scroll);
    model.track_navigation();
    model.back();
    assert_eq!(model.nav.tab(), Tab::Graph);
    assert_eq!(model.nav.step(), Some(StepSlot::new(2)));
    model.back();
    assert_eq!(model.nav.tab(), Tab::Tour);
    assert_eq!(model.nav.step(), Some(StepSlot::new(2)));
    model.back();
    assert_eq!(model.nav.tab(), Tab::Tour);
    assert_eq!(model.nav.step(), Some(StepSlot::new(0)));
    assert!(!model.nav.can_go_back());
    model.forward();
    model.forward();
    model.forward();
    assert_eq!(model.nav.step(), Some(StepSlot::new(1)));
    assert_eq!(model.nav.tab(), Tab::Graph);
    assert!(!model.nav.can_go_forward());
}

#[test]
fn back_to_the_start_page_leaves_nothing_chosen() {
    let mut model = model();
    model.track_navigation();
    model.select_tour(TOUR);
    model.track_navigation();
    model.back();
    assert_eq!(model.nav.tour(), None);
    assert_eq!(model.nav.step(), None);
    assert_eq!(model.nav.focus(), None);
}

#[test]
fn switching_tabs_is_its_own_place() {
    let mut model = model();
    model.select_tour(TOUR);
    model.track_navigation();
    model.set_tab(Tab::Graph);
    model.track_navigation();
    model.set_tab(Tab::Source);
    model.track_navigation();
    model.back();
    assert_eq!(model.nav.tab(), Tab::Graph);
    model.back();
    assert_eq!(model.nav.tab(), Tab::Tour);
    assert!(!model.nav.can_go_back());
}

#[test]
fn each_walk_is_its_own_place() {
    let mut model = model();
    model.select_tour(TOUR);
    model.track_navigation();
    for _ in 0..3 {
        model.walk(Walk::Down);
        model.track_navigation();
    }
    model.back();
    assert_eq!(model.nav.step(), Some(StepSlot::new(2)));
    model.back();
    assert_eq!(model.nav.step(), Some(StepSlot::new(1)));
    model.back();
    assert_eq!(model.nav.step(), Some(StepSlot::new(0)));
    assert!(!model.nav.can_go_back());
}

#[test]
fn a_jump_within_one_file_is_a_place() {
    let mut model = model();
    let file = model
        .index
        .find_file(&RelativePath::new("src/main.rs"))
        .unwrap();
    model.open_line(file, Line::new(1));
    model.track_navigation();
    model.open_line(file, Line::new(5));
    model.track_navigation();
    model.back();
    assert_eq!(model.nav.lines(), Some(LineSelection::one(Line::new(1))));
}

#[test]
fn dragging_over_lines_selects_from_the_pressed_line_to_the_pointer() {
    let mut model = model();
    model.select_line(Line::new(3), LineGesture::Press);
    model.select_line(Line::new(7), LineGesture::Drag);
    model.select_line(Line::new(5), LineGesture::Drag);
    assert_eq!(
        model.nav.lines(),
        Some(LineSelection {
            from: Line::new(3),
            to: Line::new(5),
        })
    );
    model.select_line(Line::new(1), LineGesture::Drag);
    let upward = model.nav.lines().unwrap();
    assert_eq!((upward.low(), upward.high()), (Line::new(1), Line::new(3)));
}

#[test]
fn a_drag_no_selecting_press_began_leaves_the_selection() {
    let mut model = model();
    let file = model
        .index
        .find_file(&RelativePath::new("src/main.rs"))
        .unwrap();
    model.select_line(Line::new(3), LineGesture::Press);
    model.release_lines();
    model.open_line(file, Line::new(5));
    model.select_line(Line::new(9), LineGesture::Drag);
    assert_eq!(model.nav.lines(), Some(LineSelection::one(Line::new(5))));
}

#[test]
fn back_restores_the_scroll_the_place_was_left_at() {
    let mut model = model();
    model.select_tour(TOUR);
    model.scrolls.set(ids::document(), Px::new(500));
    model.track_navigation();
    model.select_step(key(2), Scrolling::Scroll);
    model.scrolls.set(ids::document(), Px::new(40));
    model.track_navigation();
    model.back();
    assert_eq!(model.scrolls.get(ids::document()), Px::new(500));
    assert_eq!(model.nav.scroll_to_step(), None);
}

#[test]
fn history_finds_its_tour_by_name_after_the_map_is_reloaded() {
    let mut model = model();
    model.select_step(key(2), Scrolling::Scroll);
    model.track_navigation();
    model.select_step(key(1), Scrolling::Scroll);
    model.track_navigation();
    let mut map = model.map.clone();
    assert!(
        map.add_tour(TourName::new("aaa").unwrap(), TourKind::Flow, Author::Human)
            .is_ok()
    );
    let mut tours = map.tours().to_vec();
    tours.rotate_right(1);
    model.map = Map::new(tours).unwrap();
    model.map.resolve_all(&model.index);
    model.reselect(Some(TourSlot::new(1)), Some(StepSlot::new(1)));
    model.back();
    assert_eq!(model.nav.tour(), Some(TourSlot::new(1)));
    assert_eq!(model.nav.step(), Some(StepSlot::new(2)));
}

#[test]
fn history_keeps_the_last_two_hundred_places() {
    let mut model = model();
    model.select_tour(TOUR);
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
fn selecting_a_step_expands_its_ancestors() {
    let mut model = model();
    model
        .views
        .entry(key(0))
        .flags
        .set(ViewFlag::Collapsed, true);
    model
        .views
        .entry(key(1))
        .flags
        .set(ViewFlag::Collapsed, true);
    model.select_step(key(2), Scrolling::Stay);
    assert!(!model.views.get(key(0)).flags.has(ViewFlag::Collapsed));
    assert!(!model.views.get(key(1)).flags.has(ViewFlag::Collapsed));
    assert_eq!(model.nav.scroll_to_step(), None);
}

fn app() -> App {
    App::of_model(model(), &Root::new(FsPath::new("/nowhere")))
}

#[test]
fn hide_all_code_hides_every_step_and_show_all_code_expands_none() {
    let mut app = app();
    app.apply(Action::Toggle(key(1), ViewFlag::Collapsed));
    app.apply(Action::HideAll(TOUR, Hide::Hide));
    for step in 0..4 {
        assert!(app.model.views.get(key(step)).flags.has(ViewFlag::Hidden));
    }
    assert!(app.model.views.get(key(1)).flags.has(ViewFlag::Collapsed));
    app.apply(Action::HideAll(TOUR, Hide::Show));
    for step in 0..4 {
        let flags = app.model.views.get(key(step)).flags;
        assert!(!flags.has(ViewFlag::Hidden));
        assert!(!flags.has(ViewFlag::Collapsed));
    }
}

#[test]
fn collapse_all_collapses_only_steps_with_children() {
    let mut app = app();
    app.apply(Action::CollapseAll(TOUR, Collapse::Collapse));
    let collapsed: Vec<bool> = (0..4)
        .map(|step| {
            app.model
                .views
                .get(key(step))
                .flags
                .has(ViewFlag::Collapsed)
        })
        .collect();
    assert_eq!(collapsed, [true, true, false, false]);
    app.apply(Action::CollapseAll(TOUR, Collapse::Expand));
    assert!(!app.model.views.get(key(0)).flags.has(ViewFlag::Collapsed));
}

#[test]
fn removing_a_step_moves_later_views_up_and_clears_the_selection() {
    let mut app = app();
    app.apply(Action::SelectStep(key(1), Scrolling::Stay));
    app.apply(Action::Toggle(key(3), ViewFlag::Whole));
    app.apply(Action::RemoveStep(key(1)));
    assert_eq!(app.model.nav.step(), None);
    assert_eq!(app.model.step_count(TOUR), Count::new(3));
    assert!(app.model.views.get(key(2)).flags.has(ViewFlag::Whole));
    assert!(!app.model.views.get(key(3)).flags.has(ViewFlag::Whole));
    assert_eq!(
        app.model.status.to_string(),
        "deleted step 1.1 fill (src/main.rs) from 'startup'; unsaved"
    );
}

#[test]
fn adding_the_focused_symbol_hangs_it_under_the_target() {
    let mut app = app();
    let text = FileText::from("fn helper() {}\n");
    let highlights = vec![Vec::new(); text.all().len()];
    let hash = text.whole_hash();
    app.model.index.push(SourceFile::new(
        RelativePath::new("src/helper.rs"),
        text,
        highlights,
        vec![symbol("helper", 0, 0)],
        Imports::new(),
        hash,
        Backend::TreeSitter,
    ));
    app.apply(Action::OpenTour(TOUR, Tab::Graph));
    let helper = app
        .model
        .index
        .symbol_ids()
        .find(|id| {
            app.model
                .index
                .symbol(*id)
                .is_some_and(|found| found.name().as_str() == "helper")
        })
        .unwrap();
    app.apply(Action::Focus(helper));
    app.apply(Action::Authoring(Authoring::AddOffered));
    assert_eq!(app.model.step_count(TOUR), Count::new(5));
    let added = app
        .model
        .tour(TOUR)
        .and_then(|tour| {
            tour.steps()
                .iter()
                .find(|step| step.symbol().is_some_and(|name| name.as_str() == "helper"))
        })
        .unwrap();
    assert_eq!(added.parent().map(StepId::as_str), Some("aaaaaa"));
    assert_eq!(
        app.model.status.to_string(),
        "step 1.3 added to 'startup' under 1"
    );
}

#[test]
fn adding_selected_source_lines_uses_the_same_control() {
    let mut app = app();
    app.apply(Action::OpenTour(TOUR, Tab::Source));
    app.model.select_line(Line::new(1), LineGesture::Press);
    app.apply(Action::Authoring(Authoring::AddOffered));
    assert_eq!(app.model.step_count(TOUR), Count::new(5));
    assert!(
        app.model.status.to_string().contains("added"),
        "{}",
        app.model.status
    );
}

#[test]
fn the_focused_step_itself_is_not_offered_again() {
    let mut app = app();
    app.apply(Action::OpenTour(TOUR, Tab::Graph));
    app.apply(Action::Authoring(Authoring::AddOffered));
    assert_eq!(app.model.step_count(TOUR), Count::new(4));
    assert_eq!(
        app.model.status.to_string(),
        "select another symbol, or lines in the Source view, to add a step"
    );
}

#[test]
fn show_references_opens_the_view_after_it_was_closed() {
    let mut app = app();
    app.model.panels.close_view(View::References);
    assert!(!app.model.panels.is_shown(View::References));
    app.apply(Action::ShowReferences);
    assert!(app.model.panels.is_shown(View::References));
}

#[test]
fn removing_the_tour_forgets_it() {
    let mut app = app();
    app.apply(Action::OpenTour(TOUR, Tab::Tour));
    app.apply(Action::RemoveTour(TOUR));
    assert_eq!(app.model.nav.tour(), None);
    assert_eq!(app.model.map.tours().len(), 0);
    assert_eq!(
        app.model.status.to_string(),
        "deleted tour 'startup' (4 steps); unsaved"
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
        [Parentage::Tour, Parentage::Callees(built.nodes[0])]
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
fn a_tour_linked_from_elsewhere_is_refused_with_the_linking_steps() {
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
        Some(TourName::new("target").unwrap()),
    );
    let tour = |name: &str, steps: Vec<Step>| {
        Tour::new(
            TourName::new(name).unwrap(),
            TourKind::Flow,
            Author::Agent,
            None,
            None,
            steps,
        )
        .unwrap()
    };
    let mut map = Map::new(vec![
        tour("caller", vec![linking]),
        tour("target", Vec::new()),
    ])
    .unwrap();
    map.resolve_all(&index);
    let store = MapStore::new(&Root::new(FsPath::new("/nowhere")));
    let model = Model::new(index, map, store, Readable::Reads);
    let mut app = App::of_model(model, &Root::new(FsPath::new("/nowhere")));
    app.apply(Action::RemoveTour(TourSlot::new(1)));
    assert_eq!(app.model.tour_count(), Count::new(2));
    assert_eq!(
        app.model.status.to_string(),
        "'target' is linked from caller[0]; unlink those steps first"
    );
}

fn filtered(model: &mut Model, text: &str) -> (usize, Vec<String>) {
    model.fields.fill(Which::TourFilter, &Label::new(text));
    let listed = model
        .listed_rows()
        .iter()
        .filter(|row| matches!(row, Row::Tour { .. }))
        .count();
    let found = model
        .found_steps(TOUR)
        .iter()
        .map(|numbered| numbered.number.as_str().to_owned())
        .collect();
    (listed, found)
}

#[test]
fn the_tours_filter_lists_a_tour_by_its_steps_and_names_the_steps_that_match() {
    let mut model = model();
    assert_eq!(filtered(&mut model, ""), (1, Vec::new()));
    assert_eq!(filtered(&mut model, "start"), (1, Vec::new()));
    assert_eq!(filtered(&mut model, "REPORT"), (1, vec!["1.2".to_owned()]));
    assert_eq!(
        filtered(&mut model, "main.rs"),
        (1, ["1", "1.1", "1.1.1", "1.2"].map(str::to_owned).to_vec())
    );
    assert_eq!(filtered(&mut model, "absent"), (0, Vec::new()));
}

#[test]
fn every_palette_entry_in_the_registry_runs_one_command() {
    for feature in Feature::VARIANTS {
        let entries = feature
            .spec()
            .triggers()
            .iter()
            .filter(|trigger| matches!(trigger, Trigger::Palette(..)))
            .count();
        assert_eq!(
            entries,
            commands(*feature).len(),
            "{feature:?} lists {entries} palette entries"
        );
    }
}

fn palette_after(model: &mut Model, typed: &str) -> Vec<(String, String)> {
    model.palette = Some(Palette::default());
    model.fields.fill(Which::Palette, &Label::new(typed));
    model.refresh_palette();
    model
        .palette
        .iter()
        .flat_map(|palette| &palette.entries)
        .map(|entry| {
            (
                entry.kind.tag().as_str().to_owned(),
                entry.name.as_str().to_owned(),
            )
        })
        .collect()
}

fn listed(kind: &str, name: &str) -> (String, String) {
    (kind.to_owned(), name.to_owned())
}

#[test]
fn the_palette_ranks_a_contiguous_match_above_scattered_ones_and_a_chevron_keeps_only_actions() {
    let mut model = model();
    let found = palette_after(&mut model, ">sa");
    assert_eq!(found.first(), Some(&listed("action", "save")));
    assert!(found.iter().all(|(kind, _)| kind == "action"), "{found:?}");
}

#[test]
fn the_palette_lists_symbols_and_files_only_once_something_is_typed() {
    let mut model = model();
    let empty = palette_after(&mut model, "");
    assert!(empty.contains(&listed("tour", "startup")));
    assert!(
        empty
            .iter()
            .all(|(kind, _)| kind != "symbol" && kind != "file"),
        "{empty:?}"
    );
    let typed = palette_after(&mut model, "main");
    assert!(typed.contains(&listed("symbol", "main")), "{typed:?}");
    assert!(typed.contains(&listed("file", "src/main.rs")), "{typed:?}");
}

#[test]
fn the_palette_ranks_a_step_of_the_tour_being_read_above_the_symbol_it_pins() {
    let mut model = model();
    model.select_tour(TOUR);
    let found = palette_after(&mut model, "add");
    assert_eq!(
        found.get(..2),
        Some(&[listed("step", "1.1.1 add"), listed("symbol", "add")][..])
    );
}

#[test]
fn a_graph_node_with_no_calls_says_so_instead_of_leaving_its_buttons_out() {
    let model = model();
    let built = built(&model, &[(10, 4), (10, 4)]);
    let header = built.header(&model, &GraphState::default(), built.nodes[1]);
    let text: String = header.runs.iter().map(|run| run.text.as_str()).collect();
    assert!(text.contains("calls nothing"), "{text}");
    assert!(text.contains("no callers in this repo"), "{text}");
    assert!(!text.contains("calls not in this repo"), "{text}");
}

#[test]
fn a_graph_node_names_calls_the_index_could_not_place_in_the_repo() {
    let mut model = model();
    let symbol = model.index.symbol_ids().next().unwrap();
    model
        .index
        .symbol_mut(symbol)
        .unwrap()
        .set_targets(vec![domain::Location {
            file: RelativePath::new("library/std/vec.rs"),
            line: Line::new(0),
        }]);
    let built = built(&model, &[(10, 4), (10, 4)]);
    let header = built.header(&model, &GraphState::default(), built.nodes[0]);
    let text: String = header.runs.iter().map(|run| run.text.as_str()).collect();
    assert!(text.contains("calls not in this repo"), "{text}");
    assert!(!text.contains("calls nothing"), "{text}");
}

#[test]
fn a_graph_node_offers_callees_of_the_symbol_even_when_the_step_shows_other_lines() {
    let mut model = model();
    let symbols: Vec<_> = model.index.symbol_ids().collect();
    model
        .index
        .symbol_mut(symbols[1])
        .unwrap()
        .set_callees(vec![symbols[3]]);
    let built = built(&model, &[(10, 4), (10, 4)]);
    let header = built.header(&model, &GraphState::default(), built.nodes[1]);
    let labels: Vec<_> = header
        .buttons
        .iter()
        .map(|labelled| labelled.label.as_str().to_owned())
        .collect();
    assert!(
        labels.iter().any(|label| label.contains("callees")),
        "{labels:?}"
    );
}

const DEEP: &str = "fn main() {\n    a();\n    get_x();\n    get_y();\n    t1();\n    t2();\n}\nfn a() {\n    b();\n}\nfn b() {\n    c();\n}\nfn c() {\n    d();\n}\nfn d() {\n    a();\n}\nfn get_x(&self) -> u32 { self.x }\nfn get_y(&self) -> u32 { self.y }\n";

fn deep_app() -> App {
    let text = FileText::from(DEEP);
    let highlights = vec![Vec::new(); text.all().len()];
    let hash = text.whole_hash();
    let mut index = Index::new(Root::new(FsPath::new("/nowhere")));
    index.push(SourceFile::new(
        RelativePath::new("src/main.rs"),
        text,
        highlights,
        vec![
            symbol("main", 0, 6),
            symbol("a", 7, 9),
            symbol("b", 10, 12),
            symbol("c", 13, 15),
            symbol("d", 16, 18),
            symbol("get_x", 19, 19),
            symbol("get_y", 20, 20),
        ],
        Imports::new(),
        hash,
        Backend::TreeSitter,
    ));
    let tests = FileText::from("fn t1() {\n}\nfn t2() {\n}\n");
    let test_highlights = vec![Vec::new(); tests.all().len()];
    let test_hash = tests.whole_hash();
    index.push(SourceFile::new(
        RelativePath::new("src/tests.rs"),
        tests,
        test_highlights,
        vec![symbol("t1", 0, 1), symbol("t2", 2, 3)],
        Imports::new(),
        test_hash,
        Backend::TreeSitter,
    ));
    let at = |name: &str| {
        index
            .symbol_ids()
            .find(|id| index.symbol(*id).unwrap().name().as_str() == name)
            .unwrap()
    };
    let edges: Vec<Edge> = [
        ("main", "a"),
        ("main", "get_x"),
        ("main", "get_y"),
        ("main", "t1"),
        ("main", "t2"),
        ("a", "b"),
        ("b", "c"),
        ("c", "d"),
        ("d", "a"),
    ]
    .into_iter()
    .map(|(from, to)| Edge {
        from: at(from),
        to: at(to),
    })
    .collect();
    index.connect(&edges);
    let store = MapStore::new(&Root::new(FsPath::new("/nowhere")));
    App::of_model(
        Model::new(index, Map::default(), store, Readable::Reads),
        &Root::new(FsPath::new("/nowhere")),
    )
}

fn symbol_named(app: &App, name: &str) -> SymbolId {
    let index = &app.model.index;
    index
        .symbol_ids()
        .find(|id| index.symbol(*id).unwrap().name().as_str() == name)
        .unwrap()
}

fn wizard_rows(app: &App) -> Vec<String> {
    let wizard = app.model.wizard.as_ref().unwrap();
    let outline = wizard.outline();
    outline
        .lines()
        .iter()
        .map(|line| match line {
            wizard::Line::Branch(id) => {
                let branch = outline.branch(*id).unwrap();
                format!(
                    "{}{} {}",
                    "  ".repeat(branch.depth().value() as usize),
                    app.model
                        .index
                        .symbol(branch.symbol().unwrap())
                        .unwrap()
                        .name(),
                    if branch.tick == Tick::Ticked {
                        "x"
                    } else {
                        "-"
                    }
                )
            }
            wizard::Line::Fold(fold) => format!(
                "fold {:?} {} {:?}",
                fold.cut,
                fold.members.len(),
                fold.shown
            ),
        })
        .collect()
}

fn branch_named(app: &App, name: &str) -> BranchId {
    let wizard = app.model.wizard.as_ref().unwrap();
    let outline = wizard.outline();
    let symbol = symbol_named(app, name);
    outline
        .lines()
        .iter()
        .find_map(|line| match line {
            wizard::Line::Branch(id) if outline.branch(*id).unwrap().symbol() == Some(symbol) => {
                Some(*id)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn tour_from_here_opens_the_wizard_on_steps_with_the_start_and_a_name_from_the_symbol() {
    let mut app = deep_app();
    app.model.set_tab(Tab::Source);
    let start = symbol_named(&app, "a");
    app.apply(Action::Wizard(WizardAct::FromHere(start)));
    let wizard = app.model.wizard.as_ref().unwrap();
    assert_eq!(wizard.page(), Page::Steps);
    assert_eq!(wizard.start(), Some(start));
    assert_eq!(app.model.typed_name().as_str(), "a");
    assert_eq!(app.model.nav.tab(), Tab::Tour);
    assert_eq!(wizard_rows(&app), ["a x", "  b x", "    c x"]);
    app.apply(Action::Wizard(WizardAct::Back));
    assert_eq!(app.model.wizard.as_ref().unwrap().page(), Page::Start);
}

#[test]
fn opening_a_row_past_promote_depth_loads_its_callees_and_a_call_back_into_the_path_is_a_leaf() {
    let mut app = deep_app();
    app.apply(Action::Wizard(WizardAct::FromHere(symbol_named(
        &app, "main",
    ))));
    assert_eq!(
        wizard_rows(&app),
        [
            "main x",
            "  a x",
            "    b x",
            "fold Test 2 Closed",
            "fold Trivial 2 Closed"
        ]
    );
    app.apply(Action::Wizard(WizardAct::Expand(branch_named(&app, "b"))));
    app.apply(Action::Wizard(WizardAct::Expand(branch_named(&app, "c"))));
    app.apply(Action::Wizard(WizardAct::Expand(branch_named(&app, "d"))));
    assert_eq!(
        wizard_rows(&app)[2..6],
        ["    b x", "      c x", "        d x", "          a -"]
    );
    let wizard = app.model.wizard.as_ref().unwrap();
    let position = wizard_rows(&app)
        .iter()
        .position(|row| row == "          a -")
        .unwrap();
    let shaped = wizard.rows()[position].clone();
    let wizard::Line::Branch(back) = shaped.line else {
        panic!("a fold where the call back should be");
    };
    assert_eq!(
        wizard.outline().branch(back).unwrap().verdict(),
        Verdict::Cycle
    );
    assert_eq!(shaped.expander, Expander::Leaf);
    app.apply(Action::Wizard(WizardAct::Toggle(back)));
    assert_eq!(
        app.model
            .wizard
            .as_ref()
            .unwrap()
            .outline()
            .branch(back)
            .unwrap()
            .tick,
        Tick::Unticked
    );
}

#[test]
fn unticking_a_wizard_row_unticks_its_subtree_and_ticking_one_ticks_its_parents() {
    let mut app = deep_app();
    app.apply(Action::Wizard(WizardAct::FromHere(symbol_named(
        &app, "main",
    ))));
    app.apply(Action::Wizard(WizardAct::Expand(branch_named(&app, "b"))));
    app.apply(Action::Wizard(WizardAct::Expand(branch_named(&app, "c"))));
    app.apply(Action::Wizard(WizardAct::Toggle(branch_named(&app, "a"))));
    assert_eq!(
        wizard_rows(&app)[..5],
        ["main x", "  a -", "    b -", "      c -", "        d -"]
    );
    app.apply(Action::Wizard(WizardAct::Toggle(branch_named(&app, "c"))));
    assert_eq!(
        wizard_rows(&app)[..5],
        ["main x", "  a x", "    b x", "      c x", "        d -"]
    );
}

#[test]
fn a_collapsed_row_keeps_its_ticks_and_create_adds_the_hidden_ticked_rows() {
    let mut app = deep_app();
    app.apply(Action::Wizard(WizardAct::FromHere(symbol_named(
        &app, "main",
    ))));
    app.apply(Action::Wizard(WizardAct::Expand(branch_named(&app, "b"))));
    app.apply(Action::Wizard(WizardAct::Expand(branch_named(&app, "c"))));
    app.apply(Action::Wizard(WizardAct::Expand(branch_named(&app, "b"))));
    assert_eq!(wizard_rows(&app)[..3], ["main x", "  a x", "    b x"]);
    assert_eq!(wizard_rows(&app)[3], "fold Test 2 Closed");
    app.apply(Action::Wizard(WizardAct::Next));
    app.apply(Action::Wizard(WizardAct::Next));
    app.apply(Action::Wizard(WizardAct::Next));
    assert!(app.model.wizard.is_none());
    let tour = app.model.map.tour(&TourName::new("main").unwrap()).unwrap();
    let tree: Vec<String> = tour
        .tree_order()
        .into_iter()
        .map(|placed| {
            format!(
                "{}{}",
                "  ".repeat(placed.depth.value() as usize),
                tour.step(&placed.step).unwrap().symbol().unwrap()
            )
        })
        .collect();
    assert_eq!(tree, ["main", "  a", "    b", "      c", "        d"]);
}

#[test]
fn cut_callees_fold_by_reason_and_an_open_fold_lists_each_one_to_tick() {
    let mut app = deep_app();
    let main = symbol_named(&app, "main");
    app.apply(Action::Wizard(WizardAct::FromHere(main)));
    let root = branch_named(&app, "main");
    app.apply(Action::Wizard(WizardAct::Fold(root, Cut::Trivial)));
    assert_eq!(
        wizard_rows(&app)[3..],
        [
            "fold Test 2 Closed",
            "fold Trivial 2 Open",
            "  get_x -",
            "  get_y -"
        ]
    );
    app.apply(Action::Wizard(WizardAct::Toggle(branch_named(
        &app, "get_y",
    ))));
    app.apply(Action::Wizard(WizardAct::Fold(root, Cut::Trivial)));
    assert_eq!(wizard_rows(&app)[4..], ["fold Trivial 2 Closed"]);
    let ticked: Vec<SymbolId> = app
        .model
        .wizard
        .as_ref()
        .unwrap()
        .outline()
        .ticked()
        .into_iter()
        .map(|entry| entry.symbol)
        .collect();
    assert!(ticked.contains(&symbol_named(&app, "get_y")));
    assert!(!ticked.contains(&symbol_named(&app, "get_x")));
}

#[test]
fn opening_a_tour_cancels_the_wizard_and_the_next_wizard_starts_fresh() {
    let mut app = app();
    app.apply(Action::Wizard(WizardAct::Start));
    app.model
        .fields
        .fill(Which::WizardName, &Label::new("half typed"));
    app.apply(Action::OpenTour(TOUR, Tab::Tour));
    assert!(app.model.wizard.is_none());
    assert_eq!(app.model.nav.tour(), Some(TOUR));
    app.apply(Action::Wizard(WizardAct::Start));
    assert_eq!(app.model.wizard.as_ref().unwrap().page(), Page::Name);
    assert_eq!(app.model.typed_name().as_str(), "");
}

fn edit_rows(app: &App) -> Vec<String> {
    let outline = app.model.wizard.as_ref().unwrap().outline();
    outline
        .lines()
        .iter()
        .filter_map(|line| match line {
            wizard::Line::Branch(id) => {
                let branch = outline.branch(*id).unwrap();
                let number = match &branch.state {
                    wizard::StepState::Existing { number, .. } => number.to_string(),
                    wizard::StepState::New => "new".to_owned(),
                };
                Some(format!(
                    "{}{number} {} {}",
                    "  ".repeat(branch.depth().value() as usize),
                    app.model
                        .index
                        .symbol(branch.symbol().unwrap())
                        .unwrap()
                        .name(),
                    if branch.tick == Tick::Ticked {
                        "x"
                    } else {
                        "-"
                    }
                ))
            }
            wizard::Line::Fold(_) => None,
        })
        .collect()
}

#[test]
fn the_edit_outline_starts_ticked_from_the_tour_opens_calls_it_lacks_and_unticking_removes() {
    let mut app = deep_app();
    let walk = drafted(&mut app, "walk", &[("main", 0), ("a", 1), ("b", 2)]);
    let step_of = |shown: &App, name: &str| {
        let symbol = symbol_named(shown, name);
        shown
            .model
            .map
            .tour(&walk)
            .unwrap()
            .steps()
            .iter()
            .find(|step| step.resolved_symbol() == Some(symbol))
            .unwrap()
            .id()
            .clone()
    };
    let (first, second) = (step_of(&app, "a"), step_of(&app, "b"));
    let _noted = app
        .model
        .map
        .set_step_note(&walk, &first, Note::new("why a"))
        .unwrap();
    app.apply(Action::Wizard(WizardAct::Edit(TourSlot::new(0))));
    assert_eq!(edit_rows(&app), ["1 main x", "  1.1 a x", "    1.1.1 b x"]);
    assert!(app.model.wizard.as_ref().unwrap().changes().is_empty());
    app.apply(Action::Wizard(WizardAct::More(branch_named(&app, "main"))));
    let outline = app.model.wizard.as_ref().unwrap().outline();
    let added: Vec<Tick> = outline
        .ids()
        .filter_map(|id| outline.branch(id))
        .filter(|branch| branch.state == wizard::StepState::New)
        .map(|branch| branch.tick)
        .collect();
    assert_eq!(added, [Tick::Unticked; 4]);
    app.apply(Action::Wizard(WizardAct::Expand(branch_named(&app, "b"))));
    assert_eq!(edit_rows(&app)[2..], ["    1.1.1 b x", "      new c x"]);
    app.apply(Action::Wizard(WizardAct::Toggle(branch_named(&app, "a"))));
    assert_eq!(
        edit_rows(&app),
        ["1 main x", "  1.1 a -", "    1.1.1 b -", "      new c -"]
    );
    let changes = app.model.wizard.as_ref().unwrap().changes();
    assert_eq!(
        changes,
        [
            EditChange::Removed {
                step: first,
                lost: Note::new("why a"),
            },
            EditChange::Removed {
                step: second,
                lost: None,
            },
        ]
    );
}

fn drafted(app: &mut App, name: &str, steps: &[(&str, u32)]) -> TourName {
    let tour = TourName::new(name).unwrap();
    let entries: Vec<TreeEntry> = steps
        .iter()
        .map(|(symbol, depth)| TreeEntry {
            symbol: symbol_named(app, symbol),
            depth: Depth::new(*depth),
        })
        .collect();
    let draft = Draft {
        name: tour.clone(),
        kind: TourKind::Flow,
        group: None,
        note: None,
        author: Author::Agent,
    };
    let index = app.model.index.clone();
    let _created = app
        .model
        .map
        .add_drafted_tour(&index, draft, &entries)
        .unwrap();
    tour
}

fn editing_walk() -> App {
    let mut app = deep_app();
    let _walk = drafted(&mut app, "walk", &[("main", 0), ("a", 1), ("b", 2)]);
    app.apply(Action::Wizard(WizardAct::Edit(TourSlot::new(0))));
    app
}

fn press(app: &mut App, key: Key) {
    let mut input = Input::default();
    input.keys.push(Press {
        key,
        mods: Mods::NONE,
    });
    app.keys(&input);
}

fn type_text(app: &mut App, text: &str) {
    let mut input = Input::default();
    input.typed.push_str(text);
    app.keys(&input);
}

fn change_count(app: &App) -> usize {
    app.model.wizard.as_ref().unwrap().changes().len()
}

#[test]
fn escape_in_a_step_note_releases_the_field_and_keeps_the_edit_open() {
    let mut app = editing_walk();
    app.model
        .fields
        .focus(Which::StepNote(branch_named(&app, "a")));
    press(&mut app, Key::Escape);
    assert_eq!(app.model.fields.focused(), None);
    assert!(app.model.wizard.is_some());
}

#[test]
fn escape_with_changes_pending_keeps_them_and_says_cancel_discards_them() {
    let mut app = editing_walk();
    app.model
        .fields
        .focus(Which::StepNote(branch_named(&app, "a")));
    type_text(&mut app, "why a");
    press(&mut app, Key::Escape);
    press(&mut app, Key::Escape);
    assert!(app.model.wizard.is_some());
    assert_eq!(
        app.model.status,
        Status::WizardHeld(Held::Changes(Count::new(1)))
    );
    assert_eq!(
        app.model.status.to_string(),
        "1 change pending: press cancel to discard it"
    );
    app.apply(Action::Wizard(WizardAct::Cancel));
    assert!(app.model.wizard.is_none());
}

#[test]
fn escape_with_nothing_pending_cancels_the_edit() {
    let mut app = editing_walk();
    press(&mut app, Key::Escape);
    assert!(app.model.wizard.is_none());
}

#[test]
fn escape_cancels_the_build_wizard_only_while_nothing_is_entered() {
    let mut app = deep_app();
    app.apply(Action::Wizard(WizardAct::Start));
    assert_eq!(app.model.fields.focused(), Some(Which::WizardName));
    press(&mut app, Key::Escape);
    assert!(app.model.wizard.is_some());
    press(&mut app, Key::Escape);
    assert!(app.model.wizard.is_none());
    app.apply(Action::Wizard(WizardAct::Start));
    type_text(&mut app, "walk");
    press(&mut app, Key::Escape);
    press(&mut app, Key::Escape);
    assert!(app.model.wizard.is_some());
    assert_eq!(app.model.status, Status::WizardHeld(Held::Unbuilt));
}

#[test]
fn unticking_a_promoted_row_counts_as_entered_for_escape() {
    let mut app = deep_app();
    app.apply(Action::Wizard(WizardAct::FromHere(symbol_named(
        &app, "main",
    ))));
    app.model.fields.release(Which::WizardName);
    app.apply(Action::Wizard(WizardAct::Toggle(branch_named(&app, "b"))));
    press(&mut app, Key::Escape);
    assert_eq!(app.model.status, Status::WizardHeld(Held::Unbuilt));
    app.apply(Action::Wizard(WizardAct::Toggle(branch_named(&app, "b"))));
    press(&mut app, Key::Escape);
    assert!(app.model.wizard.is_none());
}

#[test]
fn a_call_opened_under_a_kept_step_ticks_by_the_build_rule_even_when_another_tour_has_it() {
    let mut app = deep_app();
    let walk = drafted(&mut app, "walk", &[("main", 0), ("a", 1)]);
    let _other = drafted(&mut app, "other", &[("b", 0)]);
    let index = app.model.index.clone();
    let file = index.find_file(&RelativePath::new("src/main.rs")).unwrap();
    let a_step = {
        let symbol = symbol_named(&app, "a");
        app.model
            .map
            .tour(&walk)
            .unwrap()
            .steps()
            .iter()
            .find(|step| step.resolved_symbol() == Some(symbol))
            .unwrap()
            .id()
            .clone()
    };
    let _lines = app
        .model
        .map
        .add_step(
            &index,
            &walk,
            file,
            Span::new(Line::new(13), Line::new(18)).unwrap(),
            Author::Agent,
            Some(&a_step),
        )
        .unwrap();
    let slot = app.model.find_tour(&walk).unwrap();
    app.apply(Action::Wizard(WizardAct::Edit(slot)));
    app.apply(Action::Wizard(WizardAct::More(branch_named(&app, "a"))));
    let first = app.model.wizard.as_ref().unwrap().outline();
    let mapped = first.branch(branch_named(&app, "b")).unwrap();
    assert_eq!(mapped.verdict(), Verdict::Stopped(Stop::Mapped));
    assert_eq!(mapped.tick, Tick::Ticked);
    app.apply(Action::Wizard(WizardAct::More(branch_named(&app, "b"))));
    let second = app.model.wizard.as_ref().unwrap().outline();
    let covered_by_the_edited_tour = second.branch(branch_named(&app, "c")).unwrap();
    assert_eq!(covered_by_the_edited_tour.verdict(), Verdict::Kept);
    app.apply(Action::Wizard(WizardAct::Toggle(branch_named(&app, "a"))));
    let unticked = app.model.wizard.as_ref().unwrap().outline();
    let below = unticked.branch(branch_named(&app, "c")).unwrap();
    assert_eq!(below.tick, Tick::Unticked);
}

#[test]
fn the_pending_edit_is_recounted_after_a_typed_note_and_after_a_kind_change() {
    let mut app = editing_walk();
    assert_eq!(change_count(&app), 0);
    app.model
        .fields
        .focus(Which::StepNote(branch_named(&app, "a")));
    type_text(&mut app, "why a");
    assert_eq!(change_count(&app), 1);
    app.apply(Action::Wizard(WizardAct::Kind(TourKind::Layer)));
    assert_eq!(change_count(&app), 2);
}

fn press_with(app: &mut App, key: Key, mods: Mods) {
    let mut input = Input::default();
    input.keys.push(Press { key, mods });
    app.keys(&input);
}

#[test]
fn ctrl_enter_applies_the_edit_and_plain_enter_in_a_step_note_does_not() {
    let mut app = editing_walk();
    app.model
        .fields
        .focus(Which::StepNote(branch_named(&app, "a")));
    type_text(&mut app, "why a");
    press(&mut app, Key::Enter);
    assert!(app.model.wizard.is_some());
    assert_eq!(change_count(&app), 1);
    let walk = TourName::new("walk").unwrap();
    assert_eq!(
        app.model
            .map
            .tour(&walk)
            .unwrap()
            .steps()
            .iter()
            .filter(|step| step.note().is_some())
            .count(),
        0
    );
    press_with(&mut app, Key::Enter, Mods::CTRL);
    assert!(app.model.wizard.is_none());
    assert_eq!(
        app.model
            .map
            .tour(&walk)
            .unwrap()
            .steps()
            .iter()
            .filter(|step| step.note().is_some())
            .count(),
        1
    );
}

#[test]
fn a_field_without_the_keyboard_shows_the_start_of_its_text_cut_with_an_ellipsis() {
    let mut fields = Fields::default();
    fields.fill(Which::WizardNote, &Label::new("abcdefghij"));
    let field = fields.get(Which::WizardNote);
    let room = FieldRoom::Cells(Count::new(5));
    let idle = field.shown(Attention::Idle, room);
    assert_eq!(
        (idle.before.as_str(), idle.after.as_str()),
        ("abcd\u{2026}", "")
    );
    let focused = field.shown(Attention::Focused, room);
    assert_eq!(
        (focused.before.as_str(), focused.after.as_str()),
        ("abcdefghij", "")
    );
}
