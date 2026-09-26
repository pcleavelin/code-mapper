use crate::color::Color;
use crate::geometry::{Axis, Point, Px};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Exact(Px),
    Fit,
    Grow,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Row,
    Column,
}

impl Direction {
    pub const fn main(self) -> Axis {
        match self {
            Self::Row => Axis::Horizontal,
            Self::Column => Axis::Vertical,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Start,
    Center,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Layout {
    pub direction: Direction,
    pub width: Size,
    pub height: Size,
    pub floating: Option<Point>,
    pub padding: Px,
    pub gap: Px,
    pub cross: Align,
    pub scroll: Option<Point>,
}

impl Layout {
    pub const fn row() -> Self {
        Self {
            direction: Direction::Row,
            width: Size::Fit,
            height: Size::Fit,
            floating: None,
            padding: Px::ZERO,
            gap: Px::ZERO,
            cross: Align::Start,
            scroll: None,
        }
    }

    pub const fn column() -> Self {
        Self {
            direction: Direction::Column,
            ..Self::row()
        }
    }

    pub const fn size_along(self, axis: Axis) -> Size {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }

    pub const fn clips(self) -> bool {
        self.scroll.is_some()
    }

    pub const fn scroll_offset(self) -> Point {
        match self.scroll {
            Some(offset) => offset,
            None => Point::new(Px::ZERO, Px::ZERO),
        }
    }

    #[must_use]
    pub const fn size(mut self, width: Size, height: Size) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    #[must_use]
    pub const fn grow(self) -> Self {
        self.size(Size::Grow, Size::Grow)
    }

    #[must_use]
    pub const fn grow_width(mut self) -> Self {
        self.width = Size::Grow;
        self
    }

    #[must_use]
    pub const fn grow_height(mut self) -> Self {
        self.height = Size::Grow;
        self
    }

    #[must_use]
    pub const fn width(mut self, width: Px) -> Self {
        self.width = Size::Exact(width);
        self
    }

    #[must_use]
    pub const fn height(mut self, height: Px) -> Self {
        self.height = Size::Exact(height);
        self
    }

    #[must_use]
    pub const fn padding(mut self, padding: Px) -> Self {
        self.padding = padding;
        self
    }

    #[must_use]
    pub const fn gap(mut self, gap: Px) -> Self {
        self.gap = gap;
        self
    }

    #[must_use]
    pub const fn cross(mut self, align: Align) -> Self {
        self.cross = align;
        self
    }

    #[must_use]
    pub const fn scroll(mut self, offset: Point) -> Self {
        self.scroll = Some(offset);
        self
    }

    #[must_use]
    pub const fn floating(mut self, at: Point) -> Self {
        self.floating = Some(at);
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Sides(u8);

impl Sides {
    pub const NONE: Self = Self(0);
    pub const LEFT: Self = Self(1);
    pub const RIGHT: Self = Self(2);
    pub const TOP: Self = Self(4);
    pub const BOTTOM: Self = Self(8);
    pub const ALL: Self = Self(15);

    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, side: Self) -> bool {
        self.0 & side.0 != 0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Style {
    pub background: Option<Color>,
    pub border: Sides,
    pub border_color: Color,
}

impl Style {
    pub const NONE: Self = Self {
        background: None,
        border: Sides::NONE,
        border_color: Color::TRANSPARENT,
    };

    pub const fn background(color: Color) -> Self {
        Self {
            background: Some(color),
            ..Self::NONE
        }
    }

    #[must_use]
    pub const fn border(mut self, sides: Sides, color: Color) -> Self {
        self.border = sides;
        self.border_color = color;
        self
    }
}
