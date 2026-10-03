use domain::{FontFamily, Theme};
use platform::BUNDLED_FAMILY;
use strum::VariantArray;
use ui::{Button, Count, Label, Point, Px, Run};

use crate::action::Action;
use crate::ids;
use crate::model::Model;
use crate::panels::Direction;
use crate::settings::{GraphDirectionName, SettingsAct, ThemeName};
use crate::theme::{
    PANEL_PADDING, ROW_EXTRA, SETTINGS_FONT_ROWS, SETTINGS_HEADING, SETTINGS_WIDTH, TEXT,
    TOOLTIP_PADDING, WEAK,
};
use crate::widgets::{Chosen, Container, Frame, Scroller};

fn heading(frame: &mut Frame<'_>, text: &Label) {
    let width = usize::try_from(SETTINGS_HEADING.get()).unwrap_or(0);
    frame.label(format!("{:<width$}", text.as_str()), WEAK);
}

pub(super) fn settings(model: &Model, frame: &mut Frame<'_>) {
    if model.settings_menu.is_none() {
        return;
    }
    let settings = &model.settings;
    let window = frame.ui.size();
    let width = SETTINGS_WIDTH.of(frame.cell_width()) + TOOLTIP_PADDING * 2;
    let at = Point::new(
        ((window.width - width) / 2).max(Px::ZERO),
        window.height / 10,
    );
    frame.start(Container::Settings { at, width });

    frame.start(Container::ToolbarTight);
    frame.title("settings");
    frame.grow();
    if frame.control("close", ids::SETTINGS_CLOSE).clicked() {
        frame.push(Action::Settings(SettingsAct::Close));
    }
    frame.finish();

    frame.start(Container::ToolbarTight);
    heading(frame, &Label::new("theme"));
    for theme in Theme::VARIANTS {
        let word = ThemeName::new(*theme).to_string();
        if frame
            .button(
                word.as_str(),
                ids::SETTINGS_THEME.with(&Label::new(&word)),
                Chosen::of(theme, &settings.theme()),
            )
            .clicked()
        {
            frame.push(Action::Settings(SettingsAct::Theme(*theme)));
        }
    }
    frame.finish();

    frame.start(Container::ToolbarTight);
    heading(frame, &Label::new("font size"));
    let size = settings.size();
    if frame.control("smaller", ids::SETTINGS_SMALLER).clicked() {
        frame.push(Action::Settings(SettingsAct::Size(size.smaller())));
    }
    frame.label(size.get().to_string(), TEXT);
    if frame.control("larger", ids::SETTINGS_LARGER).clicked() {
        frame.push(Action::Settings(SettingsAct::Size(size.larger())));
    }
    frame.finish();

    frame.start(Container::ToolbarTight);
    heading(frame, &Label::new("graph starts"));
    let graph = Direction::of_saved(settings.graph_direction());
    for direction in [Direction::Right, Direction::Down] {
        if frame
            .button(
                GraphDirectionName::new(direction).to_string(),
                ids::SETTINGS_GRAPH.with(&Label::new(direction.to_string())),
                Chosen::of(&direction, &graph),
            )
            .clicked()
        {
            frame.push(Action::Settings(SettingsAct::Graph(direction)));
        }
    }
    frame.finish();

    frame.start(Container::FillRow);
    heading(frame, &Label::new("font"));
    fonts(model, frame);
    frame.finish();

    frame.label("esc or ctrl+, to close", WEAK);
    let shown = frame.ui.interaction(ids::settings_box()).rect();
    frame.finish();

    let pointer = frame.ui.pointer();
    if pointer.pressed.contains(Button::Left)
        && shown.is_some_and(|rect| !rect.contains(pointer.mouse))
    {
        frame.push(Action::Settings(SettingsAct::Close));
    }
}

fn fonts(model: &Model, frame: &mut Frame<'_>) {
    let installed: Vec<&FontFamily> = model
        .fonts
        .as_ref()
        .map(|fonts| fonts.families().collect())
        .unwrap_or_default();
    let current = model.settings.font();
    let rows = (installed.len() + 1).min(SETTINGS_FONT_ROWS.get());
    let row = frame.row_height() + ROW_EXTRA;
    let height = Px::of_count(rows) * row.get() + PANEL_PADDING * 2;
    let id = ids::SETTINGS_FONT.id();
    frame.scroll_column(id, model.scrolls.get(id), Scroller::Sized { height }, None);
    if frame
        .row(
            vec![
                Run::new(BUNDLED_FAMILY.as_str(), TEXT),
                Run::new(" (bundled)", WEAK),
            ],
            ids::SETTINGS_FONT.nth(Count::ZERO),
            Chosen::of(&None, &current),
        )
        .clicked()
    {
        frame.push(Action::Settings(SettingsAct::Font(None)));
    }
    for (slot, family) in installed.into_iter().enumerate() {
        if frame
            .row(
                vec![Run::new(family.as_str(), TEXT)],
                ids::SETTINGS_FONT.nth(Count::new(slot + 1)),
                Chosen::of(&Some(family), &current),
            )
            .clicked()
        {
            frame.push(Action::Settings(SettingsAct::Font(Some(family.clone()))));
        }
    }
    frame.finish();
}
