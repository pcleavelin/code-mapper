use std::fmt;

use features::{Feature, Trigger};
use strum::VariantArray;
use ui::{Count, Id, Label};

use crate::action::Action;
use crate::app::App;
use crate::ids::{self, Control};
use crate::model::Model;
use crate::palette::{PaletteAction, chord_label};
use crate::panels::{Panels, View};

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
pub(crate) enum Opening {
    Board,
    Workspace,
}

pub(crate) struct Spell(&'static str);

impl Spell {
    pub(crate) const fn as_str(&self) -> &'static str {
        self.0
    }
}

impl Opening {
    const fn word(self) -> Spell {
        Spell(match self {
            Self::Board => "board",
            Self::Workspace => "workspace",
        })
    }

    pub(crate) fn named(text: &Label) -> Self {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|opening| opening.word().as_str() == text.as_str())
            .unwrap_or(Self::Board)
    }

    pub(crate) fn panels(self) -> Option<Panels> {
        match self {
            Self::Board => Some(Panels::bare()),
            Self::Workspace => None,
        }
    }

    pub(crate) fn welcome(self) -> Welcome {
        Welcome::first(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuideStep {
    NewTour,
    NameKind,
    AddStep,
    Target,
    Save,
}

impl GuideStep {
    const fn word(self) -> Spell {
        Spell(match self {
            Self::NewTour => "new-tour",
            Self::NameKind => "name-kind",
            Self::AddStep => "add-step",
            Self::Target => "target",
            Self::Save => "save",
        })
    }

    const fn next(self) -> Option<Self> {
        match self {
            Self::NewTour => Some(Self::NameKind),
            Self::NameKind => Some(Self::AddStep),
            Self::AddStep => Some(Self::Target),
            Self::Target => Some(Self::Save),
            Self::Save => None,
        }
    }

    pub(crate) const fn view(self) -> Option<View> {
        match self {
            Self::NewTour | Self::NameKind | Self::Target => Some(View::Tours),
            Self::AddStep => Some(View::Symbols),
            Self::Save => None,
        }
    }

    pub(crate) fn spotlight(self) -> Id {
        match self {
            Self::NewTour => ids::NEW_TOUR.id(),
            Self::NameKind => ids::NEW_TOUR_FIELD.id(),
            Self::AddStep => ids::TAB
                .with(&Label::new(View::Symbols.name().as_str()))
                .id(),
            Self::Target => ids::STEP_LIST_ROW.nth(Count::ZERO).id(),
            Self::Save => ids::SAVE.id(),
        }
    }

    pub(crate) fn sentence(self) -> Label {
        Label::new(match self {
            Self::NewTour => {
                "Press + new tour in the Tours list. That is the tour you are about to write."
            }
            Self::NameKind => {
                "Type a name, then pick a kind. flow is what happens when X. layer is a boundary and the functions on its surface. data is a structure and what mutates it. The kind stays as you set it. Press create."
            }
            Self::AddStep => {
                "Hover a symbol in Symbols and press + step. The step joins the tour named in the strip."
            }
            Self::Target => {
                "Click that step. The next one hangs under it. Press top level in the strip to place the next one beside it instead."
            }
            Self::Save => {
                "Press save. Closing the window drops unsaved work. A note is still a Console command, tour-note or step-note, and the index it takes is the one the tours command prints, not the 1.1 numbers in this window."
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    Open,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Aim {
    Under,
    Top,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Seen {
    pub(crate) form: Form,
    pub(crate) tours: Count,
    pub(crate) steps: Count,
    pub(crate) aim: Aim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Guide {
    step: GuideStep,
    tours: Count,
    steps: Count,
}

impl Guide {
    const fn start() -> Self {
        Self {
            step: GuideStep::NewTour,
            tours: Count::ZERO,
            steps: Count::ZERO,
        }
    }

    const fn step(self) -> GuideStep {
        self.step
    }

    fn advanced(self, seen: Seen) -> Option<GuideStep> {
        match self.step {
            GuideStep::NewTour if seen.form == Form::Open => Some(GuideStep::NameKind),
            GuideStep::NameKind if seen.tours.get() > self.tours.get() => Some(GuideStep::AddStep),
            GuideStep::AddStep if seen.steps.get() > self.steps.get() => Some(GuideStep::Target),
            GuideStep::Target if seen.aim == Aim::Under => Some(GuideStep::Save),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arrival {
    Board,
    Guide(Guide),
    Workspace,
}

impl fmt::Display for Arrival {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Board => formatter.write_str("board"),
            Self::Guide(guide) => write!(formatter, "guide {}", guide.step().word().as_str()),
            Self::Workspace => formatter.write_str("workspace"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WelcomeAct {
    StartGuide,
    Leave,
    Skip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Welcome {
    arrival: Arrival,
}

impl Welcome {
    pub(crate) const fn workspace() -> Self {
        Self {
            arrival: Arrival::Workspace,
        }
    }

    pub(crate) fn first(opening: Opening) -> Self {
        let arrival = match opening {
            Opening::Board => Arrival::Board,
            Opening::Workspace => Arrival::Workspace,
        };
        Self { arrival }
    }

    pub(crate) const fn arrival(self) -> Arrival {
        self.arrival
    }

    pub(crate) const fn fills_panel(self) -> bool {
        matches!(self.arrival, Arrival::Board)
    }

    pub(crate) const fn guide_step(self) -> Option<GuideStep> {
        match self.arrival {
            Arrival::Guide(guide) => Some(guide.step()),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) const fn guiding() -> Self {
        Self {
            arrival: Arrival::Guide(Guide::start()),
        }
    }

    fn begin_guide(&mut self) {
        self.arrival = Arrival::Guide(Guide::start());
    }

    fn leave(&mut self) {
        self.arrival = Arrival::Workspace;
    }

    fn skip(&mut self, seen: Seen) -> Option<GuideStep> {
        let Arrival::Guide(guide) = self.arrival else {
            return None;
        };
        let step = guide.step();
        if let Some(next) = step.next() {
            self.arrival = Arrival::Guide(Guide {
                step: next,
                tours: seen.tours,
                steps: seen.steps,
            });
            Some(next)
        } else {
            self.leave();
            None
        }
    }

    fn follow(&mut self, seen: Seen) -> Option<GuideStep> {
        let Arrival::Guide(guide) = self.arrival else {
            return None;
        };
        let step = guide.advanced(seen)?;
        self.arrival = Arrival::Guide(Guide {
            step,
            tours: seen.tours,
            steps: seen.steps,
        });
        Some(step)
    }

    pub(crate) fn note_saved(&mut self) -> bool {
        let Arrival::Guide(guide) = self.arrival else {
            return false;
        };
        if guide.step() != GuideStep::Save {
            return false;
        }
        self.leave();
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
pub(crate) enum BoardAction {
    Palette,
    Guide,
    Save,
    Back,
    Workspace,
}

impl BoardAction {
    pub(crate) const fn title(self) -> Spell {
        Spell(match self {
            Self::Palette => "Command palette",
            Self::Guide => "Add a tour",
            Self::Save => "Save the map",
            Self::Back => "Go back",
            Self::Workspace => "Open the window",
        })
    }

    pub(crate) const fn detail(self) -> Spell {
        Spell(match self {
            Self::Palette => "a symbol, a file, a tour, or an action",
            Self::Guide => "each step, on the real controls",
            Self::Save => "write the tours that changed",
            Self::Back => "the previous place",
            Self::Workspace => "the tabs, without the guide",
        })
    }

    pub(crate) fn chord(self) -> Option<Label> {
        match self {
            Self::Palette => palette_chord(Feature::CommandPalette),
            Self::Save => palette_chord(Feature::Save),
            Self::Back => palette_chord(Feature::GoBack),
            Self::Guide | Self::Workspace => None,
        }
    }

    pub(crate) const fn control(self) -> Control {
        match self {
            Self::Palette => ids::WELCOME_PALETTE,
            Self::Guide => ids::WELCOME_GUIDE,
            Self::Save => ids::WELCOME_SAVE,
            Self::Back => ids::WELCOME_BACK,
            Self::Workspace => ids::WELCOME_WORK,
        }
    }

    pub(crate) fn press(self) -> Action {
        match self {
            Self::Palette => Action::Palette(PaletteAction::Toggle),
            Self::Guide => Action::Welcome(WelcomeAct::StartGuide),
            Self::Save => Action::Save,
            Self::Back => Action::Back,
            Self::Workspace => Action::Welcome(WelcomeAct::Leave),
        }
    }
}

impl Model {
    pub(crate) fn follow_welcome(&mut self) {
        let seen = self.welcome_seen();
        if let Some(step) = self.welcome.follow(seen) {
            self.reveal_guide(step);
        }
    }

    pub(crate) fn reveal_guide(&mut self, step: GuideStep) {
        if let Some(view) = step.view() {
            self.show_view(view);
        }
    }

    pub(crate) fn welcome_seen(&self) -> Seen {
        Seen {
            form: if self.new_tour.is_some() {
                Form::Open
            } else {
                Form::Closed
            },
            tours: Count::new(self.map.tours().len()),
            steps: self
                .nav
                .tour()
                .map_or(Count::ZERO, |tour| self.step_count(tour)),
            aim: if self.target_under().is_some() {
                Aim::Under
            } else {
                Aim::Top
            },
        }
    }
}

impl App {
    pub(crate) fn welcome(&mut self, act: WelcomeAct) {
        match act {
            WelcomeAct::StartGuide => {
                self.model.welcome.begin_guide();
                self.open_workspace_panels();
                self.model.reveal_guide(GuideStep::NewTour);
            }
            WelcomeAct::Leave => {
                self.model.welcome.leave();
                self.open_workspace_panels();
            }
            WelcomeAct::Skip => {
                let seen = self.model.welcome_seen();
                match self.model.welcome.skip(seen) {
                    Some(step) if !self.model.panels.is_bare() => self.model.reveal_guide(step),
                    Some(_) => {}
                    None => self.open_workspace_panels(),
                }
            }
        }
    }

    fn open_workspace_panels(&mut self) {
        if self.model.panels.is_bare() {
            self.model.panels = Panels::default();
        }
    }
}

pub(crate) fn palette_chord(feature: Feature) -> Option<Label> {
    feature
        .spec()
        .triggers()
        .iter()
        .find_map(|trigger| match trigger {
            Trigger::Key(chord) => Some(chord_label(*chord)),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(form: Form, tours: usize, steps: usize, aim: Aim) -> Seen {
        Seen {
            form,
            tours: Count::new(tours),
            steps: Count::new(steps),
            aim,
        }
    }

    #[test]
    fn the_guide_advances_only_when_that_step_has_happened() {
        let mut welcome = Welcome::guiding();
        assert_eq!(welcome.guide_step(), Some(GuideStep::NewTour));
        assert_eq!(welcome.follow(seen(Form::Closed, 3, 0, Aim::Top)), None);
        assert_eq!(
            welcome.follow(seen(Form::Open, 3, 0, Aim::Top)),
            Some(GuideStep::NameKind)
        );
        assert_eq!(welcome.follow(seen(Form::Open, 3, 0, Aim::Top)), None);
        assert_eq!(
            welcome.follow(seen(Form::Closed, 4, 0, Aim::Top)),
            Some(GuideStep::AddStep)
        );
        assert_eq!(welcome.follow(seen(Form::Closed, 4, 0, Aim::Top)), None);
        assert_eq!(
            welcome.follow(seen(Form::Closed, 4, 1, Aim::Top)),
            Some(GuideStep::Target)
        );
        assert_eq!(welcome.follow(seen(Form::Closed, 4, 1, Aim::Top)), None);
        assert_eq!(
            welcome.follow(seen(Form::Closed, 4, 1, Aim::Under)),
            Some(GuideStep::Save)
        );
        assert!(welcome.note_saved());
        assert_eq!(welcome.arrival(), Arrival::Workspace);
        assert!(!welcome.note_saved());
    }

    #[test]
    fn saving_before_the_save_step_leaves_the_guide_where_it_is() {
        let mut welcome = Welcome::guiding();
        assert!(!welcome.note_saved());
        assert_eq!(welcome.guide_step(), Some(GuideStep::NewTour));
    }

    #[test]
    fn the_board_is_the_only_arrival_that_fills_the_panel() {
        assert!(Welcome::first(Opening::Board).fills_panel());
        assert!(!Welcome::guiding().fills_panel());
        assert!(!Welcome::workspace().fills_panel());
    }

    #[test]
    fn the_palette_row_shows_the_palette_key() {
        let shown = BoardAction::Palette.chord();
        let bound = palette_chord(Feature::CommandPalette);
        assert_eq!(shown, bound);
        assert_eq!(shown.unwrap().as_str(), "ctrl+p");
    }
}
