use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::{Duration, Instant};

use domain::{
    Depth, FileId, GroupName, Index, Line, LineCount, Map, Path, PathDiff, PathKind, PathName, Row,
    Span, Step, StepId,
};
use io_map::{MapStore, Stamp};
use strum::VariantArray;
use ui::{Count, Extent, FontSize, Id, Label, Px};

use crate::authoring::StepGrab;
use crate::field::{Fields, Which};
use crate::graph::GraphState;
use crate::nav::Nav;
use crate::panels::Panels;
use crate::peek::{Peek, Queries};
use crate::status::{OutputLog, Status};
use crate::text::Needle;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
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
        Self::VARIANTS
            .iter()
            .copied()
            .find(|tab| tab.name().as_str() == name.as_str())
            .unwrap_or(Self::Path)
    }
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

    pub(crate) fn customized(&self) -> impl Iterator<Item = (StepKey, StepView)> + '_ {
        self.0
            .iter()
            .filter(|(_, view)| **view != StepView::default())
            .map(|(key, view)| (*key, *view))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StepShape {
    pub(crate) view: StepView,
    pub(crate) span: Option<Span>,
    pub(crate) note: Count,
    pub(crate) width: Px,
    pub(crate) row: Px,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Measured {
    pub(crate) shape: StepShape,
    pub(crate) height: Px,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Measures(BTreeMap<Id, Measured>);

impl Measures {
    pub(crate) fn height(&self, id: Id, shape: StepShape) -> Option<Px> {
        self.0
            .get(&id)
            .filter(|measured| measured.shape == shape)
            .map(|measured| measured.height)
    }

    pub(crate) fn set(&mut self, id: Id, measured: Measured) {
        self.0.insert(id, measured);
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

pub(crate) const HIT_LIMIT: Count = Count::new(5000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HitsShown {
    All,
    First,
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
    pub(crate) panels: Panels,
    pub(crate) scrolls: Scrolls,
    pub(crate) across: Scrolls,
    pub(crate) measures: Measures,
    pub(crate) graph: GraphState,
    pub(crate) tip_shown: Option<Label>,
    pub(crate) fields: Fields,
    pub(crate) new_path: Option<PathKind>,
    pub(crate) step_grab: Option<StepGrab>,
    pub(crate) results: Vec<Hit>,
    pub(crate) hits_shown: HitsShown,
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
            panels: Panels::default(),
            scrolls: Scrolls::default(),
            across: Scrolls::default(),
            measures: Measures::default(),
            graph: GraphState::default(),
            tip_shown: None,
            fields: Fields::default(),
            new_path: None,
            step_grab: None,
            results: Vec::new(),
            hits_shown: HitsShown::All,
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

    fn path_filter(&self) -> Needle {
        Needle::new(self.fields.get(Which::PathFilter).text().as_str())
    }

    pub(crate) fn lists(&self, name: &PathName) -> bool {
        self.path_filter().found_in(name.as_str())
    }

    fn step_found(needle: &Needle, step: &Step) -> bool {
        step.symbol()
            .is_some_and(|symbol| needle.found_in(symbol.as_str()))
            || needle.found_in(step.file().as_str())
    }

    pub(crate) fn listed_rows(&self) -> Vec<Row> {
        let needle = self.path_filter();
        self.map.rows_where(|path| {
            needle.found_in(path.name().as_str())
                || path
                    .steps()
                    .iter()
                    .any(|step| Self::step_found(&needle, step))
        })
    }

    pub(crate) fn found_steps(&self, slot: PathSlot) -> Vec<Numbered> {
        let needle = self.path_filter();
        let Some(path) = self.path(slot) else {
            return Vec::new();
        };
        if needle.is_empty() || needle.found_in(path.name().as_str()) {
            return Vec::new();
        }
        self.numbered(slot)
            .into_iter()
            .filter(|numbered| {
                path.steps()
                    .get(numbered.step.get())
                    .is_some_and(|step| Self::step_found(&needle, step))
            })
            .collect()
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
