use std::collections::BTreeSet;

use domain::{
    AddedStep, AddedSteps, Author, Cut, Depth, Draft, EditChange, GroupName, Index, Map, Note,
    Planned, StepId, StepNote, StepNumber, Stop, SymbolId, Tour, TourEdit, TourKind, TourName,
    TreeEntry, Verdict,
};
use strum::VariantArray;
use ui::{Count, Label, Px};

use crate::app::App;
use crate::field::Which;
use crate::ids;
use crate::model::{Dirty, Model, StepSlot, Tab, TourSlot};
use crate::status::{Held, Status};
use crate::welcome::Spell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, VariantArray)]
pub(crate) enum Page {
    Name,
    Start,
    Steps,
    Note,
    Create,
}

impl Page {
    pub(crate) const fn word(self) -> Spell {
        Spell::new(match self {
            Self::Name => "Name",
            Self::Start => "Start",
            Self::Steps => "Steps",
            Self::Note => "Note",
            Self::Create => "Create",
        })
    }

    pub(crate) fn number(self) -> Count {
        Count::new(
            Self::VARIANTS
                .iter()
                .position(|page| *page == self)
                .unwrap_or(0)
                + 1,
        )
    }

    const fn next(self) -> Option<Self> {
        match self {
            Self::Name => Some(Self::Start),
            Self::Start => Some(Self::Steps),
            Self::Steps => Some(Self::Note),
            Self::Note => Some(Self::Create),
            Self::Create => None,
        }
    }

    const fn back(self) -> Option<Self> {
        match self {
            Self::Name => None,
            Self::Start => Some(Self::Name),
            Self::Steps => Some(Self::Start),
            Self::Note => Some(Self::Steps),
            Self::Create => Some(Self::Note),
        }
    }

    const fn field(self) -> Option<Which> {
        match self {
            Self::Name => Some(Which::WizardName),
            Self::Start => Some(Which::WizardSearch),
            Self::Note => Some(Which::WizardNote),
            Self::Steps | Self::Create => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tick {
    Ticked,
    Unticked,
}

pub(crate) const fn verdict_words(verdict: Verdict) -> Spell {
    Spell::new(match verdict {
        Verdict::Kept => "",
        Verdict::Cut(Cut::Test) => "test",
        Verdict::Cut(Cut::Accessor) => "accessor",
        Verdict::Cut(Cut::Trivial) => "trivial body",
        Verdict::Stopped(Stop::Mapped) => "leaf: in another tour",
        Verdict::Stopped(Stop::Shared) => "leaf: shared by many callers",
        Verdict::Stopped(Stop::OtherPackage) => "leaf: another package",
        Verdict::Cycle => "calls back into",
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct BranchId(Count);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Subject {
    Call(Planned),
    Lines,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StepState {
    New,
    Existing { id: StepId, number: StepNumber },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Loaded {
    No,
    Yes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shown {
    Open,
    Closed,
}

impl Shown {
    const fn flipped(self) -> Self {
        match self {
            Self::Open => Self::Closed,
            Self::Closed => Self::Open,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Expander {
    Leaf,
    Closed,
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Branch {
    pub(crate) subject: Subject,
    pub(crate) state: StepState,
    parent: Option<BranchId>,
    children: Vec<BranchId>,
    loaded: Loaded,
    shown: Shown,
    depth: Depth,
    pub(crate) tick: Tick,
}

impl Branch {
    pub(crate) const fn symbol(&self) -> Option<SymbolId> {
        match self.subject {
            Subject::Call(planned) => Some(planned.entry.symbol),
            Subject::Lines => None,
        }
    }

    pub(crate) const fn verdict(&self) -> Verdict {
        match self.subject {
            Subject::Call(planned) => planned.verdict,
            Subject::Lines => Verdict::Kept,
        }
    }

    pub(crate) const fn depth(&self) -> Depth {
        self.depth
    }

    pub(crate) const fn step(&self) -> Option<&StepId> {
        match &self.state {
            StepState::Existing { id, .. } => Some(id),
            StepState::New => None,
        }
    }

    const fn fixed(&self) -> bool {
        matches!(self.subject, Subject::Call(planned) if matches!(planned.verdict, Verdict::Cycle))
            || (self.parent.is_none() && matches!(self.state, StepState::New))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fold {
    pub(crate) parent: BranchId,
    pub(crate) cut: Cut,
    pub(crate) members: Vec<BranchId>,
    pub(crate) shown: Shown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Line {
    Branch(BranchId),
    Fold(Fold),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutlineRow {
    pub(crate) line: Line,
    pub(crate) expander: Expander,
    pub(crate) more: Option<Count>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    Visible,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FoldKey {
    parent: BranchId,
    cut: Cut,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Outline {
    branches: Vec<Branch>,
    open_folds: BTreeSet<FoldKey>,
}

const FOLDED_AT: Count = Count::new(2);

const fn ticked_under(above: Tick, verdict: Verdict) -> Tick {
    match (above, verdict) {
        (Tick::Ticked, Verdict::Kept | Verdict::Stopped(_)) => Tick::Ticked,
        _ => Tick::Unticked,
    }
}

impl Outline {
    fn push(&mut self, branch: Branch) -> BranchId {
        let id = BranchId(Count::new(self.branches.len()));
        if let Some(above) = branch.parent.and_then(|parent| self.branch_mut(parent)) {
            above.children.push(id);
        }
        self.branches.push(branch);
        id
    }

    fn of(planned: &[Planned]) -> Self {
        let mut outline = Self::default();
        let mut stack: Vec<BranchId> = Vec::new();
        let mut depths = Vec::new();
        for entry in planned {
            while depths
                .last()
                .is_some_and(|depth| *depth >= entry.entry.depth)
            {
                depths.pop();
                stack.pop();
            }
            let walked = entry.verdict == Verdict::Kept && entry.entry.depth < Map::PROMOTE_DEPTH;
            let parent = stack.last().copied();
            let above = parent
                .and_then(|id| outline.branch(id))
                .map_or(Tick::Ticked, |branch| branch.tick);
            let id = outline.push(Branch {
                subject: Subject::Call(*entry),
                state: StepState::New,
                parent,
                children: Vec::new(),
                loaded: if walked { Loaded::Yes } else { Loaded::No },
                shown: if walked { Shown::Open } else { Shown::Closed },
                depth: entry.entry.depth,
                tick: ticked_under(above, entry.verdict),
            });
            stack.push(id);
            depths.push(entry.entry.depth);
        }
        outline
    }

    fn of_tour(tour: &Tour, index: &Index) -> Self {
        let mut outline = Self::default();
        let mut stack: Vec<BranchId> = Vec::new();
        for numbered in tour.numbered(index) {
            let Some(step) = tour.step(&numbered.step) else {
                continue;
            };
            stack.truncate(usize::try_from(numbered.depth.value()).unwrap_or(0));
            let subject = step.resolved_symbol().map_or(Subject::Lines, |symbol| {
                Subject::Call(Planned {
                    entry: TreeEntry {
                        symbol,
                        depth: numbered.depth,
                    },
                    verdict: Verdict::Kept,
                })
            });
            let id = outline.push(Branch {
                subject,
                state: StepState::Existing {
                    id: numbered.step,
                    number: numbered.number,
                },
                parent: stack.last().copied(),
                children: Vec::new(),
                loaded: Loaded::No,
                shown: Shown::Open,
                depth: numbered.depth,
                tick: Tick::Ticked,
            });
            stack.push(id);
        }
        for branch in &mut outline.branches {
            if branch.children.is_empty() {
                branch.shown = Shown::Closed;
            }
        }
        outline
    }

    pub(crate) fn branch(&self, id: BranchId) -> Option<&Branch> {
        self.branches.get(id.0.get())
    }

    fn branch_mut(&mut self, id: BranchId) -> Option<&mut Branch> {
        self.branches.get_mut(id.0.get())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.branches.is_empty()
    }

    fn children(&self, id: BranchId) -> &[BranchId] {
        self.branch(id).map_or(&[], |branch| &branch.children)
    }

    fn roots(&self) -> Vec<BranchId> {
        self.branches
            .iter()
            .enumerate()
            .filter(|pair| pair.1.parent.is_none())
            .map(|pair| BranchId(Count::new(pair.0)))
            .collect()
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = BranchId> {
        (0..self.branches.len()).map(|position| BranchId(Count::new(position)))
    }

    pub(crate) fn path(&self, id: BranchId) -> Vec<SymbolId> {
        let mut path = Vec::new();
        let mut at = Some(id);
        while let Some(branch) = at.and_then(|here| self.branch(here)) {
            path.extend(branch.symbol());
            at = branch.parent;
        }
        path.reverse();
        path
    }

    pub(crate) fn placed(&self) -> BTreeSet<SymbolId> {
        self.branches.iter().filter_map(Branch::symbol).collect()
    }

    fn waiting(&self, id: BranchId, index: &Index, placed: &BTreeSet<SymbolId>) -> Count {
        let loadable = self.branch(id).is_some_and(|branch| {
            branch.loaded == Loaded::No
                && branch.symbol().is_some()
                && branch.verdict() != Verdict::Cycle
        });
        if !loadable {
            return Count::ZERO;
        }
        Count::new(index.unplaced_callees(&self.path(id), placed).len())
    }

    fn branch_row(&self, id: BranchId, index: &Index, placed: &BTreeSet<SymbolId>) -> OutlineRow {
        let line = Line::Branch(id);
        let Some(branch) = self.branch(id) else {
            return OutlineRow {
                line,
                expander: Expander::Leaf,
                more: None,
            };
        };
        let waiting = self.waiting(id, index, placed);
        let has_children = !branch.children.is_empty();
        let expander = match (has_children || waiting > Count::ZERO, branch.shown) {
            (false, _) => Expander::Leaf,
            (true, Shown::Open) => Expander::Open,
            (true, Shown::Closed) => Expander::Closed,
        };
        let more = (has_children && branch.shown == Shown::Open && waiting > Count::ZERO)
            .then_some(waiting);
        OutlineRow {
            line,
            expander,
            more,
        }
    }

    fn rows(&self, index: &Index) -> Vec<OutlineRow> {
        let placed = self.placed();
        self.lines()
            .into_iter()
            .map(|line| match line {
                Line::Branch(id) => self.branch_row(id, index, &placed),
                Line::Fold(fold) => OutlineRow {
                    expander: match fold.shown {
                        Shown::Open => Expander::Open,
                        Shown::Closed => Expander::Closed,
                    },
                    line: Line::Fold(fold),
                    more: None,
                },
            })
            .collect()
    }

    fn planned_tick(&self, id: BranchId) -> Tick {
        let Some(branch) = self.branch(id) else {
            return Tick::Unticked;
        };
        let above = branch
            .parent
            .map_or(Tick::Ticked, |parent| self.planned_tick(parent));
        ticked_under(above, branch.verdict())
    }

    fn touched(&self) -> bool {
        self.ids().any(|id| {
            self.branch(id)
                .is_some_and(|branch| branch.tick != self.planned_tick(id))
        })
    }

    fn expand(&mut self, id: BranchId, index: &Index, map: &Map, mode: &Mode) {
        let Some(branch) = self.branch(id) else {
            return;
        };
        if branch.children.is_empty() && branch.loaded == Loaded::No {
            self.load(id, index, map, mode);
        } else if let Some(opened) = self.branch_mut(id) {
            opened.shown = opened.shown.flipped();
        }
    }

    fn load(&mut self, id: BranchId, index: &Index, map: &Map, mode: &Mode) {
        let Some(branch) = self.branch(id) else {
            return;
        };
        if branch.loaded == Loaded::Yes
            || branch.symbol().is_none()
            || branch.verdict() == Verdict::Cycle
        {
            return;
        }
        let tick = branch.tick;
        let depth = branch.depth.deeper();
        let callees =
            map.plan_promotion_callees(index, mode.besides(), &self.path(id), &self.placed());
        for mut planned in callees {
            planned.entry.depth = depth;
            let _child = self.push(Branch {
                subject: Subject::Call(planned),
                state: StepState::New,
                parent: Some(id),
                children: Vec::new(),
                loaded: Loaded::No,
                shown: Shown::Closed,
                depth,
                tick: ticked_under(tick, planned.verdict),
            });
        }
        if let Some(loaded) = self.branch_mut(id) {
            loaded.loaded = Loaded::Yes;
            loaded.shown = Shown::Open;
        }
    }

    pub(crate) fn toggle(&mut self, id: BranchId) {
        let Some(branch) = self.branch(id) else {
            return;
        };
        if branch.fixed() {
            return;
        }
        match branch.tick {
            Tick::Ticked => self.untick_below(id),
            Tick::Unticked => {
                let mut at = Some(id);
                while let Some(above) = at.and_then(|here| self.branch_mut(here)) {
                    above.tick = Tick::Ticked;
                    at = above.parent;
                }
            }
        }
    }

    fn untick_below(&mut self, id: BranchId) {
        let mut pending = vec![id];
        while let Some(next) = pending.pop() {
            pending.extend_from_slice(self.children(next));
            if let Some(branch) = self.branch_mut(next) {
                branch.tick = Tick::Unticked;
            }
        }
    }

    fn toggle_fold(&mut self, parent: BranchId, cut: Cut) {
        let key = FoldKey { parent, cut };
        if !self.open_folds.remove(&key) {
            self.open_folds.insert(key);
        }
    }

    fn walk(&self, id: BranchId, reach: Reach, out: &mut Vec<Line>) {
        let Some(branch) = self.branch(id) else {
            return;
        };
        out.push(Line::Branch(id));
        if reach == Reach::Visible && branch.shown == Shown::Closed {
            return;
        }
        let children = self.children(id);
        let cut_of = |child: &BranchId| match self.branch(*child).map(Branch::verdict) {
            Some(Verdict::Cut(cut)) => Some(cut),
            _ => None,
        };
        for child in children.iter().filter(|child| cut_of(child).is_none()) {
            self.walk(*child, reach, out);
        }
        let cuts: BTreeSet<Cut> = children.iter().filter_map(cut_of).collect();
        for cut in cuts {
            let members: Vec<BranchId> = children
                .iter()
                .copied()
                .filter(|child| cut_of(child) == Some(cut))
                .collect();
            let folded = members.len() >= FOLDED_AT.get();
            let shown = if self.open_folds.contains(&FoldKey { parent: id, cut }) {
                Shown::Open
            } else {
                Shown::Closed
            };
            if folded && reach == Reach::Visible {
                out.push(Line::Fold(Fold {
                    parent: id,
                    cut,
                    members: members.clone(),
                    shown,
                }));
                if shown == Shown::Closed {
                    continue;
                }
            }
            for member in members {
                self.walk(member, reach, out);
            }
        }
    }

    fn walked(&self, reach: Reach) -> Vec<Line> {
        let mut out = Vec::new();
        for root in self.roots() {
            self.walk(root, reach, &mut out);
        }
        out
    }

    pub(crate) fn lines(&self) -> Vec<Line> {
        self.walked(Reach::Visible)
    }

    fn every(&self) -> Vec<&Branch> {
        self.walked(Reach::All)
            .into_iter()
            .filter_map(|line| match line {
                Line::Branch(id) => self.branch(id),
                Line::Fold(_) => None,
            })
            .collect()
    }

    pub(crate) fn ticked(&self) -> Vec<TreeEntry> {
        self.every()
            .into_iter()
            .filter(|branch| branch.tick == Tick::Ticked)
            .filter_map(|branch| {
                Some(TreeEntry {
                    symbol: branch.symbol()?,
                    depth: branch.depth,
                })
            })
            .collect()
    }

    fn removed(&self) -> Vec<StepId> {
        self.every()
            .into_iter()
            .filter(|branch| branch.tick == Tick::Unticked)
            .filter_map(|branch| branch.step().cloned())
            .collect()
    }

    fn kept_notes(&self, note_of: &impl Fn(BranchId) -> Option<Note>) -> Vec<StepNote> {
        self.ids()
            .filter_map(|id| {
                let branch = self.branch(id)?;
                (branch.tick == Tick::Ticked).then_some(())?;
                Some(StepNote {
                    step: branch.step()?.clone(),
                    note: note_of(id),
                })
            })
            .collect()
    }

    fn added_step(
        &self,
        id: BranchId,
        note_of: &impl Fn(BranchId) -> Option<Note>,
    ) -> Option<AddedStep> {
        let branch = self.branch(id)?;
        if branch.state != StepState::New || branch.tick == Tick::Unticked {
            return None;
        }
        Some(AddedStep {
            symbol: branch.symbol()?,
            note: note_of(id),
            below: branch
                .children
                .iter()
                .filter_map(|child| self.added_step(*child, note_of))
                .collect(),
        })
    }

    fn added_steps(&self, note_of: &impl Fn(BranchId) -> Option<Note>) -> Vec<AddedSteps> {
        self.ids()
            .filter_map(|id| {
                let branch = self.branch(id)?;
                let under = branch.step()?.clone();
                let steps: Vec<AddedStep> = branch
                    .children
                    .iter()
                    .filter_map(|child| self.added_step(*child, note_of))
                    .collect();
                (!steps.is_empty()).then_some(AddedSteps { under, steps })
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Build,
    EditTour(TourName),
}

impl Mode {
    const fn besides(&self) -> Option<&TourName> {
        match self {
            Self::Build => None,
            Self::EditTour(name) => Some(name),
        }
    }
}

const TYPED: [Which; 4] = [
    Which::WizardName,
    Which::WizardGroup,
    Which::WizardSearch,
    Which::WizardNote,
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Baseline {
    kind: TourKind,
    start: Option<SymbolId>,
    texts: Vec<Label>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Derived {
    rows: Vec<OutlineRow>,
    pending: Option<Pending>,
    changes: Vec<EditChange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wizard {
    page: Page,
    kind: TourKind,
    start: Option<SymbolId>,
    seen: Option<SymbolId>,
    outline: Outline,
    refusal: Option<Label>,
    mode: Mode,
    baseline: Option<Baseline>,
    derived: Derived,
}

impl Wizard {
    pub(crate) fn rows(&self) -> &[OutlineRow] {
        &self.derived.rows
    }

    pub(crate) fn changes(&self) -> &[EditChange] {
        &self.derived.changes
    }

    pub(crate) fn invalid(&self) -> Option<&Label> {
        self.derived
            .pending
            .as_ref()
            .and_then(|pending| pending.edit.as_ref().err())
    }

    pub(crate) const fn page(&self) -> Page {
        self.page
    }

    pub(crate) const fn mode(&self) -> &Mode {
        &self.mode
    }

    pub(crate) const fn kind(&self) -> TourKind {
        self.kind
    }

    pub(crate) const fn start(&self) -> Option<SymbolId> {
        self.start
    }

    pub(crate) const fn outline(&self) -> &Outline {
        &self.outline
    }

    pub(crate) const fn refusal(&self) -> Option<&Label> {
        self.refusal.as_ref()
    }

    fn choose(&mut self, index: &Index, map: &Map, symbol: SymbolId) {
        self.refusal = None;
        if self.start == Some(symbol) {
            return;
        }
        self.start = Some(symbol);
        self.outline = Outline::of(&map.plan_promotion(index, symbol));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WizardAct {
    Start,
    FromHere(SymbolId),
    Next,
    Back,
    Cancel,
    Kind(TourKind),
    Choose(SymbolId),
    UseFocus,
    Toggle(BranchId),
    Expand(BranchId),
    Fold(BranchId, Cut),
    More(BranchId),
    Return,
    Edit(TourSlot),
    Apply,
    Escape,
}

impl Model {
    pub(crate) fn follow_focus(&mut self) {
        let focus = self.nav.focus();
        let Some(wizard) = self.wizard.as_mut() else {
            return;
        };
        if wizard.seen == focus {
            return;
        }
        wizard.seen = focus;
        if wizard.page == Page::Start
            && let Some(symbol) = focus
        {
            wizard.choose(&self.index, &self.map, symbol);
            self.refresh_wizard();
        }
    }

    pub(crate) fn typed_name(&self) -> Label {
        Label::new(self.fields.get(Which::WizardName).text().as_str().trim())
    }

    pub(crate) fn wizard_edited(&mut self) {
        if let Some(wizard) = self.wizard.as_mut() {
            wizard.refusal = None;
        }
        self.refresh_wizard();
    }

    fn refuse(&mut self, why: Label) {
        if let Some(wizard) = self.wizard.as_mut() {
            wizard.refusal = Some(why);
        }
    }

    fn name_refusal(&self) -> Option<Label> {
        let typed = self.typed_name();
        if typed.as_str().is_empty() {
            return Some(Label::new("type a name for the tour"));
        }
        TourName::new(typed.as_str())
            .and_then(|name| self.map.check_name_free(&name))
            .err()
            .map(|error| Label::new(cli::map_failure(&self.map, error).to_string()))
    }

    fn show_page(&mut self, page: Page) {
        let Some(wizard) = self.wizard.as_mut() else {
            return;
        };
        wizard.page = page;
        wizard.refusal = None;
        for which in [Which::WizardName, Which::WizardSearch, Which::WizardNote] {
            self.fields.release(which);
        }
        if let Some(which) = page.field() {
            self.fields.focus(which);
        }
    }

    pub(crate) fn close_wizard(&mut self) {
        self.wizard = None;
        for which in [
            Which::WizardName,
            Which::WizardGroup,
            Which::WizardSearch,
            Which::WizardNote,
        ] {
            self.fields.release(which);
        }
        self.fields.drop_step_notes();
    }

    fn note_in(&self, which: Which) -> Option<Note> {
        let text = self.fields.get(which).text().as_str();
        if text.trim().is_empty() {
            None
        } else {
            Note::new(text)
        }
    }

    fn edit_name(&self, tour: &TourName) -> Result<TourName, Label> {
        let typed = self.typed_name();
        if typed.as_str().is_empty() {
            return Err(Label::new("type a name for the tour"));
        }
        let refused = |error| Label::new(cli::map_failure(&self.map, error).to_string());
        let name = TourName::new(typed.as_str()).map_err(refused)?;
        if !name.same_letters(tour) {
            let _free = self.map.check_name_free(&name).map_err(refused)?;
        }
        Ok(name)
    }

    fn plan_edit(&self) -> Option<Pending> {
        let wizard = self.wizard.as_ref()?;
        let Mode::EditTour(tour) = &wizard.mode else {
            return None;
        };
        let note_of = |id: BranchId| self.note_in(Which::StepNote(id));
        let edit = self.edit_name(tour).map(|name| TourEdit {
            name,
            kind: wizard.kind,
            group: GroupName::new(self.fields.get(Which::WizardGroup).text().as_str()),
            note: self.note_in(Which::WizardNote),
            step_notes: wizard.outline.kept_notes(&note_of),
            removed: wizard.outline.removed(),
            added: wizard.outline.added_steps(&note_of),
            author: Author::Human,
        });
        Some(Pending {
            tour: tour.clone(),
            edit,
        })
    }

    pub(crate) fn refresh_wizard(&mut self) {
        let Some(wizard) = self.wizard.as_ref() else {
            return;
        };
        let rows = wizard.outline.rows(&self.index);
        let pending = self.plan_edit();
        let changes = pending
            .as_ref()
            .and_then(|planned| {
                let edit = planned.edit.as_ref().ok()?;
                Some(self.map.tour_edit_changes(&planned.tour, edit))
            })
            .unwrap_or_default();
        if let Some(open) = self.wizard.as_mut() {
            open.derived = Derived {
                rows,
                pending,
                changes,
            };
        }
    }

    fn typed_texts(&self) -> Vec<Label> {
        TYPED
            .iter()
            .map(|which| self.fields.get(*which).text().label())
            .collect()
    }

    fn baseline_now(&self) -> Option<Baseline> {
        let wizard = self.wizard.as_ref()?;
        Some(Baseline {
            kind: wizard.kind,
            start: wizard.start,
            texts: self.typed_texts(),
        })
    }

    fn mark_baseline(&mut self) {
        let baseline = self.baseline_now();
        if let Some(wizard) = self.wizard.as_mut() {
            wizard.baseline = baseline;
        }
    }

    pub(crate) fn held(&self) -> Option<Held> {
        let wizard = self.wizard.as_ref()?;
        match &wizard.mode {
            Mode::EditTour(_) => {
                if wizard.invalid().is_some() {
                    return Some(Held::Unapplied);
                }
                let count = wizard.changes().len();
                (count > 0).then_some(Held::Changes(Count::new(count)))
            }
            Mode::Build => {
                let entered = wizard.outline.touched()
                    || wizard.baseline.as_ref() != self.baseline_now().as_ref();
                entered.then_some(Held::Unbuilt)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    tour: TourName,
    edit: Result<TourEdit, Label>,
}

impl App {
    pub(crate) fn wizard(&mut self, act: WizardAct) {
        self.wizard_act(act);
        self.model.refresh_wizard();
    }

    fn wizard_act(&mut self, act: WizardAct) {
        let model = &mut self.model;
        match act {
            WizardAct::Start => self.open_wizard(),
            WizardAct::FromHere(symbol) => self.tour_from_here(symbol),
            WizardAct::Next => {
                if model
                    .wizard
                    .as_ref()
                    .is_some_and(|open| open.mode == Mode::Build)
                {
                    self.next_page();
                }
            }
            WizardAct::Edit(tour) => self.open_edit(tour),
            WizardAct::Apply => self.apply_edit(),
            WizardAct::Back => {
                if let Some(page) = model.wizard.as_ref().and_then(|open| open.page.back()) {
                    model.show_page(page);
                }
            }
            WizardAct::Cancel => model.close_wizard(),
            WizardAct::Escape => match model.held() {
                None => model.close_wizard(),
                Some(held) => model.status = Status::WizardHeld(held),
            },
            WizardAct::Return => model.set_tab(Tab::Tour),
            WizardAct::Kind(kind) => {
                if let Some(wizard) = model.wizard.as_mut() {
                    wizard.kind = kind;
                }
            }
            WizardAct::Choose(symbol) => {
                if let Some(wizard) = model.wizard.as_mut() {
                    wizard.choose(&model.index, &model.map, symbol);
                }
            }
            WizardAct::UseFocus => {
                if let (Some(wizard), Some(symbol)) = (model.wizard.as_mut(), model.nav.focus()) {
                    wizard.choose(&model.index, &model.map, symbol);
                }
            }
            WizardAct::Toggle(branch) => {
                if let Some(wizard) = model.wizard.as_mut() {
                    wizard.outline.toggle(branch);
                }
            }
            WizardAct::Expand(branch) => {
                if let Some(wizard) = model.wizard.as_mut() {
                    wizard
                        .outline
                        .expand(branch, &model.index, &model.map, &wizard.mode);
                }
            }
            WizardAct::More(branch) => {
                if let Some(wizard) = model.wizard.as_mut() {
                    wizard
                        .outline
                        .load(branch, &model.index, &model.map, &wizard.mode);
                }
            }
            WizardAct::Fold(parent, cut) => {
                if let Some(wizard) = model.wizard.as_mut() {
                    wizard.outline.toggle_fold(parent, cut);
                }
            }
        }
    }

    fn open_wizard(&mut self) {
        let model = &mut self.model;
        let group = model
            .nav
            .tour()
            .and_then(|tour| model.tour(tour))
            .and_then(|tour| tour.group())
            .map_or("", GroupName::as_str);
        model.fields.fill(Which::WizardGroup, &Label::new(group));
        for which in [Which::WizardName, Which::WizardSearch, Which::WizardNote] {
            model.fields.fill(which, &Label::default());
        }
        let focus = model.nav.focus();
        let mut wizard = Wizard {
            page: Page::Name,
            kind: TourKind::Flow,
            start: None,
            seen: focus,
            outline: Outline::default(),
            refusal: None,
            mode: Mode::Build,
            baseline: None,
            derived: Derived::default(),
        };
        if let Some(symbol) = focus {
            wizard.choose(&model.index, &model.map, symbol);
        }
        model.wizard = Some(wizard);
        model.show_page(Page::Name);
        model.set_tab(Tab::Tour);
        model.mark_baseline();
    }

    fn tour_from_here(&mut self, symbol: SymbolId) {
        self.open_wizard();
        let model = &mut self.model;
        let Some(found) = model.index.symbol(symbol) else {
            return;
        };
        let name = TourName::from_symbol(found.name());
        model
            .fields
            .fill(Which::WizardName, &Label::new(name.as_str()));
        if let Some(wizard) = model.wizard.as_mut() {
            wizard.choose(&model.index, &model.map, symbol);
        }
        model.show_page(Page::Steps);
        model.mark_baseline();
    }

    fn next_page(&mut self) {
        let model = &mut self.model;
        let Some(page) = model.wizard.as_ref().map(Wizard::page) else {
            return;
        };
        let refusal = match page {
            Page::Name => model.name_refusal(),
            Page::Start => model
                .wizard
                .as_ref()
                .and_then(Wizard::start)
                .is_none()
                .then(|| Label::new("choose the symbol the tour starts from")),
            Page::Steps | Page::Note => None,
            Page::Create => {
                self.create_from_wizard();
                return;
            }
        };
        if let Some(why) = refusal {
            model.refuse(why);
            return;
        }
        if let Some(next) = page.next() {
            model.show_page(next);
        }
    }

    fn create_from_wizard(&mut self) {
        let model = &mut self.model;
        let Some(wizard) = model.wizard.as_ref() else {
            return;
        };
        let typed = model.typed_name();
        let entries = wizard.outline.ticked();
        let kind = wizard.kind;
        let group = GroupName::new(model.fields.get(Which::WizardGroup).text().as_str());
        let note = Note::new(model.fields.get(Which::WizardNote).text().as_str().trim());
        let created = TourName::new(typed.as_str()).and_then(|name| {
            let draft = Draft {
                name: name.clone(),
                kind,
                group,
                note,
                author: Author::Human,
            };
            model
                .map
                .add_drafted_tour(&model.index, draft, &entries)
                .map(|_| name)
        });
        let name = match created {
            Ok(name) => name,
            Err(error) => {
                let why = Label::new(cli::map_failure(&model.map, error).to_string());
                model.refuse(why);
                return;
            }
        };
        model.close_wizard();
        model.disk.dirty = Dirty::Unsaved;
        if let Some(slot) = model.find_tour(&name) {
            model.select_tour(slot);
            model.set_tab(Tab::Tour);
        }
        model.status = Status::TourCreated(name);
    }

    fn open_edit(&mut self, slot: TourSlot) {
        let model = &mut self.model;
        let Some(tour) = model.tour(slot).cloned() else {
            return;
        };
        model.close_wizard();
        model
            .fields
            .fill(Which::WizardName, &Label::new(tour.name().as_str()));
        model.fields.fill(
            Which::WizardGroup,
            &Label::new(tour.group().map_or("", GroupName::as_str)),
        );
        model.fields.fill(
            Which::WizardNote,
            &Label::new(tour.note().map_or("", Note::as_str)),
        );
        model.fields.fill(Which::WizardSearch, &Label::default());
        let outline = Outline::of_tour(&tour, &model.index);
        for id in outline.ids() {
            let note = outline
                .branch(id)
                .and_then(Branch::step)
                .and_then(|step| tour.step(step))
                .and_then(domain::Step::note)
                .map_or("", Note::as_str);
            model.fields.fill(Which::StepNote(id), &Label::new(note));
        }
        if model.nav.tour() != Some(slot) {
            model.select_tour(slot);
        }
        model.wizard = Some(Wizard {
            page: Page::Name,
            kind: tour.kind(),
            start: None,
            seen: model.nav.focus(),
            outline,
            refusal: None,
            mode: Mode::EditTour(tour.name().clone()),
            baseline: None,
            derived: Derived::default(),
        });
        model.scrolls.set(ids::wizard(), Px::ZERO);
        model.set_tab(Tab::Tour);
    }

    fn apply_edit(&mut self) {
        let model = &mut self.model;
        let Some(pending) = model.plan_edit() else {
            return;
        };
        let edit = match pending.edit {
            Ok(edit) => edit,
            Err(why) => {
                model.refuse(why);
                return;
            }
        };
        if model.map.tour_edit_changes(&pending.tour, &edit).is_empty() {
            model.close_wizard();
            return;
        }
        let name = edit.name.clone();
        let Some(slot) = model.find_tour(&pending.tour) else {
            return;
        };
        let before: Vec<StepId> = model
            .tour(slot)
            .map(|tour| tour.steps().iter().map(|step| step.id().clone()).collect())
            .unwrap_or_default();
        if let Err(error) = model.map.apply_tour_edit(&model.index, &pending.tour, edit) {
            let why = Label::new(cli::map_failure(&model.map, error).to_string());
            model.refuse(why);
            return;
        }
        let after: Vec<StepId> = model
            .tour(slot)
            .map(|tour| tour.steps().iter().map(|step| step.id().clone()).collect())
            .unwrap_or_default();
        model.steps_moved(slot, |step| {
            let id = before.get(step.get())?;
            after
                .iter()
                .position(|other| other == id)
                .map(StepSlot::new)
        });
        model.close_wizard();
        model.disk.dirty = Dirty::Unsaved;
        model.select_tour(slot);
        model.set_tab(Tab::Tour);
        model.status = Status::TourEdited(name);
    }
}
