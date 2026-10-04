use domain::{MapError, RelativePath, Revision, SymbolName, TextFragment, TourName};
use io_comments::{CommentLoadError, CommentSaveError};
use io_map::{MapLoadError, MapSaveError, ParseError};
use io_vcs::Program;

use crate::convert::{Count, StepIndex, Under};
use crate::output::Output;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepPlace {
    pub tour: TourName,
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
        tour: TourName,
        steps: Vec<StepPlace>,
    },
    NoSuchTour(TextFragment),
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
    NoLinkTarget,
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
    NoSuchComment(TextFragment),
    EmptyComment,
    EmptyReply,
    CommentLoad(CommentLoadError),
    CommentSave(CommentSaveError),
    Stale {
        steps: Count,
        links: Count,
    },
}

impl From<MapError> for Failure {
    fn from(error: MapError) -> Self {
        Self::Map(error)
    }
}
