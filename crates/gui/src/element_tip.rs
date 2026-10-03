use std::time::Duration;

use features::Feature;
use ui::{Button, Extent, Id, Label, Point, Pointer, Px, Rect};

use crate::palette::chords_label;
use crate::theme::{Cells, ELEMENT_TIP_GAP};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ElementTip {
    text: Label,
    chord: Option<Label>,
}

impl ElementTip {
    pub(crate) fn of(feature: Feature) -> Self {
        Self {
            text: Label::new(feature.spec().summary().as_str()),
            chord: chords_label(feature),
        }
    }

    pub(crate) const fn chord(&self) -> Option<&Label> {
        self.chord.as_ref()
    }

    pub(crate) fn lines(&self, width: Cells) -> Vec<Label> {
        let most = usize::try_from(width.get()).unwrap_or(0).max(1);
        let mut lines: Vec<String> = Vec::new();
        for word in self.text.as_str().split_whitespace() {
            match lines.last_mut() {
                Some(line) if line.chars().count() + 1 + word.chars().count() <= most => {
                    line.push(' ');
                    line.push_str(word);
                }
                _ => lines.push(word.to_owned()),
            }
        }
        lines.into_iter().map(Label::new).collect()
    }

    pub(crate) fn shown(&self) -> Label {
        match &self.chord {
            Some(chord) => Label::new(format!("{}  {}", self.text.as_str(), chord.as_str())),
            None => self.text.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AttachedTip {
    pub(crate) id: Id,
    pub(crate) rect: Rect,
    pub(crate) tip: ElementTip,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Pressing {
    Pressed,
    #[default]
    Released,
}

impl Pressing {
    pub(crate) fn of(pointer: Pointer) -> Self {
        if pointer.down.contains(Button::Left) || pointer.pressed.contains(Button::Left) {
            Self::Pressed
        } else {
            Self::Released
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Resting {
    on: Option<Id>,
    since: Duration,
    pressing: Pressing,
}

impl Resting {
    const DELAY: Duration = Duration::from_millis(500);

    pub(crate) fn rest(&mut self, on: Option<Id>, pressing: Pressing, now: Duration) {
        if on != self.on {
            *self = Self {
                on,
                since: now,
                pressing: Pressing::Released,
            };
        }
        if pressing == Pressing::Pressed {
            self.pressing = Pressing::Pressed;
        }
    }

    pub(crate) fn due(&self, id: Id, now: Duration) -> bool {
        self.on == Some(id)
            && self.pressing == Pressing::Released
            && now.saturating_sub(self.since) >= Self::DELAY
    }

    pub(crate) fn pending(&self, now: Duration) -> bool {
        self.on.is_some()
            && self.pressing == Pressing::Released
            && now.saturating_sub(self.since) < Self::DELAY
    }
}

pub(crate) fn beside(window: Extent, element: Rect, size: Extent) -> Point {
    let across = element.left.min(window.width - size.width).max(Px::ZERO);
    let below = element.bottom() + ELEMENT_TIP_GAP;
    let down = if below + size.height <= window.height {
        below
    } else {
        (element.top - ELEMENT_TIP_GAP - size.height).max(Px::ZERO)
    };
    Point::new(across, down)
}
