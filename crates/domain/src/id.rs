use std::marker::PhantomData;

use crate::text::TextHash;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position(usize);

impl Position {
    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> usize {
        self.0
    }
}

pub trait Key: Copy {
    fn at(position: Position) -> Self;

    fn position(self) -> Position;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry<K, V> {
    pub id: K,
    pub item: V,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdList<K, V> {
    items: Vec<V>,
    key: PhantomData<K>,
}

impl<K, V> Default for IdList<K, V> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            key: PhantomData,
        }
    }
}

impl<K: Key, V> IdList<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: V) -> K {
        let id = K::at(Position::new(self.items.len()));
        self.items.push(item);
        id
    }

    pub fn get(&self, id: K) -> Option<&V> {
        self.items.get(id.position().value())
    }

    pub fn get_mut(&mut self, id: K) -> Option<&mut V> {
        self.items.get_mut(id.position().value())
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &V> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.items.iter_mut()
    }

    pub fn ids(&self) -> impl Iterator<Item = K> + use<K, V> {
        (0..self.items.len()).map(|position| K::at(Position::new(position)))
    }

    pub fn entries(&self) -> impl Iterator<Item = Entry<K, &V>> {
        self.items.iter().enumerate().map(|pair| Entry {
            id: K::at(Position::new(pair.0)),
            item: pair.1,
        })
    }
}

impl<K: Key, V> FromIterator<V> for IdList<K, V> {
    fn from_iter<I: IntoIterator<Item = V>>(items: I) -> Self {
        Self {
            items: items.into_iter().collect(),
            key: PhantomData,
        }
    }
}

pub(crate) struct ShortId(String);

impl ShortId {
    const RADIX: u32 = 36;

    pub(crate) fn fresh(seed: &str, digits: usize, taken: impl Fn(&str) -> bool) -> Self {
        let candidate = |counter: u32| -> String {
            let mut hash = TextHash::of_bytes(seed.bytes().chain(counter.to_le_bytes())).value();
            (0..digits)
                .map(|_| {
                    let digit = u32::try_from(hash % u64::from(Self::RADIX)).unwrap_or_default();
                    hash /= u64::from(Self::RADIX);
                    char::from_digit(digit, Self::RADIX).unwrap_or('0')
                })
                .collect()
        };
        let mut counter: u32 = 0;
        loop {
            let id = candidate(counter);
            if !taken(&id) {
                return Self(id);
            }
            counter = counter.wrapping_add(1);
        }
    }

    pub(crate) fn valid(id: &str, digits: usize) -> bool {
        id.chars().count() == digits
            && id
                .chars()
                .all(|character| character.is_ascii_digit() || character.is_ascii_lowercase())
    }

    pub(crate) fn into_text(self) -> String {
        self.0
    }
}
