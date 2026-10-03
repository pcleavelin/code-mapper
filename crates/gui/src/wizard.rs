use std::collections::BTreeSet;

use domain::{
    Author, Cut, Draft, GroupName, Index, Map, Note, Planned, Stop, SymbolId, TourKind, TourName,
    TreeEntry, Verdict,
};
use strum::VariantArray;
use ui::{Count, Label};

use crate::app::App;
use crate::field::Which;
use crate::model::{Dirty, Model, Tab};
use crate::status::Status;
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

impl BranchId {
    const ROOT: Self = Self(Count::ZERO);
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
enum Callees {
    Unloaded,
    Loaded(Vec<BranchId>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Branch {
    pub(crate) planned: Planned,
    parent: Option<BranchId>,
    callees: Callees,
    shown: Shown,
    pub(crate) tick: Tick,
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

const fn ticked_at_first(verdict: Verdict) -> Tick {
    match verdict {
        Verdict::Kept | Verdict::Stopped(_) => Tick::Ticked,
        Verdict::Cut(_) | Verdict::Cycle => Tick::Unticked,
    }
}

impl Outline {
    fn of(planned: &[Planned]) -> Self {
        let mut outline = Self::default();
        let mut stack: Vec<BranchId> = Vec::new();
        let mut depths = Vec::new();
        for (row, entry) in planned.iter().enumerate() {
            let id = BranchId(Count::new(row));
            while depths
                .last()
                .is_some_and(|depth| *depth >= entry.entry.depth)
            {
                depths.pop();
                stack.pop();
            }
            let parent = stack.last().copied();
            let walked = entry.verdict == Verdict::Kept && entry.entry.depth < Map::PROMOTE_DEPTH;
            outline.branches.push(Branch {
                planned: *entry,
                parent,
                callees: if walked {
                    Callees::Loaded(Vec::new())
                } else {
                    Callees::Unloaded
                },
                shown: if walked { Shown::Open } else { Shown::Closed },
                tick: ticked_at_first(entry.verdict),
            });
            if let Some(Callees::Loaded(children)) = parent
                .and_then(|above| outline.branch_mut(above))
                .map(|branch| &mut branch.callees)
            {
                children.push(id);
            }
            stack.push(id);
            depths.push(entry.entry.depth);
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
        match self.branch(id).map(|branch| &branch.callees) {
            Some(Callees::Loaded(children)) => children,
            Some(Callees::Unloaded) | None => &[],
        }
    }

    pub(crate) fn path(&self, id: BranchId) -> Vec<SymbolId> {
        let mut path = Vec::new();
        let mut at = Some(id);
        while let Some(branch) = at.and_then(|here| self.branch(here)) {
            path.push(branch.planned.entry.symbol);
            at = branch.parent;
        }
        path.reverse();
        path
    }

    fn placed(&self) -> BTreeSet<SymbolId> {
        self.branches
            .iter()
            .map(|branch| branch.planned.entry.symbol)
            .collect()
    }

    pub(crate) fn expander(&self, id: BranchId, index: &Index) -> Expander {
        let Some(branch) = self.branch(id) else {
            return Expander::Leaf;
        };
        let has_callees = match &branch.callees {
            _ if branch.planned.verdict == Verdict::Cycle => false,
            Callees::Loaded(children) => !children.is_empty(),
            Callees::Unloaded => !index
                .unplaced_callees(&self.path(id), &self.placed())
                .is_empty(),
        };
        match (has_callees, branch.shown) {
            (false, _) => Expander::Leaf,
            (true, Shown::Open) => Expander::Open,
            (true, Shown::Closed) => Expander::Closed,
        }
    }

    fn expand(&mut self, id: BranchId, index: &Index, map: &Map) {
        let Some(branch) = self.branch(id) else {
            return;
        };
        if branch.planned.verdict == Verdict::Cycle {
            return;
        }
        if branch.callees != Callees::Unloaded {
            if let Some(opened) = self.branch_mut(id) {
                opened.shown = opened.shown.flipped();
            }
            return;
        }
        let tick = branch.tick;
        let callees = map.plan_promotion_callees(index, &self.path(id), &self.placed());
        let mut children = Vec::new();
        for planned in callees {
            let child = BranchId(Count::new(self.branches.len()));
            self.branches.push(Branch {
                planned,
                parent: Some(id),
                callees: Callees::Unloaded,
                shown: Shown::Closed,
                tick: match tick {
                    Tick::Ticked => ticked_at_first(planned.verdict),
                    Tick::Unticked => Tick::Unticked,
                },
            });
            children.push(child);
        }
        if let Some(loaded) = self.branch_mut(id) {
            loaded.callees = Callees::Loaded(children);
            loaded.shown = Shown::Open;
        }
    }

    pub(crate) fn toggle(&mut self, id: BranchId) {
        let Some(branch) = self.branch(id) else {
            return;
        };
        if branch.parent.is_none() || branch.planned.verdict == Verdict::Cycle {
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
        let cut_of = |child: &BranchId| match self.branch(*child).map(|other| other.planned.verdict)
        {
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

    pub(crate) fn lines(&self) -> Vec<Line> {
        let mut out = Vec::new();
        if !self.branches.is_empty() {
            self.walk(BranchId::ROOT, Reach::Visible, &mut out);
        }
        out
    }

    pub(crate) fn ticked(&self) -> Vec<TreeEntry> {
        let mut out = Vec::new();
        if !self.branches.is_empty() {
            self.walk(BranchId::ROOT, Reach::All, &mut out);
        }
        out.into_iter()
            .filter_map(|line| match line {
                Line::Branch(id) => self.branch(id),
                Line::Fold(_) => None,
            })
            .filter(|branch| branch.tick == Tick::Ticked)
            .map(|branch| branch.planned.entry)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wizard {
    page: Page,
    kind: TourKind,
    start: Option<SymbolId>,
    seen: Option<SymbolId>,
    outline: Outline,
    refusal: Option<Label>,
}

impl Wizard {
    pub(crate) const fn page(&self) -> Page {
        self.page
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
    Return,
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
        }
    }

    pub(crate) fn typed_name(&self) -> Label {
        Label::new(self.fields.get(Which::WizardName).text().as_str().trim())
    }

    pub(crate) fn wizard_edited(&mut self) {
        if let Some(wizard) = self.wizard.as_mut() {
            wizard.refusal = None;
        }
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
    }
}

impl App {
    pub(crate) fn wizard(&mut self, act: WizardAct) {
        let model = &mut self.model;
        match act {
            WizardAct::Start => self.open_wizard(),
            WizardAct::FromHere(symbol) => self.tour_from_here(symbol),
            WizardAct::Next => self.next_page(),
            WizardAct::Back => {
                if let Some(page) = model.wizard.as_ref().and_then(|open| open.page.back()) {
                    model.show_page(page);
                }
            }
            WizardAct::Cancel => model.close_wizard(),
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
                    wizard.outline.expand(branch, &model.index, &model.map);
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
        };
        if let Some(symbol) = focus {
            wizard.choose(&model.index, &model.map, symbol);
        }
        model.wizard = Some(wizard);
        model.show_page(Page::Name);
        model.set_tab(Tab::Tour);
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
}
