use std::fmt;

use domain::{Author, StepChange, TourKind};
use std::iter;
use ui::Count;

use crate::model::Gone;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Clipped(String);

impl Clipped {
    pub(crate) fn right(text: &str, columns: usize) -> Self {
        if text.chars().count() <= columns {
            return Self(text.to_owned());
        }
        Self(
            text.chars()
                .take(columns.saturating_sub(1))
                .chain(iter::once('\u{2026}'))
                .collect(),
        )
    }

    pub(crate) fn left(text: &str, columns: usize) -> Self {
        let count = text.chars().count();
        if count <= columns {
            return Self(text.to_owned());
        }
        Self(
            iter::once('\u{2026}')
                .chain(text.chars().skip(count + 1 - columns.max(1)))
                .collect(),
        )
    }
}

impl fmt::Display for Clipped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Needle(String);

impl Needle {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn found_in(&self, text: &str) -> bool {
        let needle = self.0.as_bytes();
        needle.is_empty()
            || text
                .as_bytes()
                .windows(needle.len())
                .any(|window| window.eq_ignore_ascii_case(needle))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Noun {
    Step,
    Tour,
    Hit,
    Symbol,
    Comment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Counted {
    count: Count,
    noun: Noun,
}

impl Counted {
    pub(crate) const fn new(count: Count, noun: Noun) -> Self {
        Self { count, noun }
    }
}

impl fmt::Display for Counted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let one = self.count.get() == 1;
        let word = match self.noun {
            Noun::Step if one => "step",
            Noun::Step => "steps",
            Noun::Tour if one => "tour",
            Noun::Tour => "tours",
            Noun::Hit if one => "hit",
            Noun::Hit => "hits",
            Noun::Symbol if one => "symbol",
            Noun::Symbol => "symbols",
            Noun::Comment if one => "comment",
            Noun::Comment => "comments",
        };
        write!(formatter, "{} {word}", self.count)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tag(&'static str);

impl Tag {
    pub(crate) const fn kind(kind: TourKind) -> Self {
        Self(match kind {
            TourKind::Flow => "flow",
            TourKind::Layer => "layer",
            TourKind::Data => "data",
        })
    }

    pub(crate) const fn author(author: Author) -> Self {
        Self(match author {
            Author::Human => "",
            Author::Agent => " (ai)",
        })
    }

    pub(crate) const fn author_word(author: Author) -> Self {
        Self(match author {
            Author::Human => "human",
            Author::Agent => "ai",
        })
    }

    pub(crate) const fn gone(gone: Gone) -> Self {
        Self(match gone {
            Gone::Nothing => "",
            Gone::Lines => "(lines gone)",
            Gone::Symbol => "(symbol gone)",
            Gone::Step => "(step gone)",
            Gone::Tour => "(tour gone)",
            Gone::File => "(file gone)",
        })
    }

    pub(crate) const fn change(change: StepChange) -> Self {
        Self(match change {
            StepChange::Added => "new",
            StepChange::Repinned => "re-pinned",
            StepChange::NoteEdited => "note edited",
            StepChange::Relinked => "link changed",
        })
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_keeps_short_text_and_marks_cut_text() {
        assert_eq!(Clipped::right("short", 8).to_string(), "short");
        assert_eq!(
            Clipped::right("a long name", 6).to_string(),
            "a lon\u{2026}"
        );
        assert_eq!(
            Clipped::left("src/main.rs:12", 8).to_string(),
            "\u{2026}n.rs:12"
        );
        assert_eq!(Clipped::left("abc", 0).to_string(), "\u{2026}");
    }

    #[test]
    fn a_needle_matches_ignoring_ascii_case() {
        let needle = Needle::new("count");
        assert!(needle.found_in("IndexCounts"));
        assert!(!needle.found_in("Coun"));
        assert!(Needle::new("").found_in("anything"));
    }

    #[test]
    fn a_count_of_one_takes_the_singular() {
        assert_eq!(
            Counted::new(Count::new(1), Noun::Step).to_string(),
            "1 step"
        );
        assert_eq!(Counted::new(Count::ZERO, Noun::Step).to_string(), "0 steps");
        assert_eq!(
            Counted::new(Count::new(3), Noun::Tour).to_string(),
            "3 tours"
        );
    }
}
