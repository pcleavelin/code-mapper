use std::fmt;
use std::time::Duration;

use crate::geometry::{Extent, Point, Vector};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1);
    pub const SHIFT: Self = Self(2);
    pub const ALT: Self = Self(4);

    pub fn new(ctrl: bool, shift: bool, alt: bool) -> Self {
        let mut mods = Self::NONE;
        if ctrl {
            mods = mods.with(Self::CTRL);
        }
        if shift {
            mods = mods.with(Self::SHIFT);
        }
        if alt {
            mods = mods.with(Self::ALT);
        }
        mods
    }

    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn ctrl(self) -> bool {
        self.0 & Self::CTRL.0 != 0
    }

    pub const fn shift(self) -> bool {
        self.0 & Self::SHIFT.0 != 0
    }

    pub const fn alt(self) -> bool {
        self.0 & Self::ALT.0 != 0
    }

    pub const fn same_ctrl_alt(self, other: Self) -> bool {
        self.ctrl() == other.ctrl() && self.alt() == other.alt()
    }
}

impl fmt::Debug for Mods {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Mods")
            .field("ctrl", &self.ctrl())
            .field("shift", &self.shift())
            .field("alt", &self.alt())
            .finish()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Glyph(char);

impl Glyph {
    pub const SPACE: Self = Self(' ');

    pub const fn new(value: char) -> Self {
        Self(value)
    }

    pub const fn get(self) -> char {
        self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Enter,
    Escape,
    Backspace,
    Remove,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Character(Glyph),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Press {
    pub key: Key,
    pub mods: Mods,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Buttons(u8);

impl Buttons {
    pub const NONE: Self = Self(0);

    const fn bit(button: Button) -> u8 {
        match button {
            Button::Left => 1,
            Button::Right => 2,
            Button::Middle => 4,
            Button::Back => 8,
            Button::Forward => 16,
        }
    }

    pub const fn contains(self, button: Button) -> bool {
        self.0 & Self::bit(button) != 0
    }

    pub const fn insert(&mut self, button: Button) {
        self.0 |= Self::bit(button);
    }

    pub const fn remove(&mut self, button: Button) {
        self.0 &= !Self::bit(button);
    }
}

impl fmt::Debug for Buttons {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entry(&self.contains(Button::Left))
            .entry(&self.contains(Button::Right))
            .entry(&self.contains(Button::Middle))
            .finish()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Clicks([u8; 3]);

impl Clicks {
    const fn slot(button: Button) -> Option<usize> {
        match button {
            Button::Left => Some(0),
            Button::Right => Some(1),
            Button::Middle => Some(2),
            Button::Back | Button::Forward => None,
        }
    }

    pub fn count(self, button: Button) -> u8 {
        Self::slot(button)
            .and_then(|slot| self.0.get(slot).copied())
            .unwrap_or(0)
    }

    pub fn set(&mut self, button: Button, count: u8) {
        if let Some(value) = Self::slot(button).and_then(|slot| self.0.get_mut(slot)) {
            *value = count;
        }
    }

    pub fn is_double(self, button: Button) -> bool {
        self.count(button) == 2
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Pointer {
    pub mouse: Point,
    pub pressed: Buttons,
    pub down: Buttons,
    pub clicks: Clicks,
    pub wheel: Vector,
    pub pinch: Pinch,
    pub mods: Mods,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Pinch(f32);

impl Pinch {
    pub const ZERO: Self = Self(0.0);

    pub const fn new(amount: f32) -> Self {
        Self(amount)
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Typed(String);

impl Typed {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push_str(&mut self, text: &str) {
        self.0.push_str(text);
    }

    pub fn push(&mut self, character: char) {
        self.0.push(character);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    pub pointer: Pointer,
    pub keys: Vec<Press>,
    pub typed: Typed,
    pub size: Extent,
    pub time: Duration,
}

impl Input {
    pub fn end_frame(&mut self) {
        self.pointer.pressed = Buttons::NONE;
        self.pointer.clicks = Clicks::default();
        self.pointer.wheel = Vector::ZERO;
        self.pointer.pinch = Pinch::ZERO;
        self.keys.clear();
        self.typed.clear();
    }

    pub fn pressed_key(&self, key: Key, mods: Mods) -> bool {
        self.keys
            .iter()
            .any(|press| press.key == key && press.mods.same_ctrl_alt(mods))
    }
}
