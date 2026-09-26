use std::fmt;

use crate::map::name::{GroupName, PathName, TextFragment};
use crate::map::step::StepId;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvalidName(String);

impl InvalidName {
    pub fn new(name: &str) -> Self {
        Self(name.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InvalidName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepAddress {
    pub path: PathName,
    pub step: StepId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapError {
    InvalidName(InvalidName),
    NameTaken(PathName),
    CaseClash {
        name: PathName,
        other: PathName,
    },
    NoGroupGiven,
    NoSuchGroup(GroupName),
    NoSuchPath(PathName),
    NoSuchStep(StepAddress),
    NoSuchParent,
    NoSuchFile,
    NoSuchSymbol,
    OutsideFile,
    LinkToOwnPath,
    LinkedFrom {
        path: PathName,
        steps: Vec<StepAddress>,
    },
    UnderItself,
    NoteLacks(TextFragment),
    SecondStep(StepAddress),
    UnknownParent {
        step: StepAddress,
        parent: StepId,
    },
}
