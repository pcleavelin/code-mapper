use std::path::{self, Component};

use domain::{GroupName, Row, Step, Tour, TourCount};
use features::{Feature, Modifiers, Trigger};
use strum::VariantArray;
use ui::{Count, Label};

use crate::app::App;
use crate::model::{Model, Tab};
use crate::palette::{chord_label, chords_label};
use crate::wizard::WizardAct;

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
pub(crate) enum Opening {
    Start,
    FirstTour,
}

pub(crate) struct Spell(&'static str);

impl Spell {
    pub(crate) const fn new(text: &'static str) -> Self {
        Self(text)
    }

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
pub(crate) enum WelcomeAct {
    BuildTour,
    StartPage,
}

impl App {
    pub(crate) fn welcome(&mut self, act: WelcomeAct) {
        match act {
            WelcomeAct::BuildTour => self.wizard(WizardAct::Start),
            WelcomeAct::StartPage => {
                self.model.close_wizard();
                self.model.show_start_page();
            }
        }
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
        let first = spec.triggers().iter().find_map(|trigger| match trigger {
            Trigger::Key(chord) => Some(*chord),
            _ => None,
        });
        let (Some(first), Some(chord)) = (first, chords_label(*feature)) else {
            continue;
        };
        ranked.push((
            rank(first.modifiers()),
            KeyRow {
                chord,
                meaning: spoken(*feature),
            },
        ));
    }
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked.into_iter().map(|(_, row)| row).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
