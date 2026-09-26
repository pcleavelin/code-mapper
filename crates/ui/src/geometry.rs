use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Px(i32);

impl Px {
    pub const ZERO: Self = Self(0);
    pub const LARGEST: Self = Self(0x7fff_ffff);

    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> i32 {
        self.0
    }

    pub fn of_count(count: usize) -> Self {
        Self(i32::try_from(count).unwrap_or(0x7fff_ffff))
    }

    pub fn wide(self) -> i64 {
        i64::from(self.0)
    }

    pub fn of_wide(value: i64) -> Self {
        Self(i32::try_from(value).unwrap_or(if value < 0 { -0x8000_0000 } else { 0x7fff_ffff }))
    }

    pub fn float(self) -> f32 {
        Coordinate::of_integer(self.0).get()
    }

    pub fn unsigned(self) -> u32 {
        u32::try_from(self.0).unwrap_or(0)
    }

    #[must_use]
    pub const fn absolute(self) -> Self {
        Self(self.0.abs())
    }

    pub const fn ratio(self, divisor: Self) -> i32 {
        self.0 / divisor.0
    }
}

impl Add for Px {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Px {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl Neg for Px {
    type Output = Self;

    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<i32> for Px {
    type Output = Self;

    fn mul(self, factor: i32) -> Self {
        Self(self.0 * factor)
    }
}

impl Mul<Count> for Px {
    type Output = Self;

    fn mul(self, count: Count) -> Self {
        Self(self.0 * Self::of_count(count.get()).0)
    }
}

impl Div<i32> for Px {
    type Output = Self;

    fn div(self, divisor: i32) -> Self {
        Self(self.0 / divisor)
    }
}

impl fmt::Display for Px {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, formatter)
    }
}

impl fmt::Debug for Px {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl AddAssign for Px {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl SubAssign for Px {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Count(usize);

impl Count {
    pub const ZERO: Self = Self(0);

    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    pub const fn get(self) -> usize {
        self.0
    }

    pub fn of_px(value: Px) -> Self {
        Self(usize::try_from(value.get()).unwrap_or(0))
    }
}

impl Add for Count {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl fmt::Display for Count {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl AddAssign for Count {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Point {
    pub horizontal: Px,
    pub vertical: Px,
}

impl Point {
    pub const fn new(horizontal: Px, vertical: Px) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }

    pub const fn along(self, axis: Axis) -> Px {
        match axis {
            Axis::Horizontal => self.horizontal,
            Axis::Vertical => self.vertical,
        }
    }

    #[must_use]
    pub const fn with(self, axis: Axis, value: Px) -> Self {
        match axis {
            Axis::Horizontal => Self::new(value, self.vertical),
            Axis::Vertical => Self::new(self.horizontal, value),
        }
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(
            self.horizontal + other.horizontal,
            self.vertical + other.vertical,
        )
    }
}

impl Sub for Point {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(
            self.horizontal - other.horizontal,
            self.vertical - other.vertical,
        )
    }
}

impl fmt::Debug for Point {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("")
            .field(&self.horizontal)
            .field(&self.vertical)
            .finish()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Extent {
    pub width: Px,
    pub height: Px,
}

impl Extent {
    pub const fn new(width: Px, height: Px) -> Self {
        Self { width, height }
    }

    pub const fn along(self, axis: Axis) -> Px {
        match axis {
            Axis::Horizontal => self.width,
            Axis::Vertical => self.height,
        }
    }

    #[must_use]
    pub const fn with(self, axis: Axis, value: Px) -> Self {
        match axis {
            Axis::Horizontal => Self::new(value, self.height),
            Axis::Vertical => Self::new(self.width, value),
        }
    }
}

impl fmt::Debug for Extent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_list()
            .entry(&self.width)
            .entry(&self.height)
            .finish()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub const BOTH: [Self; 2] = [Self::Horizontal, Self::Vertical];

    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rect {
    pub left: Px,
    pub top: Px,
    pub width: Px,
    pub height: Px,
}

impl Rect {
    pub const fn new(left: Px, top: Px, width: Px, height: Px) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }

    pub const fn at(origin: Point, extent: Extent) -> Self {
        Self::new(
            origin.horizontal,
            origin.vertical,
            extent.width,
            extent.height,
        )
    }

    pub const fn origin(self) -> Point {
        Point::new(self.left, self.top)
    }

    pub const fn extent(self) -> Extent {
        Extent::new(self.width, self.height)
    }

    pub fn right(self) -> Px {
        self.left + self.width
    }

    pub fn bottom(self) -> Px {
        self.top + self.height
    }

    pub fn center(self) -> Point {
        Point::new(self.left + self.width / 2, self.top + self.height / 2)
    }

    pub fn contains(self, point: Point) -> bool {
        point.horizontal >= self.left
            && point.vertical >= self.top
            && point.horizontal < self.right()
            && point.vertical < self.bottom()
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        let left = self.left.max(other.left);
        let top = self.top.max(other.top);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Self::new(
            left,
            top,
            (right - left).max(Px::ZERO),
            (bottom - top).max(Px::ZERO),
        )
    }

    #[must_use]
    pub fn shrink(self, amount: Px) -> Self {
        Self::new(
            self.left + amount,
            self.top + amount,
            (self.width - amount * 2).max(Px::ZERO),
            (self.height - amount * 2).max(Px::ZERO),
        )
    }

    pub fn is_empty(self) -> bool {
        self.width <= Px::ZERO || self.height <= Px::ZERO
    }
}

impl fmt::Debug for Rect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Rect")
            .field("x", &self.left)
            .field("y", &self.top)
            .field("w", &self.width)
            .field("h", &self.height)
            .finish()
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Default)]
pub struct Coordinate(f32);

impl Coordinate {
    pub const ZERO: Self = Self(0.0);

    pub const fn new(value: f32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> f32 {
        self.0
    }

    pub fn of_px(value: Px) -> Self {
        Self::of_integer(value.get())
    }

    pub fn of_integer(value: i32) -> Self {
        let high = i16::try_from(value >> 16).unwrap_or(0);
        let low = u16::try_from(value & 0xffff).unwrap_or(0);
        Self(f32::from(high) * 65536.0 + f32::from(low))
    }

    pub fn narrow(value: f64) -> Self {
        let bits = value.to_bits();
        let negative = bits >> 63 == 1;
        let exponent = i64::try_from((bits >> 52) & 0x7ff).unwrap_or(0) - 1023;
        let mantissa = bits & ((1 << 52) - 1);
        let magnitude = if value.is_nan() {
            f32::NAN.to_bits()
        } else if exponent > 127 {
            f32::INFINITY.to_bits()
        } else if exponent < -126 {
            0
        } else {
            let mut kept = mantissa >> 29;
            let rest = mantissa & ((1 << 29) - 1);
            let half = 1 << 28;
            if rest > half || (rest == half && kept & 1 == 1) {
                kept += 1;
            }
            let biased = u64::try_from(exponent + 127).unwrap_or(0);
            let joined = (biased << 23) + kept;
            if joined >= 0xff << 23 {
                f32::INFINITY.to_bits()
            } else {
                u32::try_from(joined).unwrap_or(0)
            }
        };
        let sign = if negative { 1 << 31 } else { 0 };
        Self(f32::from_bits(sign | magnitude))
    }

    pub fn truncate(self) -> Px {
        Px::new(Self::truncate_bits(f64::from(self.0)))
    }

    pub fn truncate_wide(value: f64) -> Px {
        Px::new(Self::truncate_bits(value))
    }

    fn truncate_bits(value: f64) -> i32 {
        if value.is_nan() {
            return 0;
        }
        let whole = value.trunc();
        if whole >= 2_147_483_647.0 {
            return 0x7fff_ffff;
        }
        if whole <= -2_147_483_648.0 {
            return -0x8000_0000;
        }
        let mut rest = whole.abs();
        let mut magnitude: i64 = 0;
        for bit in (0..31).rev() {
            let weight = f64::from(1_u32 << bit);
            if rest >= weight {
                rest -= weight;
                magnitude |= 1 << bit;
            }
        }
        let signed = if whole < 0.0 { -magnitude } else { magnitude };
        i32::try_from(signed).unwrap_or(0)
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Vector {
    pub horizontal: Coordinate,
    pub vertical: Coordinate,
}

impl Vector {
    pub const ZERO: Self = Self::new(Coordinate::ZERO, Coordinate::ZERO);

    pub const fn new(horizontal: Coordinate, vertical: Coordinate) -> Self {
        Self {
            horizontal,
            vertical,
        }
    }

    pub fn of_point(point: Point) -> Self {
        Self::new(
            Coordinate::of_px(point.horizontal),
            Coordinate::of_px(point.vertical),
        )
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub struct Scale(f64);

impl Scale {
    pub const ONE: Self = Self(1.0);

    pub const fn new(value: f64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FontSize(u32);

impl FontSize {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub fn scaled(base: u32, scale: Scale, minimum: u32) -> Self {
        let size = (f64::from(base) * scale.get())
            .round()
            .max(f64::from(minimum));
        Self(u32::try_from(Coordinate::truncate_wide(size).get()).unwrap_or(minimum))
    }

    pub fn float(self) -> f32 {
        Coordinate::of_integer(i32::try_from(self.0).unwrap_or(0)).get()
    }
}
