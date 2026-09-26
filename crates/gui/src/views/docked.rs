use platform::Cursor;
use ui::{Button, Extent, Label, Px, Ui};

use crate::action::Action;
use crate::dock::{self, Dock, DropTarget, Edge, Frames, Panel};
use crate::ids::{self, Target};
use crate::model::Model;
use crate::views::{left, output, xrefs};
use crate::widgets::{Chosen, Frame};

fn panel_id(panel: Panel) -> ui::Id {
    ids::panel().with(panel.name().as_str())
}

fn split_target(panel: Panel) -> Target {
    ids::SPLIT.with(&Label::new(panel.name().as_str()))
}

fn grip_target(panel: Panel) -> Target {
    ids::GRIP.with(&Label::new(panel.name().as_str()))
}

fn drop_target(model: &Model, ui: &Ui, panel: Panel) -> Option<DropTarget> {
    let body = ui.interaction(ids::body()).rect()?;
    let row = ui.interaction(ids::dock_row()).rect()?;
    model.dock.drop_target(
        panel,
        ui.pointer().mouse,
        Frames { body, row },
        |other| ui.interaction(panel_id(other)).rect(),
        ui.size(),
        model.metrics.cell,
    )
}

pub(crate) fn dock_input(model: &Model, ui: &Ui) -> Vec<Action> {
    let mut actions = Vec::new();
    let split = dock::split_width(model.metrics.font);
    let mouse = ui.pointer().mouse;
    for place in model.dock.places() {
        if !ui.interaction(split_target(place.panel).id()).down() {
            continue;
        }
        if let Some(rect) = ui.interaction(panel_id(place.panel)).rect() {
            let size = match place.edge {
                Edge::Left => mouse.horizontal - rect.left - split / 2,
                Edge::Right => rect.right() - mouse.horizontal - split / 2,
                Edge::Bottom => rect.bottom() - mouse.vertical - split / 2,
            };
            actions.push(Action::Resize(place.panel, size));
        }
    }
    let Some(grab) = model.dock.grab() else {
        return actions;
    };
    actions.push(Action::DragDock(mouse));
    if !ui.pointer().down.contains(Button::Left) {
        let mut moved = Dock::clone(&model.dock);
        moved.drag_to(mouse);
        let moving = moved.grab().is_some_and(dock::Grab::is_moving);
        let target = drop_target(model, ui, grab.panel).filter(|_| moving);
        actions.push(Action::Release(target));
    }
    actions
}

pub(super) fn docked(model: &Model, frame: &mut Frame<'_>, edge: Edge, sizes: &[Px]) {
    let split = dock::split_width(model.metrics.font);
    for (place, size) in model.dock.places().iter().zip(sizes) {
        if place.edge != edge {
            continue;
        }
        frame.docked(panel_id(place.panel), edge, *size + split);
        if edge != Edge::Left {
            splitter(model, frame, place.panel, edge);
        }
        match place.panel {
            Panel::Nav => left::left_panel(model, frame),
            Panel::Xrefs => {
                let window = frame.ui.size();
                let area = if edge == Edge::Bottom {
                    Extent::new(window.width, *size)
                } else {
                    Extent::new(*size, window.height)
                };
                xrefs::xrefs_panel(model, frame, area);
            }
            Panel::Output => output::output_panel(model, frame),
        }
        if edge == Edge::Left {
            splitter(model, frame, place.panel, edge);
        }
        frame.finish();
    }
}

fn splitter(model: &Model, frame: &mut Frame<'_>, panel: Panel, edge: Edge) {
    let target = split_target(panel);
    let interaction = frame.ui.interaction(target.id());
    let left_down = frame.ui.pointer().down.contains(Button::Left);
    if interaction.down() || (interaction.hovered() && !left_down) {
        frame.cursor = if edge == Edge::Bottom {
            Cursor::RowResize
        } else {
            Cursor::ColumnResize
        };
    }
    frame.splitter(target, edge, dock::split_width(model.metrics.font));
}

pub(crate) fn grip(model: &Model, frame: &mut Frame<'_>, panel: Panel) {
    let target = grip_target(panel);
    let held = model.dock.grab().is_some_and(|grab| grab.panel == panel);
    let interaction = frame.ui.interaction(target.id());
    if interaction.clicked() {
        let mouse = frame.ui.pointer().mouse;
        frame.push(Action::Grab(panel, mouse));
    }
    let left_down = frame.ui.pointer().down.contains(Button::Left);
    if held {
        frame.cursor = Cursor::Grabbing;
    } else if interaction.hovered() && !left_down {
        frame.cursor = Cursor::Grab;
    }
    let lit = if interaction.hovered() || held {
        Chosen::Chosen
    } else {
        Chosen::Plain
    };
    frame.grip(target, lit);
}

pub(super) fn drag_band(model: &Model, frame: &mut Frame<'_>) {
    let Some(grab) = model.dock.grab().filter(|grab| grab.is_moving()) else {
        return;
    };
    if let Some(target) = drop_target(model, frame.ui, grab.panel) {
        frame.drop_band(target.band);
    }
}
