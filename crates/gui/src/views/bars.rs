use ui::Label;

use crate::action::Action;
use crate::field::Which;
use crate::ids;
use crate::model::{Dirty, Model, Tab};
use crate::theme::{NEW_PATH_FIELD, SEARCH_FIELD, TEXT, WEAK};
use crate::widgets::{Chosen, Container, Enabled, Frame};

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
        .nav_button("<", ids::BACK.target(), enabled(model.nav.can_go_back()))
        .clicked()
    {
        frame.push(Action::Back);
    }
    if frame
        .nav_button(
            ">",
            ids::FORWARD.target(),
            enabled(model.nav.can_go_forward()),
        )
        .clicked()
    {
        frame.push(Action::Forward);
    }
    let results = format!("Results ({})", model.results.len());
    for (tab, name) in [
        (Tab::Path, "Path"),
        (Tab::Diff, "Diff"),
        (Tab::Graph, "Graph"),
        (Tab::Listing, "Listing"),
        (Tab::Results, results.as_str()),
    ] {
        let id = ids::TAB.with(&Label::new(name.split(' ').next().unwrap_or(name)));
        let chosen = if model.nav.tab() == tab {
            Chosen::Chosen
        } else {
            Chosen::Plain
        };
        if frame.button(name, id, chosen).clicked() {
            frame.push(Action::Tab(tab));
        }
    }
    frame.grow();
    frame.label("new path", WEAK);
    frame.field(
        &model.fields,
        Which::NewPath,
        &Label::new("name"),
        NEW_PATH_FIELD,
    );
    if frame.control("pin selection", ids::PIN).clicked() {
        frame.push(Action::PinSelection);
    }
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
