use std::fmt;
use std::io;
use std::path::PathBuf;

use domain::Line;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldKey(String);

impl FieldKey {
    pub fn new(key: &str) -> Self {
        Self(key.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FieldKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldValue(String);

impl FieldValue {
    pub fn new(value: &str) -> Self {
        Self(value.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FieldValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommentFault {
    Version(FieldValue),
    NoVersion,
    UnknownField(FieldKey),
    UnknownAuthor(FieldValue),
    UnknownTarget(FieldValue),
    MissingField(FieldKey),
    Lines,
    Hash,
    InvalidName(FieldValue),
    InvalidStepId(FieldValue),
    EmptyText(FieldKey),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentParseError {
    pub file: PathBuf,
    pub line: Option<Line>,
    pub fault: CommentFault,
}

#[derive(Debug)]
pub enum CommentLoadError {
    Unreadable { file: PathBuf, error: io::Error },
    Parse(CommentParseError),
    InvalidId(PathBuf),
}

#[derive(Debug)]
pub enum CommentSaveError {
    CreateDirectory {
        directory: PathBuf,
        error: io::Error,
    },
    Write {
        file: PathBuf,
        error: io::Error,
    },
    Remove {
        file: PathBuf,
        error: io::Error,
    },
}
