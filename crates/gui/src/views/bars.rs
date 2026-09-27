use ui::{Icon, Label};

use crate::action::Action;
use crate::field::Which;
use crate::ids;
use crate::model::{Dirty, Model};
use crate::theme::{SEARCH_FIELD, TEXT, WEAK};
use crate::widgets::{Container, Enabled, Frame};

pub(super) fn top_bar(model: &Model, frame: &mut Frame<'_>) {
    frame.start(Container::TopBar);
    frame.label("search", WEAK);
    frame.field(
        &model.fields,
        Which::Search,
        &Label::new("regex"),
        SEARCH_FIELD,
    );
    let enabled = |can: bool| {
        if can {
            Enabled::Enabled
        } else {
            Enabled::Disabled
        }
    };
    if frame
        .nav_button(
            Icon::Back,
            ids::BACK.target(),
            enabled(model.nav.can_go_back()),
        )
        .clicked()
    {
        frame.push(Action::Back);
    }
    if frame
        .nav_button(
            Icon::Forward,
            ids::FORWARD.target(),
            enabled(model.nav.can_go_forward()),
        )
        .clicked()
    {
        frame.push(Action::Forward);
    }
    frame.grow();
    let save = if model.disk.dirty == Dirty::Unsaved {
        "save *"
    } else {
        "save"
    };
    if frame.control(save, ids::SAVE).clicked() {
        frame.push(Action::Save);
    }
    frame.finish();
}

pub(super) fn status_bar(model: &Model, frame: &mut Frame<'_>) {
    frame.start(Container::StatusBar);
    let place = model
        .store
        .directory()
        .display()
        .to_string()
        .replace('\\', "/");
    frame.label(place, WEAK);
    let status = frame
        .overlay
        .status
        .clone()
        .unwrap_or_else(|| model.status.clone());
    frame.label(status.line(), TEXT);
    let progress = model.work.progress();
    if !progress.as_str().is_empty() {
        frame.label(progress.clone(), WEAK);
    }
    frame.finish();
}
