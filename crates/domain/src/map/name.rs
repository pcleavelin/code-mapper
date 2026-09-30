use std::fmt;

use strum::VariantArray;

use crate::index::SymbolName;
use crate::map::error::{InvalidName, MapError};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TourName(String);

impl TourName {
    pub fn new(name: &str) -> Result<Self, MapError> {
        let allowed = |character: char| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        };
        if name.is_empty() || name.starts_with('.') || !name.chars().all(allowed) {
            return Err(MapError::InvalidName(InvalidName::new(name)));
        }
        Ok(Self(name.to_owned()))
    }

    pub fn from_symbol(symbol: &SymbolName) -> Self {
        let replaced: String = symbol
            .as_str()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        let trimmed = replaced.trim_matches(|character| character == '-' || character == '.');
        if trimmed.is_empty() {
            Self("tour".to_owned())
        } else {
            Self(trimmed.to_owned())
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn same_letters(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl fmt::Display for TourName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupName(String);

impl GroupName {
    pub fn new(group: &str) -> Option<Self> {
        let normal = group
            .split('/')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
            .join("/");
        (!normal.is_empty()).then_some(Self(normal))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn last_segment(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    pub(crate) fn child_under(&self, prefix: Option<&Self>) -> Option<Self> {
        let rest = match prefix {
            None => self.0.as_str(),
            Some(prefix) => self
                .0
                .strip_prefix(prefix.0.as_str())?
                .trim_start_matches('/'),
        };
        let segment = rest
            .split('/')
            .next()
            .filter(|segment| !segment.is_empty())?;
        match prefix {
            None => Some(Self(segment.to_owned())),
            Some(prefix) => Some(Self(format!("{}/{segment}", prefix.0))),
        }
    }

    pub(crate) fn moved(&self, old: &Self, new: Option<&Self>) -> Option<String> {
        let rest = self
            .0
            .strip_prefix(old.0.as_str())
            .filter(|rest| rest.is_empty() || rest.starts_with('/'))?;
        let head = new.map_or("", |new| new.0.as_str());
        Some(format!("{head}{rest}"))
    }
}

impl fmt::Display for GroupName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Note(String);

impl Note {
    pub fn new(text: &str) -> Option<Self> {
        (!text.is_empty()).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.0.lines()
    }
}

impl fmt::Display for Note {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextFragment(String);

impl TextFragment {
    pub fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn replace_first(&self, note: Option<&Note>, new: &Self) -> Option<String> {
        let text = note.map_or("", Note::as_str);
        text.contains(self.0.as_str())
            .then(|| text.replacen(self.0.as_str(), new.0.as_str(), 1))
    }
}

impl fmt::Display for TextFragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, VariantArray)]
pub enum TourKind {
    Flow,
    Layer,
    Data,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Author {
    Human,
    Agent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TourCount(u32);

impl TourCount {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) fn of(count: usize) -> Self {
        Self(u32::try_from(count).unwrap_or_default())
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

impl fmt::Display for TourCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}
