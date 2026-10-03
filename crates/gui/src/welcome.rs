use std::fmt;
use std::path::{self, Component};

use domain::{GroupName, Row, Step, Tour, TourCount};
use features::{Feature, Modifiers, Trigger};
use strum::VariantArray;
use ui::{Count, Id, Label};

use crate::app::App;
use crate::ids;
use crate::model::{Model, Tab};
use crate::palette::chord_label;
use crate::panels::View;

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
pub(crate) enum Opening {
    Start,
    FirstTour,
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
            Self::Start => "start",
            Self::FirstTour => "first-tour",
        })
    }

    pub(crate) fn named(text: &Label) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|opening| opening.word().as_str() == text.as_str())
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
            Self::Save => "Press save. Closing the window drops unsaved work.",
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
    Guide(Guide),
    Workspace,
}

impl fmt::Display for Arrival {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
    StartPage,
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

    pub(crate) const fn arrival(self) -> Arrival {
        self.arrival
    }

    pub(crate) const fn guide_step(self) -> Option<GuideStep> {
        match self.arrival {
            Arrival::Guide(guide) => Some(guide.step()),
            Arrival::Workspace => None,
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

pub(crate) struct MapFigures {
    pub(crate) tours: Count,
    pub(crate) covered: Count,
    pub(crate) symbols: Count,
    pub(crate) stale: Count,
}

impl MapFigures {
    pub(crate) const fn uncovered(&self) -> Count {
        Count::new(self.symbols.get().saturating_sub(self.covered.get()))
    }

    pub(crate) fn percent(&self) -> Count {
        Count::new(
            self.covered
                .get()
                .saturating_mul(100)
                .checked_div(self.symbols.get())
                .unwrap_or(0),
        )
    }
}

pub(crate) enum TopGroup {
    Named(GroupName, TourCount),
    Ungrouped(Count),
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

    pub(crate) fn root_name(&self) -> Label {
        let root = self.index.root().as_path();
        let name = path::absolute(root)
            .unwrap_or_else(|_| root.to_path_buf())
            .components()
            .rev()
            .find_map(|component| match component {
                Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            });
        Label::new(name.unwrap_or_else(|| "this repository".to_owned()))
    }

    pub(crate) fn figures(&self) -> MapFigures {
        let coverage = self.map.coverage();
        let (covered, symbols) = self.index.files().fold((0, 0), |(covered, symbols), file| {
            (
                covered
                    + file
                        .symbols()
                        .filter(|symbol| coverage.covers(file.path(), symbol.span()))
                        .count(),
                symbols + file.symbols().count(),
            )
        });
        MapFigures {
            tours: Count::new(self.map.tours().len()),
            covered: Count::new(covered),
            symbols: Count::new(symbols),
            stale: Count::new(
                self.map
                    .tours()
                    .iter()
                    .flat_map(Tour::steps)
                    .filter(|step| Step::is_stale(step))
                    .count(),
            ),
        }
    }

    pub(crate) fn top_groups(&self) -> Vec<TopGroup> {
        let mut groups = Vec::new();
        let mut ungrouped = 0;
        for row in self.map.rows() {
            match row {
                Row::Group {
                    group,
                    depth,
                    tours,
                } if depth.value() == 0 => {
                    groups.push(TopGroup::Named(group, tours));
                }
                Row::Tour { depth, .. } if depth.value() == 0 => ungrouped += 1,
                _ => {}
            }
        }
        if ungrouped > 0 {
            groups.push(TopGroup::Ungrouped(Count::new(ungrouped)));
        }
        groups
    }

    pub(crate) fn show_start_page(&mut self) {
        self.forget_tour();
        self.forget_focus();
        self.set_tab(Tab::Tour);
    }
}

pub(crate) struct KeyRow {
    pub(crate) chord: Label,
    pub(crate) meaning: Label,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum KeyRank {
    Palette,
    Modified,
    Plain,
}

fn spoken(feature: Feature) -> Label {
    let mut words = String::new();
    for letter in format!("{feature:?}").chars() {
        if letter.is_uppercase() && !words.is_empty() {
            words.push(' ');
        }
        words.extend(letter.to_lowercase());
    }
    Label::new(words)
}

pub(crate) fn key_rows() -> Vec<KeyRow> {
    let mut ranked = Vec::new();
    for feature in Feature::VARIANTS {
        let spec = feature.spec();
        let rank = |modifiers: Modifiers| match (feature, modifiers) {
            (Feature::CommandPalette, _) => KeyRank::Palette,
            (_, Modifiers::Plain) => KeyRank::Plain,
            _ => KeyRank::Modified,
        };
        let named: Vec<(KeyRank, KeyRow)> = spec
            .triggers()
            .iter()
            .filter_map(|trigger| match trigger {
                Trigger::Palette(text, Some(chord)) => Some((
                    rank(chord.modifiers()),
                    KeyRow {
                        chord: chord_label(*chord),
                        meaning: Label::new(text.as_str()),
                    },
                )),
                _ => None,
            })
            .collect();
        if !named.is_empty() {
            ranked.extend(named);
            continue;
        }
        let chords: Vec<_> = spec
            .triggers()
            .iter()
            .filter_map(|trigger| match trigger {
                Trigger::Key(chord) => Some(*chord),
                _ => None,
            })
            .collect();
        let Some(first) = chords.first() else {
            continue;
        };
        let spelled: Vec<String> = chords
            .iter()
            .map(|chord| chord_label(*chord).as_str().to_owned())
            .collect();
        ranked.push((
            rank(first.modifiers()),
            KeyRow {
                chord: Label::new(spelled.join("/")),
                meaning: spoken(*feature),
            },
        ));
    }
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().map(|(_, row)| row).collect()
}

impl App {
    pub(crate) fn welcome(&mut self, act: WelcomeAct) {
        match act {
            WelcomeAct::StartGuide => {
                self.model.welcome.begin_guide();
                self.model.reveal_guide(GuideStep::NewTour);
            }
            WelcomeAct::Leave => self.model.welcome.leave(),
            WelcomeAct::Skip => {
                let seen = self.model.welcome_seen();
                if let Some(step) = self.model.welcome.skip(seen) {
                    self.model.reveal_guide(step);
                }
            }
            WelcomeAct::StartPage => self.model.show_start_page(),
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
    fn the_keys_open_with_the_palette_and_list_back_and_forward_once() {
        let rows = key_rows();
        let first = rows.first().unwrap();
        assert_eq!(first.chord.as_str(), "ctrl+p");
        for meaning in ["go back", "go forward"] {
            let found = rows
                .iter()
                .filter(|row| row.meaning.as_str() == meaning)
                .count();
            assert_eq!(found, 1, "{meaning}");
        }
        let mut chords: Vec<&str> = rows.iter().map(|row| row.chord.as_str()).collect();
        chords.sort_unstable();
        chords.dedup();
        assert_eq!(chords.len(), rows.len());
    }

    fn figures(covered: usize, symbols: usize) -> MapFigures {
        MapFigures {
            tours: Count::ZERO,
            covered: Count::new(covered),
            symbols: Count::new(symbols),
            stale: Count::ZERO,
        }
    }

    #[test]
    fn coverage_rounds_down_and_an_empty_index_is_none_covered() {
        assert_eq!(figures(2, 3).percent(), Count::new(66));
        assert_eq!(figures(2, 3).uncovered(), Count::new(1));
        assert_eq!(figures(0, 0).percent(), Count::ZERO);
    }
}
