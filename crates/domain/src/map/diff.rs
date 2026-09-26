use crate::map::name::PathName;
use crate::map::path::Path;
use crate::map::step::{Step, StepId};

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
pub struct PathDiff {
    name: PathName,
    change: Change,
    header: Header,
    steps: Vec<StepDiff>,
    removed: Vec<Step>,
}

impl PathDiff {
    pub fn name(&self) -> &PathName {
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

    pub(crate) fn added(path: &Path) -> Self {
        Self {
            name: path.name().clone(),
            change: Change::Added,
            header: Header::Same,
            steps: path
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

    pub(crate) fn removed_path(base: &Path) -> Self {
        Self {
            name: base.name().clone(),
            change: Change::Removed,
            header: Header::Same,
            steps: Vec::new(),
            removed: base.steps().to_vec(),
        }
    }

    pub(crate) fn between(path: &Path, base: &Path) -> Self {
        let steps: Vec<StepDiff> = path
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
            .filter(|old| path.step(old.id()).is_none())
            .cloned()
            .collect();
        let header = if path.note() != base.note()
            || path.kind() != base.kind()
            || path.group() != base.group()
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
            name: path.name().clone(),
            change,
            header,
            steps,
            removed,
        }
    }
}
