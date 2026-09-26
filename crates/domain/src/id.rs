use std::marker::PhantomData;

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
