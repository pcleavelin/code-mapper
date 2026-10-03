use platform::Cursor;
use ui::{Button, Extent, Icon, Label, Point, Rect, Run, Ui};

use crate::action::Action;
use crate::field::Which;
use crate::graph::GraphFrame;
use crate::ids::{self, Target};
use crate::model::{HitsShown, Model};
use crate::panels::{
    self, Branch, BranchId, Direction, DropTarget, Grab, Panel, Panels, Split, View,
};
use crate::theme::{PANEL_LEAST_ACROSS, PANEL_LEAST_DOWN, PICKER_FIELD, PICKER_WIDTH, TEXT, WEAK};
use crate::views::{center, console, document, left, references, welcome};
use crate::widgets::{Chosen, Container, Frame};

fn panel_id(panel: BranchId) -> ui::Id {
    ids::panel().nth(panel.number())
}

fn pair_id(split: BranchId) -> ui::Id {
    ids::pair().nth(split.number())
}

fn divider_target(split: BranchId) -> Target {
    ids::DIVIDER.nth(ui::Count::new(split.number()))
}

fn tab_target(view: View) -> Target {
    ids::TAB.with(&Label::new(view.name().as_str()))
}

fn numbered(control: ids::Control, panel: BranchId) -> Target {
    control.nth(ui::Count::new(panel.number()))
}

fn drop_target(model: &Model, ui: &Ui) -> Option<DropTarget> {
    model.panels.drop_target(ui.pointer().mouse, |panel| {
        ui.interaction(panel_id(panel)).rect()
    })
}

pub(crate) fn panel_input(model: &Model, ui: &Ui) -> Vec<Action> {
    let mut actions = Vec::new();
    let divider = panels::divider_width(model.metrics.font);
    let mouse = ui.pointer().mouse;
    for split in model.panels.splits() {
        if !ui.interaction(divider_target(split.id()).id()).down() {
            continue;
        }
        if let Some(rect) = ui.interaction(pair_id(split.id())).rect() {
            actions.push(Action::Resize(
                split.id(),
                split.ratio_at(rect, divider, mouse),
            ));
        }
    }
    let Some(_) = model.panels.grab() else {
        return actions;
    };
    actions.push(Action::DragView(mouse));
    if !ui.pointer().down.contains(Button::Left) {
        let mut moved = Panels::clone(&model.panels);
        moved.drag_to(mouse);
        let moving = moved.grab().is_some_and(Grab::is_moving);
        let target = drop_target(model, ui).filter(|_| moving);
        actions.push(Action::Release(target));
    }
    actions
}

fn least(model: &Model) -> Extent {
    let cell = model.metrics.cell;
    Extent::new(
        PANEL_LEAST_ACROSS.of(cell.width),
        PANEL_LEAST_DOWN.of(cell.height),
    )
}

pub(super) fn tree(model: &Model, frame: &mut Frame<'_>, graph: &mut Option<GraphFrame>) {
    let body = frame
        .ui
        .interaction(ids::body())
        .rect()
        .unwrap_or_else(|| Rect::at(Point::default(), frame.ui.size()));
    node(model, frame, model.panels.root(), body, graph);
}

fn node(
    model: &Model,
    frame: &mut Frame<'_>,
    node: &Branch,
    rect: Rect,
    graph: &mut Option<GraphFrame>,
) {
    match node {
        Branch::Split(split) => pair(model, frame, split, rect, graph),
        Branch::Panel(panel) => panel_box(model, frame, panel, rect, graph),
    }
}

fn pair(
    model: &Model,
    frame: &mut Frame<'_>,
    split: &Split,
    rect: Rect,
    graph: &mut Option<GraphFrame>,
) {
    let thickness = panels::divider_width(model.metrics.font);
    let divided = split.divide(rect, thickness, least(model));
    frame.pane(pair_id(split.id()), Some(split.direction()), rect);
    node(model, frame, split.first(), divided.first, graph);
    divider_bar(frame, split, divided.divider);
    node(model, frame, split.second(), divided.second, graph);
    frame.finish();
}

fn divider_bar(frame: &mut Frame<'_>, split: &Split, rect: Rect) {
    let target = divider_target(split.id());
    let interaction = frame.ui.interaction(target.id());
    let left_down = frame.ui.pointer().down.contains(Button::Left);
    if interaction.down() || (interaction.hovered() && !left_down) {
        frame.cursor = match split.direction() {
            Direction::Right => Cursor::ColumnResize,
            Direction::Down => Cursor::RowResize,
        };
    }
    frame.divider(target, split.direction(), rect);
}

fn panel_box(
    model: &Model,
    frame: &mut Frame<'_>,
    panel: &Panel,
    rect: Rect,
    graph: &mut Option<GraphFrame>,
) {
    let bare = model.welcome.fills_panel() && panel.views().is_empty();
    frame.pane(panel_id(panel.id()), None, rect);
    if !bare {
        header(model, frame, panel);
    }
    frame.start(Container::Center);
    match panel.active() {
        Some(view) => body(model, frame, view, rect.extent(), graph),
        None if bare => welcome::surface(model, frame),
        None => frame.label("no view here: press + to choose one", WEAK),
    }
    frame.finish();
    frame.finish();
}

fn header(model: &Model, frame: &mut Frame<'_>, panel: &Panel) {
    frame.start(Container::PanelHeader);
    for view in panel.views() {
        let chosen = if panel.active() == Some(*view) {
            Chosen::Chosen
        } else {
            Chosen::Plain
        };
        tab(model, frame, *view, chosen);
    }
    frame.start(Container::ToolbarSmall);
    frame.grow();
    let id = panel.id();
    if frame
        .small_button(Icon::Add, numbered(ids::PICK, id))
        .clicked()
    {
        frame.push(Action::TogglePicker(id));
    }
    if frame
        .small_button(Icon::SplitRight, numbered(ids::SPLIT_RIGHT, id))
        .clicked()
    {
        frame.push(Action::SplitPanel(id, Direction::Right));
    }
    if frame
        .small_button(Icon::SplitDown, numbered(ids::SPLIT_DOWN, id))
        .clicked()
    {
        frame.push(Action::SplitPanel(id, Direction::Down));
    }
    if !model.panels.is_single()
        && frame
            .small_button(Icon::Close, numbered(ids::CLOSE_PANEL, id))
            .clicked()
    {
        frame.push(Action::ClosePanel(id));
    }
    frame.finish();
    frame.finish();
}

fn tab(model: &Model, frame: &mut Frame<'_>, view: View, chosen: Chosen) {
    let target = tab_target(view);
    let label = if view == View::Search {
        let more = if model.hits_shown == HitsShown::First {
            "+"
        } else {
            ""
        };
        format!("{} ({}{more})", view.name(), model.hits.len())
    } else {
        view.name().to_string()
    };
    let close = ids::CLOSE_TAB.with(&Label::new(view.name().as_str()));
    let clicks = frame.tab(label, target, close, chosen);
    if clicks.close.clicked() {
        frame.push(Action::CloseView(view));
    } else if clicks.tab.clicked() {
        let mouse = frame.ui.pointer().mouse;
        frame.push(Action::ShowView(view));
        frame.push(Action::Grab(view, mouse));
    }
    if model
        .panels
        .grab()
        .is_some_and(|grab| grab.view == view && grab.is_moving())
    {
        frame.cursor = Cursor::Grabbing;
    }
}

fn body(
    model: &Model,
    frame: &mut Frame<'_>,
    view: View,
    area: Extent,
    graph: &mut Option<GraphFrame>,
) {
    match view {
        View::Tours | View::Symbols | View::Files => {
            frame.start(Container::PanelColumn);
            match view {
                View::Symbols => left::symbols_window(model, frame),
                View::Files => left::files_window(model, frame),
                _ => left::tours_window(model, frame),
            }
            frame.finish();
        }
        View::Tour => document::tour_document(model, frame),
        View::Diff => center::diff_view(model, frame),
        View::Graph => center::graph_tab(model, frame, graph.take()),
        View::Source => center::source(model, frame),
        View::Search => center::search_view(model, frame),
        View::References => references::references_panel(model, frame, area),
        View::Console => console::console_panel(model, frame),
    }
}

pub(super) fn drag_band(model: &Model, frame: &mut Frame<'_>) {
    if !model.panels.grab().is_some_and(Grab::is_moving) {
        return;
    }
    if let Some(target) = drop_target(model, frame.ui) {
        frame.drop_band(target.band);
    }
}

pub(super) fn picker(model: &Model, frame: &mut Frame<'_>) {
    let Some(panel) = model.panels.picker() else {
        return;
    };
    let at = frame
        .ui
        .interaction(numbered(ids::PICK, panel).id())
        .rect()
        .map(|button| Point::new(button.left, button.bottom()))
        .or_else(|| {
            frame
                .ui
                .interaction(panel_id(panel))
                .rect()
                .map(Rect::origin)
        });
    let Some(at) = at else {
        return;
    };
    let width = PICKER_WIDTH.of(frame.cell_width());
    let window = frame.ui.size();
    let at = Point::new(
        at.horizontal.min(window.width - width).max(ui::Px::ZERO),
        at.vertical,
    );
    frame.start(Container::Picker { at, width });
    frame.field(
        &model.fields,
        Which::ViewSearch,
        &Label::new("view"),
        PICKER_FIELD,
    );
    let search = model.fields.get(Which::ViewSearch).text();
    for view in View::matching(&search.label()) {
        let place = match model.panels.holder(view) {
            Some(holder) if holder == panel => "  here",
            Some(_) => "  moves here",
            None => "",
        };
        let runs = vec![
            Run::new(view.name().to_string(), TEXT),
            Run::new(place, WEAK),
        ];
        if frame
            .row(runs, ids::VIEW_ROW.nth(view.position()), Chosen::Plain)
            .clicked()
        {
            frame.push(Action::Pick(panel, view));
        }
    }
    frame.finish();
}
