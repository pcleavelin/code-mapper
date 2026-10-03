use std::fmt;

use strum::VariantArray;

use crate::layout::SplitDirection;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, VariantArray)]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontFamily(String);

impl FontFamily {
    pub fn new(name: &str) -> Option<Self> {
        let name = name.trim();
        (!name.is_empty() && !name.contains(['\n', '\r'])).then(|| Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FontFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BaseFontSize(u16);

impl BaseFontSize {
    pub const SMALLEST: Self = Self(8);
    pub const LARGEST: Self = Self(32);
    const START: Self = Self(14);

    pub fn new(size: u16) -> Option<Self> {
        (Self::SMALLEST.0..=Self::LARGEST.0)
            .contains(&size)
            .then_some(Self(size))
    }

    pub const fn get(self) -> u16 {
        self.0
    }

    #[must_use]
    pub fn larger(self) -> Self {
        Self::new(self.0 + 1).unwrap_or(self)
    }

    #[must_use]
    pub fn smaller(self) -> Self {
        Self::new(self.0.saturating_sub(1)).unwrap_or(self)
    }
}

impl Default for BaseFontSize {
    fn default() -> Self {
        Self::START
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    theme: Theme,
    font: Option<FontFamily>,
    size: BaseFontSize,
    graph_direction: SplitDirection,
}

impl Settings {
    pub const fn theme(&self) -> Theme {
        self.theme
    }

    pub const fn font(&self) -> Option<&FontFamily> {
        self.font.as_ref()
    }

    pub const fn size(&self) -> BaseFontSize {
        self.size
    }

    pub const fn graph_direction(&self) -> SplitDirection {
        self.graph_direction
    }

    #[must_use]
    pub const fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    #[must_use]
    pub fn with_font(mut self, font: Option<FontFamily>) -> Self {
        self.font = font;
        self
    }

    #[must_use]
    pub const fn with_size(mut self, size: BaseFontSize) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub const fn with_graph_direction(mut self, direction: SplitDirection) -> Self {
        self.graph_direction = direction;
        self
    }
}

#[cfg(test)]
mod tests;
