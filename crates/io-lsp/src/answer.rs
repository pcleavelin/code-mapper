use std::path::PathBuf;

use domain::{Line, Program, RelativePath, SymbolName};
use serde_json::Value;

use crate::markdown::Markdown;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Character(u32);

impl Character {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DocumentPosition {
    pub file: RelativePath,
    pub line: Line,
    pub character: Character,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub line: Line,
    pub character: Character,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OutlineKind(u64);

impl OutlineKind {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RangeEnd {
    LineStart,
    Inside,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outline {
    pub name: SymbolName,
    pub kind: OutlineKind,
    pub selection: Position,
    pub end: Line,
    pub range_end: RangeEnd,
    pub children: Vec<Outline>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply<T> {
    Given(T),
    Unanswered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallItem(Value);

impl CallItem {
    pub fn new(item: Value) -> Self {
        Self(item)
    }

    pub(crate) fn as_value(&self) -> &Value {
        &self.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Fragment(String);

impl Fragment {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub(crate) fn push(&mut self, text: &str) {
        self.0.push_str(text);
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Inline {
    Prose(Fragment),
    Code(Fragment),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HoverLine {
    Prose(Vec<Inline>),
    Heading(Vec<Inline>),
    Fenced(Fragment),
    Rule,
    Blank,
}

impl HoverLine {
    pub fn plain(&self) -> Fragment {
        let mut plain = Fragment::default();
        match self {
            Self::Prose(inlines) | Self::Heading(inlines) => {
                for inline in inlines {
                    match inline {
                        Inline::Prose(text) | Inline::Code(text) => plain.push(text.as_str()),
                    }
                }
            }
            Self::Fenced(text) => plain.push(text.as_str()),
            Self::Rule | Self::Blank => {}
        }
        plain
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HoverText(Vec<HoverLine>);

impl HoverText {
    pub(crate) fn from_parts(parts: &[String]) -> Option<Self> {
        let mut lines: Vec<HoverLine> = Vec::new();
        for part in parts {
            for line in Markdown::new(part).lines() {
                let gap = line == HoverLine::Blank;
                if gap && lines.last().is_none_or(|last| *last == HoverLine::Blank) {
                    continue;
                }
                lines.push(line);
            }
        }
        while lines.last() == Some(&HoverLine::Blank) {
            lines.pop();
        }
        (!lines.is_empty()).then_some(Self(lines))
    }

    pub fn lines(&self) -> &[HoverLine] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub path: PathBuf,
    pub file: Option<RelativePath>,
    pub line: Line,
    pub character: Character,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartError {
    Missing(Program),
    Failed(Program),
}
