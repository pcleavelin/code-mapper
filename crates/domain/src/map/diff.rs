use crate::map::name::TourName;
use crate::map::step::{Step, StepId};
use crate::map::tour::Tour;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Change {
    Same,
    Added,
    Removed,
    Changed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StepChange {
    Added,
    Repinned,
    NoteEdited,
    Relinked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepDiff {
    pub step: StepId,
    pub change: Option<StepChange>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Header {
    Same,
    Changed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TourDiff {
    name: TourName,
    change: Change,
    header: Header,
    steps: Vec<StepDiff>,
    removed: Vec<Step>,
}

impl TourDiff {
    pub fn name(&self) -> &TourName {
        &self.name
    }

    pub fn change(&self) -> Change {
        self.change
    }

    pub fn note_changed(&self) -> bool {
        self.header == Header::Changed
    }

    pub fn steps(&self) -> &[StepDiff] {
        &self.steps
    }

    pub fn removed(&self) -> &[Step] {
        &self.removed
    }

    pub(crate) fn added(tour: &Tour) -> Self {
        Self {
            name: tour.name().clone(),
            change: Change::Added,
            header: Header::Same,
            steps: tour
                .steps()
                .iter()
                .map(|step| StepDiff {
                    step: step.id().clone(),
                    change: Some(StepChange::Added),
                })
                .collect(),
            removed: Vec::new(),
        }
    }

    pub(crate) fn removed_tour(base: &Tour) -> Self {
        Self {
            name: base.name().clone(),
            change: Change::Removed,
            header: Header::Same,
            steps: Vec::new(),
            removed: base.steps().to_vec(),
        }
    }

    pub(crate) fn between(tour: &Tour, base: &Tour) -> Self {
        let steps: Vec<StepDiff> = tour
            .steps()
            .iter()
            .map(|step| StepDiff {
                step: step.id().clone(),
                change: base.step(step.id()).map_or(Some(StepChange::Added), |old| {
                    if old.anchor() != step.anchor() {
                        Some(StepChange::Repinned)
                    } else if old.note() != step.note() {
                        Some(StepChange::NoteEdited)
                    } else if old.link() != step.link() {
                        Some(StepChange::Relinked)
                    } else {
                        None
                    }
                }),
            })
            .collect();
        let removed: Vec<Step> = base
            .steps()
            .iter()
            .filter(|old| tour.step(old.id()).is_none())
            .cloned()
            .collect();
        let header = if tour.note() != base.note()
            || tour.kind() != base.kind()
            || tour.group() != base.group()
        {
            Header::Changed
        } else {
            Header::Same
        };
        let change = if header == Header::Changed
            || !removed.is_empty()
            || steps.iter().any(|step| step.change.is_some())
        {
            Change::Changed
        } else {
            Change::Same
        };
        Self {
            name: tour.name().clone(),
            change,
            header,
            steps,
            removed,
        }
    }
}
