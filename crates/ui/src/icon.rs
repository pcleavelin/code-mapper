use std::fmt;

use strum::VariantArray;

use crate::input::Glyph;
use crate::text::Label;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, VariantArray)]
pub enum Icon {
    Add,
    Close,
    SplitAcross,
    SplitDown,
    Back,
    Forward,
    Unfolded,
    Folded,
    MoreAbove,
    MoreBelow,
    Check,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IconName(&'static str);

impl IconName {
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for IconName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Icon {
    pub const fn glyph(self) -> Glyph {
        Glyph::new(match self {
            Self::Add => '\u{EA60}',
            Self::Close => '\u{EA76}',
            Self::SplitAcross => '\u{EB56}',
            Self::SplitDown => '\u{EB57}',
            Self::Back => '\u{EA9B}',
            Self::Forward => '\u{EA9C}',
            Self::Unfolded => '\u{EAB4}',
            Self::Folded => '\u{EAB6}',
            Self::MoreAbove => '\u{EAF4}',
            Self::MoreBelow => '\u{EAF3}',
            Self::Check => '\u{EAB2}',
            Self::Up => '\u{EAA1}',
            Self::Down => '\u{EA9A}',
        })
    }

    pub const fn name(self) -> IconName {
        IconName(match self {
            Self::Add => "add",
            Self::Close => "close",
            Self::SplitAcross => "split-across",
            Self::SplitDown => "split-down",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::Unfolded => "unfolded",
            Self::Folded => "folded",
            Self::MoreAbove => "more-above",
            Self::MoreBelow => "more-below",
            Self::Check => "check",
            Self::Up => "up",
            Self::Down => "down",
        })
    }

    pub fn of(glyph: Glyph) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|icon| icon.glyph() == glyph)
    }
}

impl From<Icon> for Label {
    fn from(icon: Icon) -> Self {
        Self::new(String::from(icon.glyph().get()))
    }
}
