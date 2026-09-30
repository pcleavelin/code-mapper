use std::fmt;
use std::path::PathBuf;

use domain::{Line, Program, RelativePath, SymbolName};
use serde_json::Value;

use crate::wire::WireName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    CodeBlock,
    Gap,
}

impl Mark {
    fn name(self) -> WireName {
        WireName::new(match self {
            Self::CodeBlock => "```",
            Self::Gap => "\n\n",
        })
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HoverText(String);

impl HoverText {
    pub(crate) fn from_parts(parts: &[String]) -> Option<Self> {
        let mut text = String::new();
        for part in parts {
            for line in part.lines().filter(|line| {
                !line
                    .trim_start()
                    .starts_with(Mark::CodeBlock.name().as_str())
            }) {
                if line.trim().is_empty()
                    && (text.is_empty() || text.ends_with(Mark::Gap.name().as_str()))
                {
                    continue;
                }
                text.push_str(line);
                text.push('\n');
            }
        }
        let text = text.trim();
        (!text.is_empty()).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HoverText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
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
