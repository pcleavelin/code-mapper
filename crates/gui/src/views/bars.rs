use ui::{Icon, Label, Run};

use crate::action::Action;
use crate::field::Which;
use crate::ids;
use crate::model::{Dirty, Model};
use crate::settings::SettingsAct;
use crate::status::Tone;
use crate::text::Clipped;
use crate::theme::{GREEN, MAP_PLACE, ORANGE, RED, SEARCH_FIELD, TEXT, WEAK};
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
        .nav_button(
            Icon::Back,
            ids::BACK.target(),
            enabled(model.nav.can_go_back()),
        )
        .clicked()
    {
        frame.push(Action::Back);
    }
    frame.attach_tip(ids::BACK.target());
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
    frame.attach_tip(ids::FORWARD.target());
    frame.grow();
    let unsaved = model.disk.dirty == Dirty::Unsaved;
    if unsaved {
        frame.label("unsaved changes", ORANGE);
    }
    let (save, chosen) = if unsaved {
        ("save *", Chosen::Chosen)
    } else {
        ("save", Chosen::Plain)
    };
    if frame.button(save, ids::SAVE.target(), chosen).clicked() {
        frame.push(Action::Save);
    }
    frame.attach_tip(ids::SAVE.target());
    let open = if model.settings_menu.is_some() {
        Chosen::Chosen
    } else {
        Chosen::Plain
    };
    if frame
        .button(Icon::Settings, ids::SETTINGS_OPEN.target(), open)
        .clicked()
    {
        frame.push(Action::Settings(SettingsAct::Toggle));
    }
    frame.attach_tip(ids::SETTINGS_OPEN.target());
    frame.finish();
}

pub(super) fn status_bar(model: &Model, frame: &mut Frame<'_>) {
    frame.start(Container::StatusBar);
    let status = frame
        .overlay
        .status
        .clone()
        .unwrap_or_else(|| model.status.clone());
    let color = match status.tone() {
        Tone::Plain => TEXT,
        Tone::Done => GREEN,
        Tone::Warning => ORANGE,
        Tone::Problem => RED,
    };
    frame.caption(vec![Run::new(status.line(), color)]);
    let progress = model.work.progress();
    if !progress.as_str().is_empty() {
        frame.label(progress.clone(), WEAK);
    }
    let place = model
        .store
        .directory()
        .display()
        .to_string()
        .replace('\\', "/");
    let room = usize::try_from(MAP_PLACE.get()).unwrap_or(0);
    frame.label(format!("map: {}", Clipped::left(&place, room)), WEAK);
    frame.finish();
}
