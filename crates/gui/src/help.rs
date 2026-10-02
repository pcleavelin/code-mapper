use features::{Feature, Gesture, Trigger};
use ui::Label;

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
