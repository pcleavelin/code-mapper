use std::fmt;
use std::io;
use std::path::PathBuf;

use domain::{InvalidName, Line, MapError, PathName, Revision, StepId};

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
pub enum Origin {
    File(PathBuf),
    Revision(Revision),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fault {
    Conflict,
    Version(FieldValue),
    NoVersion,
    UnknownAuthor(FieldValue),
    UnknownKind(FieldValue),
    UnknownPathField(FieldKey),
    UnknownStepField(FieldKey),
    SecondStep(FieldValue),
    Order,
    Lines,
    Hash,
    InvalidName(InvalidName),
    InvalidStepId(FieldValue),
    MissingField(FieldKey),
    NoPathLine,
    UnknownParent {
        path: PathName,
        step: StepId,
        parent: FieldValue,
    },
    Map(MapError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub origin: Origin,
    pub line: Option<Line>,
    pub fault: Fault,
}

#[derive(Debug)]
pub enum MapLoadError {
    OldFormat(PathBuf),
    Unreadable { file: PathBuf, error: io::Error },
    Parse(ParseError),
    OnePathPerFile(PathBuf),
    Misplaced { file: PathBuf, path: PathName },
}

#[derive(Debug)]
pub enum MapSaveError {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Located {
    pub(crate) line: Option<Line>,
    pub(crate) fault: Fault,
}

impl Located {
    pub(crate) fn at(line: Line, fault: Fault) -> Self {
        Self {
            line: Some(line),
            fault,
        }
    }

    pub(crate) fn anywhere(fault: Fault) -> Self {
        Self { line: None, fault }
    }

    pub(crate) fn within(self, origin: &Origin) -> ParseError {
        ParseError {
            origin: origin.clone(),
            line: self.line,
            fault: self.fault,
        }
    }
}
