use std::fmt;

use domain::{BaseFontSize, FontFamily, Settings, Theme};
use io_config::SettingsStore;
use io_fonts::Fonts;
use platform::{BUNDLED_FAMILY, Renderer};
use ui::{Label, Px};

use crate::action::Action;
use crate::app::App;
use crate::field::Which;
use crate::graph::GraphAction;
use crate::ids;
use crate::panels::Direction;
use crate::runtime::{Job, Landing, landed};
use crate::status::{FontError, Status};
use crate::theme::{self, ROW_EXTRA, SETTINGS_FONT_ROWS};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SettingsAct {
    Theme(Theme),
    Font(Option<FontFamily>),
    Size(BaseFontSize),
    Graph(Direction),
    Toggle,
    Close,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SettingsMenu;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct InUse {
    theme: Theme,
    font: Option<FontFamily>,
}

#[derive(Default)]
pub(crate) struct KeptSettings {
    store: Option<SettingsStore>,
    saved: Option<Settings>,
    in_use: InUse,
    scan: Option<Job<Fonts>>,
}

impl KeptSettings {
    pub(crate) fn load(store: Option<SettingsStore>) -> Self {
        let saved = store
            .as_ref()
            .and_then(SettingsStore::load)
            .unwrap_or_default();
        Self {
            store,
            saved: Some(saved),
            in_use: InUse::default(),
            scan: None,
        }
    }

    pub(crate) fn chosen(&self) -> Settings {
        self.saved.clone().unwrap_or_default()
    }

    pub(crate) const fn scanning(&self) -> bool {
        self.scan.is_some()
    }
}

impl App {
    pub(crate) fn change_setting(&mut self, act: SettingsAct) {
        let settings = self.model.settings.clone();
        self.model.settings = match act {
            SettingsAct::Theme(theme) => settings.with_theme(theme),
            SettingsAct::Font(font) => settings.with_font(font),
            SettingsAct::Size(size) => settings.with_size(size),
            SettingsAct::Graph(direction) => {
                if self.model.graph.direction() != direction {
                    self.apply(Action::Graph(GraphAction::Turn));
                }
                settings.with_graph_direction(direction.saved())
            }
            SettingsAct::Toggle => {
                if self.model.settings_menu.take().is_none() {
                    self.model.palette = None;
                    self.model.fields.release(Which::Palette);
                    self.model.settings_menu = Some(SettingsMenu);
                    self.list_fonts();
                    self.show_current_font();
                }
                settings
            }
            SettingsAct::Close => {
                self.model.settings_menu = None;
                settings
            }
        };
    }

    fn list_fonts(&mut self) {
        if self.model.fonts.is_none() && self.settings.scan.is_none() {
            self.settings.scan = Some(Job::start(Fonts::scan));
        }
    }

    fn show_current_font(&mut self) {
        let Some(fonts) = &self.model.fonts else {
            return;
        };
        let row = match self.model.settings.font() {
            None => 0,
            Some(current) => fonts
                .families()
                .position(|family| family == current)
                .map_or(0, |found| found + 1),
        };
        let past_view = row.saturating_sub(SETTINGS_FONT_ROWS.get() - 1);
        let row_height = self.model.metrics.row_height() + ROW_EXTRA;
        let offset = Px::of_count(past_view) * row_height.get();
        self.apply(Action::Scroll(ids::SETTINGS_FONT.id(), offset));
    }

    fn poll_fonts(&mut self) {
        match landed(&mut self.settings.scan) {
            Landing::Waiting => {}
            Landing::Landed(fonts) => {
                self.model.fonts = Some(fonts);
                if self.model.settings_menu.is_some() {
                    self.show_current_font();
                }
            }
            Landing::Failed => self.model.fonts = Some(Fonts::default()),
        }
    }

    pub(crate) fn use_settings(&mut self, renderer: &mut Renderer) {
        self.poll_fonts();
        let wanted = InUse {
            theme: self.model.settings.theme(),
            font: self.model.settings.font().cloned(),
        };
        if self.settings.in_use.theme != wanted.theme {
            renderer.use_repaint(theme::repaint(wanted.theme));
            self.settings.in_use.theme = wanted.theme;
        }
        if self.settings.in_use.font == wanted.font {
            return;
        }
        if wanted.font.is_some() && self.model.fonts.is_none() {
            self.list_fonts();
            return;
        }
        if let Err(status) = self.use_font(renderer, wanted.font.as_ref()) {
            renderer.use_font_file(None).unwrap_or_default();
            self.model.status = status;
        }
        self.settings.in_use.font = wanted.font;
    }

    fn use_font(
        &mut self,
        renderer: &mut Renderer,
        font: Option<&FontFamily>,
    ) -> Result<(), Status> {
        let Some(family) = font else {
            return renderer.use_font_file(None).map_err(|reason| {
                Status::FontNotUsed(FontError::Unloadable(
                    Label::new(BUNDLED_FAMILY.as_str()),
                    Label::new(reason.to_string()),
                ))
            });
        };
        let name = Label::new(family.as_str());
        let file = self
            .model
            .fonts
            .as_ref()
            .and_then(|fonts| fonts.read(family))
            .ok_or_else(|| Status::FontNotUsed(FontError::Missing(name.clone())))?;
        renderer.use_font_file(Some(&file)).map_err(|reason| {
            Status::FontNotUsed(FontError::Unloadable(name, Label::new(reason.to_string())))
        })
    }
    pub(crate) fn keep_settings(&mut self) {
        let Some(store) = &self.settings.store else {
            return;
        };
        if self.settings.saved.as_ref() == Some(&self.model.settings) {
            return;
        }
        if let Err(error) = store.save(&self.model.settings) {
            self.model.status = Status::SettingsUnsaved(Label::new(error.to_string()));
        }
        self.settings.saved = Some(self.model.settings.clone());
    }
}

pub(crate) struct ThemeName(Theme);

impl ThemeName {
    pub(crate) const fn new(theme: Theme) -> Self {
        Self(theme)
    }
}

impl fmt::Display for ThemeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            Theme::Dark => "dark",
            Theme::Light => "light",
        })
    }
}

pub(crate) struct GraphDirectionName(Direction);

impl GraphDirectionName {
    pub(crate) const fn new(direction: Direction) -> Self {
        Self(direction)
    }
}

impl fmt::Display for GraphDirectionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            Direction::Right => "left to right",
            Direction::Down => "top to bottom",
        })
    }
}

pub(crate) struct ShownFont<'settings>(&'settings Settings);

impl<'settings> ShownFont<'settings> {
    pub(crate) const fn new(settings: &'settings Settings) -> Self {
        Self(settings)
    }
}

impl fmt::Display for ShownFont<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            self.0
                .font()
                .map_or(BUNDLED_FAMILY.as_str(), FontFamily::as_str),
        )
    }
}

#[cfg(test)]
mod tests;
