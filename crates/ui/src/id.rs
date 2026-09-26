use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(u64);

impl Id {
    const OFFSET: Self = Self(0xcbf2_9ce4_8422_2325);
    const PRIME: Self = Self(0x0100_0000_01b3);
    const GOLDEN: Self = Self(0x9e37_79b9_7f4a_7c15);

    pub fn new(name: &str) -> Self {
        Self::OFFSET.with(name)
    }

    #[must_use]
    pub fn with(self, suffix: &str) -> Self {
        let mut hash = self.0;
        for byte in suffix.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(Self::PRIME.0);
        }
        Self(hash)
    }

    #[must_use]
    pub fn nth(self, number: usize) -> Self {
        let wide = u64::try_from(number).unwrap_or(0);
        Self((self.0 ^ wide.wrapping_mul(Self::GOLDEN.0)).wrapping_mul(Self::PRIME.0))
    }

    pub fn from_name(name: &str) -> Self {
        if let Some((base, number)) = name.split_once('/') {
            Self::new(base).nth(number.parse().unwrap_or(0))
        } else if let Some((base, suffix)) = name.split_once('@') {
            Self::new(base).with(suffix)
        } else {
            Self::new(name)
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}
