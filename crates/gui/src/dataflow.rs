use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Model, StepKey, TourSlot};
use crate::types::{Access, TypeIx, TypeModel};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Columns {
    #[default]
    Shared,
    All,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DataState {
    pub(crate) columns: Columns,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DataAction {
    TurnColumns,
}

impl DataState {
    pub(crate) fn apply(&mut self, action: DataAction) {
        match action {
            DataAction::TurnColumns => {
                self.columns = match self.columns {
                    Columns::Shared => Columns::All,
                    Columns::All => Columns::Shared,
                };
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Mark {
    Defines,
    Access(Access),
}

#[derive(Clone, Debug)]
pub(crate) struct Lane {
    pub(crate) key: StepKey,
    pub(crate) number: String,
    pub(crate) depth: usize,
    pub(crate) symbol: String,
    pub(crate) marks: BTreeMap<TypeIx, BTreeSet<Mark>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Flow {
    pub(crate) lanes: Vec<Lane>,
    pub(crate) columns: Vec<TypeIx>,
    pub(crate) spans: Vec<(usize, usize)>,
    pub(crate) hidden: usize,
}

pub(crate) fn flow(model: &Model, types: &TypeModel, path: TourSlot, columns: Columns) -> Flow {
    let mut lanes = Vec::new();
    for numbered in model.numbered(path) {
        let key = StepKey {
            tour: path,
            step: numbered.step,
        };
        let Some(step) = model.step(key) else {
            continue;
        };
        let mut marks: BTreeMap<TypeIx, BTreeSet<Mark>> = BTreeMap::new();
        let resolved = step.resolved_symbol();
        if let Some(ty) = resolved.and_then(|symbol| types.type_of.get(&symbol)) {
            marks.entry(*ty).or_default().insert(Mark::Defines);
        } else {
            let fns = match resolved.and_then(|symbol| types.fn_of.get(&symbol)) {
                Some(fn_ix) => vec![*fn_ix],
                None => model
                    .index
                    .find_file(step.file())
                    .map(|file| types.fns_within(file, step.span()))
                    .unwrap_or_default(),
            };
            for fn_ix in fns {
                if let Some(node) = types.fns.get(fn_ix) {
                    for (ty, accesses) in &node.uses {
                        let entry = marks.entry(*ty).or_default();
                        entry.extend(accesses.iter().map(|access| Mark::Access(*access)));
                    }
                }
            }
        }
        lanes.push(Lane {
            key,
            number: numbered.number.as_str().to_owned(),
            depth: usize::try_from(numbered.depth.value()).unwrap_or(0),
            symbol: step
                .symbol()
                .map_or_else(|| "(lines)".to_owned(), ToString::to_string),
            marks,
        });
    }
    let mut first: Vec<TypeIx> = Vec::new();
    let mut count: BTreeMap<TypeIx, usize> = BTreeMap::new();
    for lane in &lanes {
        for ty in lane.marks.keys() {
            if !first.contains(ty) {
                first.push(*ty);
            }
            *count.entry(*ty).or_insert(0) += 1;
        }
    }
    let total = first.len();
    let kept: Vec<TypeIx> = first
        .into_iter()
        .filter(|ty| columns == Columns::All || count.get(ty).copied().unwrap_or(0) >= 2)
        .collect();
    let spans = kept
        .iter()
        .map(|ty| {
            let rows: Vec<usize> = lanes
                .iter()
                .enumerate()
                .filter(|(_, lane)| lane.marks.contains_key(ty))
                .map(|(at, _)| at)
                .collect();
            (
                rows.first().copied().unwrap_or(0),
                rows.last().copied().unwrap_or(0),
            )
        })
        .collect();
    Flow {
        hidden: total - kept.len(),
        lanes,
        columns: kept,
        spans,
    }
}

impl Mark {
    pub(crate) fn strongest(marks: &BTreeSet<Self>) -> Option<Self> {
        let order = [
            Self::Defines,
            Self::Access(Access::Construct),
            Self::Access(Access::Mutate),
            Self::Access(Access::Consume),
            Self::Access(Access::Return),
            Self::Access(Access::Read),
            Self::Access(Access::Lend),
        ];
        order.into_iter().find(|mark| marks.contains(mark))
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Defines => "def",
            Self::Access(access) => access.name(),
        }
    }
}
