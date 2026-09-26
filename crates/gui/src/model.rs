use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::{Duration, Instant};

use domain::{
    Depth, FileId, GroupName, Index, Line, LineCount, Map, Path, PathDiff, PathName, Row, Step,
    StepId,
};
use io_map::{MapStore, Stamp};
use ui::{Count, Extent, FontSize, Id, Label, Px};

use crate::dock::Dock;
use crate::field::{Fields, Which};
use crate::graph::GraphState;
use crate::nav::Nav;
use crate::peek::{Peek, Queries};
use crate::status::{OutputLog, Status};
use crate::theme;
use crate::work::WorkState;
use std::mem;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PathSlot(usize);

impl PathSlot {
    pub(crate) const fn new(position: usize) -> Self {
        Self(position)
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

impl fmt::Display for PathSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct StepSlot(usize);

impl StepSlot {
    pub(crate) const fn new(position: usize) -> Self {
        Self(position)
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

impl fmt::Display for StepSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct StepKey {
    pub(crate) path: PathSlot,
    pub(crate) step: StepSlot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tab {
    Path,
    Diff,
    Graph,
    Listing,
    Results,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TabName(String);

impl TabName {
    pub(crate) fn new(name: &str) -> Self {
        Self(name.to_owned())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Literal(&'static str);

impl Literal {
    const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Tab {
    const ALL: [Self; 5] = [
        Self::Path,
        Self::Diff,
        Self::Graph,
        Self::Listing,
        Self::Results,
    ];

    const fn name(self) -> Literal {
        Literal(match self {
            Self::Path => "path",
            Self::Diff => "diff",
            Self::Graph => "graph",
            Self::Listing => "listing",
            Self::Results => "results",
        })
    }

    pub(crate) fn from_name(name: &TabName) -> Self {
        Self::ALL
            .into_iter()
            .find(|tab| tab.name().as_str() == name.as_str())
            .unwrap_or(Self::Path)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeftTab {
    Paths,
    Symbols,
    Files,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LineSelection {
    pub(crate) from: Line,
    pub(crate) to: Line,
}

impl LineSelection {
    pub(crate) const fn one(line: Line) -> Self {
        Self {
            from: line,
            to: line,
        }
    }

    pub(crate) fn low(self) -> Line {
        self.from.min(self.to)
    }

    pub(crate) fn high(self) -> Line {
        self.from.max(self.to)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ViewFlags(u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewFlag {
    Whole,
    Hidden,
    Folded,
    Expanded,
}

impl ViewFlags {
    const fn bit(flag: ViewFlag) -> u8 {
        match flag {
            ViewFlag::Whole => 1,
            ViewFlag::Hidden => 2,
            ViewFlag::Folded => 4,
            ViewFlag::Expanded => 8,
        }
    }

    pub(crate) const fn has(self, flag: ViewFlag) -> bool {
        self.0 & Self::bit(flag) != 0
    }

    pub(crate) fn toggle(&mut self, flag: ViewFlag) {
        self.0 ^= Self::bit(flag);
    }

    pub(crate) fn set(&mut self, flag: ViewFlag, on: bool) {
        if on {
            self.0 |= Self::bit(flag);
        } else {
            self.0 &= !Self::bit(flag);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Context {
    pub(crate) above: LineCount,
    pub(crate) below: LineCount,
}

impl Context {
    pub(crate) const LINES: LineCount = LineCount::new(10);

    pub(crate) fn is_empty(self) -> bool {
        self.above.is_zero() && self.below.is_zero()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StepView {
    pub(crate) flags: ViewFlags,
    pub(crate) context: Context,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct StepViews(BTreeMap<StepKey, StepView>);

impl StepViews {
    pub(crate) fn get(&self, key: StepKey) -> StepView {
        self.0.get(&key).copied().unwrap_or_default()
    }

    pub(crate) fn entry(&mut self, key: StepKey) -> &mut StepView {
        self.0.entry(key).or_default()
    }

    pub(crate) fn existing(&mut self, key: StepKey) -> Option<&mut StepView> {
        self.0.get_mut(&key)
    }

    pub(crate) fn each_of_path(
        &mut self,
        path: PathSlot,
        mut change: impl FnMut(StepSlot, &mut StepView),
    ) {
        for (key, view) in &mut self.0 {
            if key.path == path {
                change(key.step, view);
            }
        }
    }

    pub(crate) fn remap(&mut self, moved: impl Fn(StepKey) -> Option<StepKey>) {
        self.0 = mem::take(&mut self.0)
            .into_iter()
            .filter_map(|(key, view)| moved(key).map(|key| (key, view)))
            .collect();
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Scrolls(BTreeMap<Id, Px>);

impl Scrolls {
    pub(crate) fn get(&self, id: Id) -> Px {
        self.0.get(&id).copied().unwrap_or(Px::ZERO)
    }

    pub(crate) fn set(&mut self, id: Id, offset: Px) {
        self.0.insert(id, offset);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Openness {
    Open,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hit {
    pub(crate) file: FileId,
    pub(crate) line: Line,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dirty {
    Clean,
    Unsaved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Readable {
    Reads,
    Broken,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Warned {
    Quiet,
    Warned,
}

#[derive(Clone, Debug)]
pub(crate) struct MapDisk {
    pub(crate) stamp: Option<Stamp>,
    pub(crate) readable: Readable,
    pub(crate) dirty: Dirty,
    pub(crate) last_poll: Instant,
    pub(crate) warned: Warned,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Base {
    pub(crate) map: Option<Map>,
    pub(crate) why: Label,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Metrics {
    pub(crate) font: FontSize,
    pub(crate) cell: Extent,
}

impl Metrics {
    pub(crate) const fn cell_width(self) -> Px {
        self.cell.width
    }

    pub(crate) const fn row_height(self) -> Px {
        self.cell.height
    }
}

pub(crate) struct Numbered {
    pub(crate) step: StepSlot,
    pub(crate) depth: Depth,
    pub(crate) number: Label,
}

pub(crate) struct Placed {
    pub(crate) step: StepSlot,
}

pub(crate) struct Model {
    pub(crate) index: Index,
    pub(crate) map: Map,
    pub(crate) store: MapStore,
    pub(crate) disk: MapDisk,
    pub(crate) base: Base,
    pub(crate) nav: Nav,
    pub(crate) views: StepViews,
    pub(crate) directories: BTreeSet<Label>,
    pub(crate) groups: BTreeMap<GroupName, Openness>,
    pub(crate) peek: Option<Peek>,
    pub(crate) queries: Queries,
    pub(crate) dock: Dock,
    pub(crate) scrolls: Scrolls,
    pub(crate) across: Scrolls,
    pub(crate) graph: GraphState,
    pub(crate) tip_shown: Option<Label>,
    pub(crate) fields: Fields,
    pub(crate) results: Vec<Hit>,
    pub(crate) output: OutputLog,
    pub(crate) output_bottom: Count,
    pub(crate) status: Status,
    pub(crate) work: WorkState,
    pub(crate) metrics: Metrics,
    pub(crate) now: Duration,
}

impl Model {
    pub(crate) fn new(index: Index, map: Map, store: MapStore, readable: Readable) -> Self {
        Self {
            index,
            map,
            disk: MapDisk {
                stamp: store.stamp(),
                readable,
                dirty: Dirty::Clean,
                last_poll: Instant::now(),
                warned: Warned::Quiet,
            },
            store,
            base: Base::default(),
            nav: Nav::default(),
            views: StepViews::default(),
            directories: BTreeSet::new(),
            groups: BTreeMap::new(),
            peek: None,
            queries: Queries::default(),
            dock: Dock::default(),
            scrolls: Scrolls::default(),
            across: Scrolls::default(),
            graph: GraphState::default(),
            tip_shown: None,
            fields: Fields::default(),
            results: Vec::new(),
            output: OutputLog::default(),
            output_bottom: Count::ZERO,
            status: Status::Nothing,
            work: WorkState::default(),
            metrics: Metrics {
                font: theme::start_font(),
                cell: theme::START_CELL,
            },
            now: Duration::ZERO,
        }
    }

    pub(crate) fn path(&self, slot: PathSlot) -> Option<&Path> {
        self.map.paths().get(slot.get())
    }

    pub(crate) fn step(&self, key: StepKey) -> Option<&Step> {
        self.path(key.path)?.steps().get(key.step.get())
    }

    pub(crate) fn path_count(&self) -> Count {
        Count::new(self.map.paths().len())
    }

    pub(crate) fn step_count(&self, path: PathSlot) -> Count {
        Count::new(self.path(path).map_or(0, |found| found.steps().len()))
    }

    pub(crate) fn lists(&self, name: &PathName) -> bool {
        let filter = self.fields.get(Which::PathFilter).text().as_str();
        name.as_str()
            .to_lowercase()
            .contains(&filter.to_lowercase())
    }

    pub(crate) fn listed_rows(&self) -> Vec<Row> {
        self.map.rows_where(|path| self.lists(path.name()))
    }

    pub(crate) fn find_path(&self, name: &PathName) -> Option<PathSlot> {
        self.map
            .paths()
            .iter()
            .position(|path| path.name() == name)
            .map(PathSlot::new)
    }

    pub(crate) fn step_slot(&self, path: PathSlot, id: &StepId) -> Option<StepSlot> {
        self.path(path)?
            .steps()
            .iter()
            .position(|step| step.id() == id)
            .map(StepSlot::new)
    }

    pub(crate) fn step_id(&self, key: StepKey) -> Option<StepId> {
        self.step(key).map(|step| step.id().clone())
    }

    pub(crate) fn numbered(&self, path: PathSlot) -> Vec<Numbered> {
        let Some(found) = self.path(path) else {
            return Vec::new();
        };
        found
            .numbered(&self.index)
            .into_iter()
            .filter_map(|numbered| {
                Some(Numbered {
                    step: self.step_slot(path, &numbered.step)?,
                    depth: numbered.depth,
                    number: Label::new(numbered.number.to_string()),
                })
            })
            .collect()
    }

    pub(crate) fn tree_order(&self, path: PathSlot) -> Vec<Placed> {
        let Some(found) = self.path(path) else {
            return Vec::new();
        };
        found
            .tree_order()
            .into_iter()
            .filter_map(|placed| {
                Some(Placed {
                    step: self.step_slot(path, &placed.step)?,
                })
            })
            .collect()
    }

    pub(crate) fn descendants(&self, key: StepKey) -> Count {
        let Some(id) = self.step_id(key) else {
            return Count::ZERO;
        };
        Count::new(
            self.path(key.path)
                .map_or(0, |found| found.descendants(&id).len()),
        )
    }

    pub(crate) fn number_of(&self, key: StepKey) -> Label {
        self.numbered(key.path)
            .into_iter()
            .find(|numbered| numbered.step == key.step)
            .map_or_else(Label::default, |numbered| numbered.number)
    }

    pub(crate) fn parent_of(&self, key: StepKey) -> Option<StepSlot> {
        let parent = self.step(key)?.parent()?;
        self.step_slot(key.path, parent)
    }

    pub(crate) fn diffs(&self) -> Vec<PathDiff> {
        self.base
            .map
            .as_ref()
            .map(|base| self.map.diff(base))
            .unwrap_or_default()
    }
}
