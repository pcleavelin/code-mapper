use std::collections::{BTreeMap, BTreeSet};

use domain::{Row, TourKind};

use crate::derived::Layers;
use crate::model::{Model, StepKey, TourSlot};
use crate::sequence::Unit;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Order {
    #[default]
    List,
    Shared,
}

impl Order {
    pub(crate) const fn turned(self) -> Self {
        match self {
            Self::List => Self::Shared,
            Self::Shared => Self::List,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AtlasState {
    pub(crate) order: Order,
    pub(crate) column: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AtlasAction {
    TurnOrder,
    Column(String),
}

impl AtlasState {
    pub(crate) fn apply(&mut self, action: AtlasAction) {
        match action {
            AtlasAction::TurnOrder => self.order = self.order.turned(),
            AtlasAction::Column(name) => {
                self.column = if self.column.as_ref() == Some(&name) {
                    None
                } else {
                    Some(name)
                };
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Relation {
    Itself,
    LinksTo,
    LinkedFrom,
    Both,
    None,
}

#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub(crate) slot: TourSlot,
    pub(crate) name: String,
    pub(crate) kind: TourKind,
    pub(crate) depth: usize,
    pub(crate) counts: Vec<usize>,
    pub(crate) first: Vec<Option<StepKey>>,
    pub(crate) stale: usize,
    pub(crate) shared: usize,
    pub(crate) relation: Relation,
}

#[derive(Clone, Debug)]
pub(crate) enum Entry {
    Group { name: String, depth: usize },
    Path(Line),
}

#[derive(Clone, Debug)]
pub(crate) struct Coverage {
    pub(crate) covered: usize,
    pub(crate) total: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Atlas {
    pub(crate) units: Vec<String>,
    pub(crate) coverage: Vec<Coverage>,
    pub(crate) paths_per_unit: Vec<usize>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) levels: Vec<usize>,
}

fn identities(model: &Model, slot: TourSlot) -> BTreeSet<(String, String)> {
    model.tour(slot).map_or_else(BTreeSet::new, |path| {
        path.steps()
            .iter()
            .map(|step| {
                (
                    step.file().as_str().to_owned(),
                    step.symbol().map_or_else(String::new, ToString::to_string),
                )
            })
            .collect()
    })
}

fn links_between(model: &Model, from: TourSlot, to: TourSlot) -> bool {
    let (Some(from), Some(to)) = (model.tour(from), model.tour(to)) else {
        return false;
    };
    from.steps()
        .iter()
        .any(|step| step.link() == Some(to.name()))
}

pub(crate) fn atlas(model: &Model, state: &AtlasState, layers: &Layers) -> Atlas {
    let unit = Unit::Crate;
    let coverage_map = model.map.coverage();
    let mut tallies: BTreeMap<String, Coverage> = BTreeMap::new();
    let mut units: Vec<String> = Vec::new();
    for file in model.index.files() {
        let total = file.symbols().count();
        if total == 0 {
            continue;
        }
        let name = unit.of(file.path().as_str());
        let covered = file
            .symbols()
            .filter(|symbol| coverage_map.covers(file.path(), symbol.span()))
            .count();
        if !units.contains(&name) {
            units.push(name.clone());
        }
        let tally = tallies.entry(name).or_insert(Coverage {
            covered: 0,
            total: 0,
        });
        tally.covered += covered;
        tally.total += total;
    }
    let selected = model.nav.tour();
    let chosen = selected.map(|slot| identities(model, slot));
    let mut entries = Vec::new();
    let mut paths_per_unit = vec![0; units.len()];
    for row in model.map.rows_where(|_| true) {
        match row {
            Row::Group { group, depth, .. } => entries.push(Entry::Group {
                name: group.to_string(),
                depth: usize::try_from(depth.value()).unwrap_or(0),
            }),
            Row::Tour { name, depth } => {
                let Some(slot) = model.find_tour(&name) else {
                    continue;
                };
                let Some(path) = model.tour(slot) else {
                    continue;
                };
                let mut counts = vec![0; units.len()];
                let mut first = vec![None; units.len()];
                let mut stale = 0;
                for placed in model.tree_order(slot) {
                    let key = StepKey {
                        tour: slot,
                        step: placed.step,
                    };
                    let Some(step) = model.step(key) else {
                        continue;
                    };
                    if step.is_stale() {
                        stale += 1;
                    }
                    let owner = unit.of(step.file().as_str());
                    let at = if let Some(at) = units.iter().position(|known| *known == owner) {
                        at
                    } else {
                        units.push(owner);
                        counts.push(0);
                        first.push(None);
                        paths_per_unit.push(0);
                        units.len() - 1
                    };
                    if let Some(count) = counts.get_mut(at) {
                        *count += 1;
                    }
                    if let Some(slot_first) = first.get_mut(at)
                        && slot_first.is_none()
                    {
                        *slot_first = Some(key);
                    }
                }
                for (at, count) in counts.iter().enumerate() {
                    if *count > 0
                        && let Some(total) = paths_per_unit.get_mut(at)
                    {
                        *total += 1;
                    }
                }
                let shared = match (&chosen, selected) {
                    (Some(set), Some(other)) if other != slot => {
                        identities(model, slot).intersection(set).count()
                    }
                    _ => 0,
                };
                let relation = match selected {
                    Some(other) if other == slot => Relation::Itself,
                    Some(other) => match (
                        links_between(model, slot, other),
                        links_between(model, other, slot),
                    ) {
                        (true, true) => Relation::Both,
                        (true, false) => Relation::LinksTo,
                        (false, true) => Relation::LinkedFrom,
                        (false, false) => Relation::None,
                    },
                    None => Relation::None,
                };
                entries.push(Entry::Path(Line {
                    slot,
                    name: path.name().to_string(),
                    kind: path.kind(),
                    depth: usize::try_from(depth.value()).unwrap_or(0),
                    counts,
                    first,
                    stale,
                    shared,
                    relation,
                }));
            }
        }
    }
    let width = units.len();
    for entry in &mut entries {
        if let Entry::Path(line) = entry {
            line.counts.resize(width, 0);
            line.first.resize(width, None);
        }
    }
    let coverage: Vec<Coverage> = units
        .iter()
        .map(|name| {
            tallies.get(name).cloned().unwrap_or(Coverage {
                covered: 0,
                total: 0,
            })
        })
        .collect();
    if state.order == Order::Shared && selected.is_some() {
        let mut lines: Vec<Line> = entries
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Path(line) => Some(line),
                Entry::Group { .. } => None,
            })
            .filter(|line| line.shared > 0 || line.relation != Relation::None)
            .collect();
        lines.sort_by(|a, b| {
            let rank = |line: &Line| match line.relation {
                Relation::Itself => 0,
                _ => 1,
            };
            rank(a)
                .cmp(&rank(b))
                .then(b.shared.cmp(&a.shared))
                .then(a.name.cmp(&b.name))
        });
        entries = lines
            .into_iter()
            .map(|mut line| {
                line.depth = 0;
                Entry::Path(line)
            })
            .collect();
    }
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by_key(|at| units.get(*at).map_or(usize::MAX, |name| layers.rank(name)));
    let pick = |values: &[usize]| -> Vec<usize> {
        order
            .iter()
            .map(|at| values.get(*at).copied().unwrap_or(0))
            .collect()
    };
    let units: Vec<String> = order
        .iter()
        .filter_map(|at| units.get(*at).cloned())
        .collect();
    let coverage: Vec<Coverage> = order
        .iter()
        .filter_map(|at| coverage.get(*at).cloned())
        .collect();
    let paths_per_unit = pick(&paths_per_unit);
    for entry in &mut entries {
        if let Entry::Path(line) = entry {
            line.counts = pick(&line.counts);
            line.first = order
                .iter()
                .map(|at| line.first.get(*at).copied().flatten())
                .collect();
        }
    }
    let level = |name: &str| layers.level.get(name).copied().unwrap_or(0);
    let levels: Vec<usize> = units.iter().map(|name| level(name)).collect();
    if let Some(column) = state
        .column
        .as_ref()
        .and_then(|name| units.iter().position(|unit| unit == name))
    {
        let mut lines: Vec<Line> = entries
            .into_iter()
            .filter_map(|entry| match entry {
                Entry::Path(line) => Some(line),
                Entry::Group { .. } => None,
            })
            .filter(|line| line.counts.get(column).copied().unwrap_or(0) > 0)
            .collect();
        lines.sort_by(|a, b| {
            let count = |line: &Line| line.counts.get(column).copied().unwrap_or(0);
            count(b).cmp(&count(a)).then(a.name.cmp(&b.name))
        });
        entries = lines
            .into_iter()
            .map(|mut line| {
                line.depth = 0;
                Entry::Path(line)
            })
            .collect();
    }
    Atlas {
        units,
        coverage,
        paths_per_unit,
        entries,
        levels,
    }
}
