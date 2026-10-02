use features::{Feature, Gesture, Surface, Trigger};
use strum::VariantArray;
use ui::Label;

const BY_HAND: &[Feature] = &[
    Feature::NewTour,
    Feature::OpenTour,
    Feature::AddStep,
    Feature::ChooseTarget,
    Feature::Save,
    Feature::WalkSteps,
];

const CONSOLE_COMMANDS: &[Feature] = &[
    Feature::TourNote,
    Feature::StepNote,
    Feature::StepLink,
    Feature::Stale,
    Feature::Repin,
];

const TO_FIND: &[Feature] = &[
    Feature::CommandPalette,
    Feature::JumpToDefinition,
    Feature::PeekDefinition,
    Feature::GoBack,
];

pub(crate) fn by_hand() -> &'static [Feature] {
    BY_HAND
}

pub(crate) fn console_commands() -> &'static [Feature] {
    CONSOLE_COMMANDS
}

pub(crate) fn to_find() -> &'static [Feature] {
    TO_FIND
}

pub(crate) fn the_rest() -> Vec<Feature> {
    let listed = |feature: Feature| {
        BY_HAND.contains(&feature)
            || CONSOLE_COMMANDS.contains(&feature)
            || TO_FIND.contains(&feature)
    };
    Feature::VARIANTS
        .iter()
        .copied()
        .filter(|feature| feature.spec().surface() == Surface::Window && !listed(*feature))
        .collect()
}

use crate::palette::chord_label;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GestureWord(&'static str);

impl GestureWord {
    const fn as_str(self) -> &'static str {
        self.0
    }
}

const fn gesture_word(gesture: Gesture) -> GestureWord {
    GestureWord(match gesture {
        Gesture::ShiftClick => "shift-click",
        Gesture::ControlClick => "ctrl-click",
        Gesture::AltClick => "alt-click",
        Gesture::DoubleClick => "double-click",
        Gesture::Hover => "hover",
        Gesture::Drag => "drag",
        Gesture::Wheel => "wheel",
        Gesture::ShiftWheel => "shift+wheel",
        Gesture::ControlWheel => "ctrl+wheel",
        Gesture::Pinch => "pinch",
        Gesture::BackButton => "mouse back",
        Gesture::ForwardButton => "mouse forward",
    })
}

fn palette_key() -> Option<Label> {
    Feature::CommandPalette
        .spec()
        .triggers()
        .iter()
        .find_map(|trigger| match trigger {
            Trigger::Key(chord) => Some(chord_label(*chord)),
            Trigger::Command(_)
            | Trigger::Click(_)
            | Trigger::Type(_)
            | Trigger::Gesture(..)
            | Trigger::Palette(..) => None,
        })
}

fn word(trigger: &Trigger) -> Option<Label> {
    match trigger {
        Trigger::Key(chord) => Some(chord_label(*chord)),
        Trigger::Gesture(gesture, _) => Some(Label::new(gesture_word(*gesture).as_str())),
        Trigger::Palette(..) => palette_key(),
        Trigger::Command(_) | Trigger::Click(_) | Trigger::Type(_) => None,
    }
}

pub(crate) fn how(feature: Feature) -> Option<Label> {
    let mut words: Vec<Label> = Vec::new();
    for found in feature.spec().triggers().iter().filter_map(word) {
        if !words.contains(&found) {
            words.push(found);
        }
    }
    let joined = words
        .iter()
        .map(Label::as_str)
        .collect::<Vec<_>>()
        .join(" · ");
    (!words.is_empty()).then(|| Label::new(joined))
}
