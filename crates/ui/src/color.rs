#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Color([u8; 4]);

impl Color {
    pub const TRANSPARENT: Self = Self([0; 4]);

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self([red, green, blue, alpha])
    }

    pub const fn channels(self) -> [u8; 4] {
        self.0
    }

    pub const fn red(self) -> u8 {
        let [value, _, _, _] = self.0;
        value
    }

    pub const fn green(self) -> u8 {
        let [_, value, _, _] = self.0;
        value
    }

    pub const fn blue(self) -> u8 {
        let [_, _, value, _] = self.0;
        value
    }

    pub const fn alpha(self) -> u8 {
        let [_, _, _, value] = self.0;
        value
    }

    #[must_use]
    pub const fn with_alpha(self, alpha: u8) -> Self {
        let [red, green, blue, _] = self.0;
        Self([red, green, blue, alpha])
    }

    pub const fn is_invisible(self) -> bool {
        self.alpha() == 0
    }
}

const fn unchanged(color: Color) -> Color {
    color
}

#[derive(Clone, Copy, Debug)]
pub struct Repaint(fn(Color) -> Color);

impl Repaint {
    pub const NONE: Self = Self(unchanged);

    pub const fn new(paint: fn(Color) -> Color) -> Self {
        Self(paint)
    }

    pub fn paint(self, color: Color) -> Color {
        (self.0)(color)
    }
}

impl Default for Repaint {
    fn default() -> Self {
        Self::NONE
    }
}
