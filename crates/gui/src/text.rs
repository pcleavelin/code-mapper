use std::fmt;

use domain::{Author, PathKind, StepChange};
use std::iter;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tag(&'static str);

impl Tag {
    pub(crate) const fn kind(kind: PathKind) -> Self {
        Self(match kind {
            PathKind::Flow => "flow",
            PathKind::Layer => "layer",
            PathKind::Type => "type",
        })
    }

    pub(crate) const fn author(author: Author) -> Self {
        Self(match author {
            Author::Human => "",
            Author::Agent => " (ai)",
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
}
