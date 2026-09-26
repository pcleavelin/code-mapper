use domain::{MapError, PathName, RelativePath, Revision, SymbolName, TextFragment};
use io_map::{MapLoadError, MapSaveError, ParseError};
use io_vcs::Program;

use crate::convert::{Count, StepIndex, Under};
use crate::output::Output;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepPlace {
    pub path: PathName,
    pub index: StepIndex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub name: Output,
    pub label: Output,
}

#[derive(Debug)]
pub enum Failure {
    Map(MapError),
    LinkedFrom {
        path: PathName,
        steps: Vec<StepPlace>,
    },
    NoSuchPath(TextFragment),
    NoSuchStep,
    NoSuchFile(RelativePath),
    NoSuchSymbol(SymbolName),
    SymbolClash {
        query: SymbolName,
        candidates: Vec<Candidate>,
    },
    LineRange,
    NoPlaceUnder {
        under: Under,
        steps: Count,
    },
    NoLink(StepIndex),
    Regex(regex::Error),
    ServersInBackground,
    NoRepository,
    NoMapAt {
        directory: RelativePath,
        revision: Revision,
        program: Program,
    },
    Parse(ParseError),
    Load(MapLoadError),
    Save(MapSaveError),
    Stale {
        steps: Count,
        links: Count,
    },
    NoWindow,
}

impl From<MapError> for Failure {
    fn from(error: MapError) -> Self {
        Self::Map(error)
    }
}
