use std::collections::BTreeSet;
use std::fmt;

use crate::index::{Depth, Index, SymbolName};
use crate::map::error::{MapError, StepAddress};
use crate::map::name::{Author, GroupName, Note, PathKind, PathName};
use crate::map::step::{Step, StepId};
use crate::text::{Line, RelativePath};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedStep {
    pub step: StepId,
    pub depth: Depth,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StepNumber(Vec<u32>);

impl StepNumber {
    pub fn parts(&self) -> &[u32] {
        &self.0
    }
}

impl fmt::Display for StepNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for part in &self.0 {
            if !first {
                formatter.write_str(".")?;
            }
            first = false;
            write!(formatter, "{part}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumberedStep {
    pub step: StepId,
    pub depth: Depth,
    pub number: StepNumber,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParentLabel {
    Symbol(SymbolName),
    Line { file: RelativePath, line: Line },
    TopLevel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    name: PathName,
    kind: PathKind,
    author: Author,
    group: Option<GroupName>,
    note: Option<Note>,
    steps: Vec<Step>,
}

impl Path {
    pub fn new(
        name: PathName,
        kind: PathKind,
        author: Author,
        group: Option<GroupName>,
        note: Option<Note>,
        mut steps: Vec<Step>,
    ) -> Result<Self, MapError> {
        let mut ids = BTreeSet::new();
        for step in &steps {
            if !ids.insert(step.id().clone()) {
                return Err(MapError::SecondStep(StepAddress {
                    path: name,
                    step: step.id().clone(),
                }));
            }
        }
        if let Some(orphan) = steps
            .iter()
            .find(|step| step.parent().is_some_and(|parent| !ids.contains(parent)))
        {
            return Err(MapError::UnknownParent {
                step: StepAddress {
                    path: name.clone(),
                    step: orphan.id().clone(),
                },
                parent: orphan
                    .parent()
                    .cloned()
                    .unwrap_or_else(|| orphan.id().clone()),
            });
        }
        steps.sort_by(|one, other| (one.order(), one.id()).cmp(&(other.order(), other.id())));
        Ok(Self {
            name,
            kind,
            author,
            group,
            note,
            steps,
        })
    }

    pub(crate) fn empty(name: PathName, kind: PathKind, author: Author) -> Self {
        Self {
            name,
            kind,
            author,
            group: None,
            note: None,
            steps: Vec::new(),
        }
    }

    pub fn name(&self) -> &PathName {
        &self.name
    }

    pub fn kind(&self) -> PathKind {
        self.kind
    }

    pub fn author(&self) -> Author {
        self.author
    }

    pub fn group(&self) -> Option<&GroupName> {
        self.group.as_ref()
    }

    pub fn note(&self) -> Option<&Note> {
        self.note.as_ref()
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    pub fn step(&self, id: &StepId) -> Option<&Step> {
        self.steps.iter().find(|step| step.id() == id)
    }

    pub(crate) fn step_mut(&mut self, id: &StepId) -> Option<&mut Step> {
        self.steps.iter_mut().find(|step| step.id() == id)
    }

    pub(crate) fn steps_mut(&mut self) -> &mut Vec<Step> {
        &mut self.steps
    }

    pub(crate) fn set_name(&mut self, name: PathName) {
        self.name = name;
    }

    pub(crate) fn set_group(&mut self, group: Option<GroupName>) {
        self.group = group;
    }

    pub(crate) fn set_note(&mut self, note: Option<Note>) {
        self.note = note;
    }

    pub fn tree_order(&self) -> Vec<PlacedStep> {
        self.tree_order_by(|_, _| None)
    }

    pub fn tree_order_by(&self, key: impl Fn(&Step, &Step) -> Option<Line>) -> Vec<PlacedStep> {
        let steps = &self.steps;
        let position = |id: &StepId| steps.iter().position(|step| step.id() == id);
        let roots = steps.iter().enumerate().filter(|pair| {
            pair.1
                .parent()
                .is_none_or(|parent| position(parent).is_none_or(|found| found == pair.0))
        });
        let starts: Vec<usize> = roots.map(|pair| pair.0).chain(0..steps.len()).collect();
        let mut seen = BTreeSet::new();
        let mut order = Vec::with_capacity(steps.len());
        for start in starts {
            let mut stack = vec![(start, Depth::default())];
            while let Some((at, depth)) = stack.pop() {
                let Some(step) = steps.get(at) else {
                    continue;
                };
                if !seen.insert(at) {
                    continue;
                }
                order.push(PlacedStep {
                    step: step.id().clone(),
                    depth,
                });
                let mut children: Vec<(Option<Line>, usize)> = steps
                    .iter()
                    .enumerate()
                    .filter(|pair| pair.1.parent() == Some(step.id()))
                    .map(|pair| (key(step, pair.1), pair.0))
                    .collect();
                children.sort_by_key(|child| (child.0.is_none(), child.0, child.1));
                stack.extend(
                    children
                        .into_iter()
                        .rev()
                        .map(|child| (child.1, depth.deeper())),
                );
            }
        }
        order
    }

    pub fn numbered(&self, index: &Index) -> Vec<NumberedStep> {
        let call_line = |parent: &Step, child: &Step| {
            let name = child.symbol()?;
            let source = index.file(index.find_file(parent.file())?)?;
            parent
                .span()
                .lines()
                .find(|line| source.call_site(*line, name))
        };
        let mut counters: Vec<u32> = Vec::new();
        self.tree_order_by(call_line)
            .into_iter()
            .map(|placed| {
                let level = placed.depth.position();
                counters.truncate(level + 1);
                match counters.get_mut(level) {
                    Some(counter) => *counter += 1,
                    None => counters.push(1),
                }
                NumberedStep {
                    step: placed.step,
                    depth: placed.depth,
                    number: StepNumber(counters.clone()),
                }
            })
            .collect()
    }

    pub fn descendants(&self, id: &StepId) -> Vec<StepId> {
        let order = self.tree_order();
        let mut rest = order.into_iter().skip_while(|placed| &placed.step != id);
        let Some(found) = rest.next() else {
            return Vec::new();
        };
        rest.take_while(|placed| placed.depth > found.depth)
            .map(|placed| placed.step)
            .collect()
    }

    pub fn parent_label(&self, id: &StepId) -> Option<ParentLabel> {
        let step = self.step(id)?;
        Some(match step.parent().and_then(|parent| self.step(parent)) {
            Some(parent) => match parent.symbol() {
                Some(symbol) => ParentLabel::Symbol(symbol.clone()),
                None => ParentLabel::Line {
                    file: parent.file().clone(),
                    line: parent.span().start(),
                },
            },
            None => ParentLabel::TopLevel,
        })
    }
}
