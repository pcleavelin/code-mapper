use crate::index::SymbolId;
use crate::map::name::{Author, GroupName, Note, TourKind, TourName};
use crate::map::step::StepId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepNote {
    pub step: StepId,
    pub note: Option<Note>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddedStep {
    pub symbol: SymbolId,
    pub note: Option<Note>,
    pub below: Vec<AddedStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddedSteps {
    pub under: StepId,
    pub steps: Vec<AddedStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TourEdit {
    pub name: TourName,
    pub kind: TourKind,
    pub group: Option<GroupName>,
    pub note: Option<Note>,
    pub step_notes: Vec<StepNote>,
    pub removed: Vec<StepId>,
    pub added: Vec<AddedSteps>,
    pub author: Author,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddedUnder {
    Step(StepId),
    Added(SymbolId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditChange {
    Renamed(TourName),
    Kind(TourKind),
    Group(Option<GroupName>),
    TourNote(Option<Note>),
    StepNote(StepNote),
    Added { symbol: SymbolId, under: AddedUnder },
    Removed { step: StepId, lost: Option<Note> },
}
