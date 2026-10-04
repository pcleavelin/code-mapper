mod diff;
mod edit;
mod error;
mod follow;
mod name;
mod step;
mod tour;

use std::collections::{BTreeMap, BTreeSet};

use crate::index::{Cut, Depth, FileId, Index, Planned, PrunedTree, Stop, SymbolId, TreeEntry};
use crate::text::{RelativePath, Span};

pub use diff::{Change, StepChange, StepDiff, TourDiff};
pub use edit::{AddedStep, AddedSteps, AddedUnder, EditChange, StepNote, TourEdit};
pub use error::{InvalidName, MapError, StepAddress};
pub use follow::{Alignment, Followed, follow};
pub use name::{Author, GroupName, Note, TextFragment, TourCount, TourKind, TourName};
pub use step::{Anchor, Freshness, Resolution, Step, StepId, StepOrder};
pub use tour::{NumberedStep, ParentLabel, PlacedStep, StepNumber, Tour};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct Changed;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coverage<'map> {
    spans: BTreeMap<&'map RelativePath, Vec<Span>>,
}

impl Coverage<'_> {
    pub fn covers(&self, file: &RelativePath, span: Span) -> bool {
        self.spans
            .get(file)
            .is_some_and(|spans| spans.iter().any(|pinned| pinned.overlaps(span)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Group {
        group: GroupName,
        depth: Depth,
        tours: TourCount,
    },
    Tour {
        name: TourName,
        depth: Depth,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pruning {
    Pruned,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkCandidate {
    pub step: StepId,
    pub target: TourName,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Promoted {
    pub name: TourName,
    pub cut: BTreeMap<SymbolId, Cut>,
    pub stopped: BTreeMap<SymbolId, Stop>,
    pub links: Vec<LinkCandidate>,
}

struct Hung {
    symbol: SymbolId,
    step: StepId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub name: TourName,
    pub kind: TourKind,
    pub group: Option<GroupName>,
    pub note: Option<Note>,
    pub author: Author,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Map {
    tours: Vec<Tour>,
}

impl Map {
    pub const PROMOTE_DEPTH: Depth = Depth::new(2);

    pub fn new(tours: Vec<Tour>) -> Result<Self, MapError> {
        let mut names = BTreeSet::new();
        for tour in &tours {
            if !names.insert(tour.name()) {
                return Err(MapError::NameTaken(tour.name().clone()));
            }
        }
        Ok(Self { tours })
    }

    pub fn tours(&self) -> &[Tour] {
        &self.tours
    }

    pub fn tour(&self, name: &TourName) -> Option<&Tour> {
        self.tours.iter().find(|tour| tour.name() == name)
    }

    pub fn step(&self, name: &TourName, step: &StepId) -> Option<&Step> {
        self.tour(name)?.step(step)
    }

    fn tour_mut(&mut self, name: &TourName) -> Result<&mut Tour, MapError> {
        self.tours
            .iter_mut()
            .find(|tour| tour.name() == name)
            .ok_or_else(|| MapError::NoSuchTour(name.clone()))
    }

    fn step_mut(&mut self, name: &TourName, step: &StepId) -> Result<&mut Step, MapError> {
        self.tour_mut(name)?
            .step_mut(step)
            .ok_or_else(|| missing_step(name, step))
    }

    fn free_name(&self, name: &TourName, except: Option<&TourName>) -> Result<(), MapError> {
        match self
            .tours
            .iter()
            .find(|tour| tour.name().same_letters(name))
        {
            Some(other) if Some(other.name()) != except && other.name() == name => {
                Err(MapError::NameTaken(name.clone()))
            }
            Some(other) if Some(other.name()) != except => Err(MapError::CaseClash {
                name: name.clone(),
                other: other.name().clone(),
            }),
            _ => Ok(()),
        }
    }

    pub fn add_tour(
        &mut self,
        name: TourName,
        kind: TourKind,
        author: Author,
    ) -> Result<Changed, MapError> {
        if self.tour(&name).is_some() {
            return Ok(Changed);
        }
        self.free_name(&name, None)?;
        self.tours.push(Tour::empty(name, kind, author));
        Ok(Changed)
    }

    pub fn rename(&mut self, old: &TourName, new: TourName) -> Result<Changed, MapError> {
        self.tour(old)
            .ok_or_else(|| MapError::NoSuchTour(old.clone()))?;
        self.free_name(&new, Some(old))?;
        for step in self
            .tours
            .iter_mut()
            .flat_map(|tour| tour.steps_mut().iter_mut())
        {
            if step.link() == Some(old) {
                step.set_link(Some(new.clone()));
            }
        }
        self.tour_mut(old)?.set_name(new);
        Ok(Changed)
    }

    pub fn set_group(
        &mut self,
        name: &TourName,
        group: Option<GroupName>,
    ) -> Result<Changed, MapError> {
        self.tour_mut(name)?.set_group(group);
        Ok(Changed)
    }

    pub fn rename_group(
        &mut self,
        old: Option<&GroupName>,
        new: Option<&GroupName>,
    ) -> Result<TourCount, MapError> {
        let old = old.ok_or(MapError::NoGroupGiven)?;
        let mut moved = 0;
        for tour in &mut self.tours {
            if let Some(group) = tour.group().and_then(|group| group.moved(old, new)) {
                tour.set_group(GroupName::new(&group));
                moved += 1;
            }
        }
        if moved == 0 {
            return Err(MapError::NoSuchGroup(old.clone()));
        }
        Ok(TourCount::of(moved))
    }

    pub fn set_tour_note(
        &mut self,
        name: &TourName,
        note: Option<Note>,
    ) -> Result<Changed, MapError> {
        self.tour_mut(name)?.set_note(note);
        Ok(Changed)
    }

    pub fn set_step_note(
        &mut self,
        name: &TourName,
        step: &StepId,
        note: Option<Note>,
    ) -> Result<Changed, MapError> {
        self.step_mut(name, step)?.set_note(note);
        Ok(Changed)
    }

    pub fn edit_tour_note(
        &mut self,
        name: &TourName,
        old: &TextFragment,
        new: &TextFragment,
    ) -> Result<Option<Note>, MapError> {
        let tour = self.tour_mut(name)?;
        let note = old
            .replace_first(tour.note(), new)
            .map(|text| Note::new(&text))
            .ok_or_else(|| MapError::NoteLacks(old.clone()))?;
        tour.set_note(note.clone());
        Ok(note)
    }

    pub fn edit_step_note(
        &mut self,
        name: &TourName,
        step: &StepId,
        old: &TextFragment,
        new: &TextFragment,
    ) -> Result<Option<Note>, MapError> {
        let step = self.step_mut(name, step)?;
        let note = old
            .replace_first(step.note(), new)
            .map(|text| Note::new(&text))
            .ok_or_else(|| MapError::NoteLacks(old.clone()))?;
        step.set_note(note.clone());
        Ok(note)
    }

    pub fn add_step(
        &mut self,
        index: &Index,
        name: &TourName,
        file: FileId,
        span: Span,
        author: Author,
        parent: Option<&StepId>,
    ) -> Result<StepId, MapError> {
        let tour = self.tour_mut(name)?;
        if let Some(parent) = parent
            && tour.step(parent).is_none()
        {
            return Err(MapError::NoSuchParent);
        }
        let pinned = pin_span(index, file, span)?;
        let seed = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            tour.name(),
            pinned.anchor.file(),
            pinned.anchor.symbol().map_or("", |symbol| symbol.as_str()),
            pinned.anchor.start(),
            pinned.anchor.end(),
            parent.map_or("", StepId::as_str)
        );
        let id = StepId::fresh(&seed, |id| tour.step(id).is_some());
        let mut step = Step::fresh(id, author, pinned.anchor, pinned.resolution);
        let order = tour
            .steps()
            .iter()
            .map(|other| other.order().next())
            .max()
            .unwrap_or_default();
        step.set_order(order);
        step.set_parent(parent.cloned());
        let added = step.id().clone();
        tour.steps_mut().push(step);
        Ok(added)
    }

    pub fn pin(
        &mut self,
        index: &Index,
        name: &TourName,
        step: &StepId,
        file: FileId,
        span: Span,
        author: Author,
    ) -> Result<Changed, MapError> {
        let tour = self.tour_mut(name)?;
        let position = tour
            .steps()
            .iter()
            .position(|other| other.id() == step)
            .ok_or_else(|| missing_step(name, step))?;
        let pinned = pin_span(index, file, span)?;
        if let Some(slot) = tour.steps_mut().get_mut(position) {
            slot.repin(author, pinned.anchor, pinned.resolution);
        }
        Ok(Changed)
    }

    pub fn set_link(
        &mut self,
        name: &TourName,
        step: &StepId,
        target: Option<TourName>,
    ) -> Result<Changed, MapError> {
        self.step_mut(name, step)?;
        if let Some(target) = &target {
            if self.tour(target).is_none() {
                return Err(MapError::NoSuchTour(target.clone()));
            }
            if target == name {
                return Err(MapError::LinkToOwnTour);
            }
        }
        self.step_mut(name, step)?.set_link(target);
        Ok(Changed)
    }

    pub fn reparent(
        &mut self,
        name: &TourName,
        step: &StepId,
        parent: Option<&StepId>,
    ) -> Result<Changed, MapError> {
        let tour = self.tour_mut(name)?;
        tour.step(step).ok_or_else(|| missing_step(name, step))?;
        let mut visited = BTreeSet::new();
        let mut cursor = parent;
        while let Some(above) = cursor {
            if above == step {
                return Err(MapError::UnderItself);
            }
            if !visited.insert(above) {
                break;
            }
            cursor = tour.step(above).ok_or(MapError::NoSuchParent)?.parent();
        }
        let parent = parent.cloned();
        tour.step_mut(step)
            .ok_or_else(|| missing_step(name, step))?
            .set_parent(parent);
        Ok(Changed)
    }

    pub fn swap(
        &mut self,
        name: &TourName,
        one: &StepId,
        other: &StepId,
    ) -> Result<Changed, MapError> {
        let tour = self.tour_mut(name)?;
        for id in [one, other] {
            tour.step(id).ok_or_else(|| missing_step(name, id))?;
        }
        swap_places(tour.steps_mut(), one, other);
        Ok(Changed)
    }

    pub fn place(
        &mut self,
        name: &TourName,
        step: &StepId,
        parent: Option<&StepId>,
        before: Option<&StepId>,
    ) -> Result<Changed, MapError> {
        let _reparented = self.reparent(name, step, parent)?;
        let steps = self.tour_mut(name)?.steps_mut();
        let position = |id: &StepId| steps.iter().position(|other| other.id() == id);
        let mut at = position(step).ok_or_else(|| missing_step(name, step))?;
        let wanted = match before.filter(|before| *before != step) {
            Some(before) => {
                let found = position(before).ok_or_else(|| missing_step(name, before))?;
                if found > at { found - 1 } else { found }
            }
            None => steps.len().saturating_sub(1),
        };
        while at != wanted {
            let next = if at < wanted { at + 1 } else { at - 1 };
            let Some(neighbour) = steps.get(next).map(|other| other.id().clone()) else {
                break;
            };
            swap_places(steps, step, &neighbour);
            at = next;
        }
        Ok(Changed)
    }

    pub fn remove_step(&mut self, name: &TourName, step: &StepId) -> Result<Step, MapError> {
        let steps = self.tour_mut(name)?.steps_mut();
        let position = steps
            .iter()
            .position(|other| other.id() == step)
            .ok_or_else(|| missing_step(name, step))?;
        let removed = steps.remove(position);
        let up = removed
            .parent()
            .filter(|parent| *parent != removed.id())
            .cloned();
        for child in steps.iter_mut() {
            if child.parent() == Some(removed.id()) {
                child.set_parent(up.clone());
            }
        }
        Ok(removed)
    }

    pub fn remove_tour(&mut self, name: &TourName) -> Result<Tour, MapError> {
        let position = self
            .tours
            .iter()
            .position(|tour| tour.name() == name)
            .ok_or_else(|| MapError::NoSuchTour(name.clone()))?;
        let from: Vec<StepAddress> = self
            .links_to(name)
            .into_iter()
            .filter(|address| &address.tour != name)
            .collect();
        if !from.is_empty() {
            return Err(MapError::LinkedFrom {
                tour: name.clone(),
                steps: from,
            });
        }
        Ok(self.tours.remove(position))
    }

    fn covers(&self, besides: Option<&TourName>, index: &Index, symbol: SymbolId) -> Vec<TourName> {
        let (Some(found), Some(file)) = (index.symbol(symbol), index.file(symbol.file())) else {
            return Vec::new();
        };
        self.tours
            .iter()
            .filter(|tour| Some(tour.name()) != besides)
            .filter(|tour| {
                tour.steps().iter().any(|step| {
                    step.resolved_symbol() == Some(symbol)
                        || (step.file() == file.path()
                            && step.span().contains(found.span().start())
                            && step.span().contains(found.span().end()))
                })
            })
            .map(|tour| tour.name().clone())
            .collect()
    }

    fn link_target(&self, besides: &TourName, index: &Index, symbol: SymbolId) -> Option<TourName> {
        let covering = self.covers(Some(besides), index, symbol);
        let rooted = covering.iter().find(|name| {
            self.tour(name).is_some_and(|tour| {
                tour.steps()
                    .iter()
                    .any(|step| step.parent().is_none() && step.resolved_symbol() == Some(symbol))
            })
        });
        match (rooted, covering.as_slice()) {
            (Some(rooted), _) => Some(rooted.clone()),
            (None, [only]) => Some(only.clone()),
            _ => None,
        }
    }

    pub fn promote(
        &mut self,
        index: &Index,
        root: SymbolId,
        depth: Depth,
        name: Option<TourName>,
        author: Author,
        pruning: Pruning,
    ) -> Result<Promoted, MapError> {
        let root_symbol = index.symbol(root).ok_or(MapError::NoSuchSymbol)?;
        let name = name.unwrap_or_else(|| TourName::from_symbol(root_symbol.name()));
        let _added = self.add_tour(name.clone(), TourKind::Flow, author)?;
        let tree = match pruning {
            Pruning::All => PrunedTree {
                entries: index.call_tree(root, depth),
                ..PrunedTree::default()
            },
            Pruning::Pruned => {
                index.pruned_tree(root, depth, self.mapped(Some(&name), index, root))
            }
        };
        let mut links = Vec::new();
        for hung in self.hang(index, &name, &tree.entries, author)? {
            if tree.stopped.get(&hung.symbol) == Some(&Stop::Mapped)
                && let Some(target) = self.link_target(&name, index, hung.symbol)
            {
                links.push(LinkCandidate {
                    step: hung.step,
                    target,
                });
            }
        }
        Ok(Promoted {
            name,
            cut: tree.cut,
            stopped: tree.stopped,
            links,
        })
    }

    fn hang(
        &mut self,
        index: &Index,
        name: &TourName,
        entries: &[TreeEntry],
        author: Author,
    ) -> Result<Vec<Hung>, MapError> {
        let mut stack: Vec<StepId> = Vec::new();
        let mut hung = Vec::new();
        for entry in entries {
            let level = entry.depth.position();
            let (Some(symbol), Some(file)) =
                (index.symbol(entry.symbol), index.file(entry.symbol.file()))
            else {
                continue;
            };
            let parent = level
                .checked_sub(1)
                .and_then(|above| stack.get(above))
                .cloned();
            let existing = self.tour(name).and_then(|tour| {
                tour.steps()
                    .iter()
                    .find(|step| {
                        step.file() == file.path()
                            && step.resolved_symbol() == Some(entry.symbol)
                            && step.anchor().start().is_zero()
                    })
                    .map(|step| step.id().clone())
            });
            let id = match existing {
                Some(id) => id,
                None => self.add_step(
                    index,
                    name,
                    entry.symbol.file(),
                    symbol.span(),
                    author,
                    parent.as_ref(),
                )?,
            };
            hung.push(Hung {
                symbol: entry.symbol,
                step: id.clone(),
            });
            stack.truncate(level);
            stack.push(id);
        }
        Ok(hung)
    }

    fn mapped<'map>(
        &'map self,
        besides: Option<&'map TourName>,
        index: &'map Index,
        root: SymbolId,
    ) -> impl Fn(SymbolId) -> bool + 'map {
        move |symbol| symbol != root && !self.covers(besides, index, symbol).is_empty()
    }

    pub fn plan_promotion(&self, index: &Index, root: SymbolId) -> Vec<Planned> {
        index.planned_tree(root, Self::PROMOTE_DEPTH, self.mapped(None, index, root))
    }

    pub fn plan_promotion_callees(
        &self,
        index: &Index,
        besides: Option<&TourName>,
        path: &[SymbolId],
        placed: &BTreeSet<SymbolId>,
    ) -> Vec<Planned> {
        let Some(root) = path.first().copied() else {
            return Vec::new();
        };
        index.plan_callees(path, placed, &self.mapped(besides, index, root))
    }

    pub fn check_name_free(&self, name: &TourName) -> Result<Changed, MapError> {
        self.free_name(name, None)?;
        Ok(Changed)
    }

    pub fn add_drafted_tour(
        &mut self,
        index: &Index,
        draft: Draft,
        entries: &[TreeEntry],
    ) -> Result<Changed, MapError> {
        let _free = self.check_name_free(&draft.name)?;
        let mut built = self.clone();
        let _added = built.add_tour(draft.name.clone(), draft.kind, draft.author)?;
        let _grouped = built.set_group(&draft.name, draft.group)?;
        let _noted = built.set_tour_note(&draft.name, draft.note)?;
        let _hung = built.hang(index, &draft.name, entries, draft.author)?;
        *self = built;
        Ok(Changed)
    }

    pub fn tour_edit_changes(&self, tour: &TourName, edit: &TourEdit) -> Vec<EditChange> {
        let Some(found) = self.tour(tour) else {
            return Vec::new();
        };
        let mut changes = Vec::new();
        if &edit.name != found.name() {
            changes.push(EditChange::Renamed(edit.name.clone()));
        }
        if edit.kind != found.kind() {
            changes.push(EditChange::Kind(edit.kind));
        }
        if edit.group.as_ref() != found.group() {
            changes.push(EditChange::Group(edit.group.clone()));
        }
        if edit.note.as_ref() != found.note() {
            changes.push(EditChange::TourNote(edit.note.clone()));
        }
        for note in &edit.step_notes {
            if edit.removed.contains(&note.step) {
                continue;
            }
            if found
                .step(&note.step)
                .is_some_and(|step| step.note() != note.note.as_ref())
            {
                changes.push(EditChange::StepNote(note.clone()));
            }
        }
        for added in &edit.added {
            push_added(
                &mut changes,
                &AddedUnder::Step(added.under.clone()),
                &added.steps,
            );
        }
        for step in &edit.removed {
            if let Some(gone) = found.step(step) {
                changes.push(EditChange::Removed {
                    step: step.clone(),
                    lost: gone.note().cloned(),
                });
            }
        }
        changes
    }

    pub fn apply_tour_edit(
        &mut self,
        index: &Index,
        tour: &TourName,
        edit: TourEdit,
    ) -> Result<Changed, MapError> {
        let mut edited = self.clone();
        for note in &edit.step_notes {
            if !edit.removed.contains(&note.step) {
                let _noted = edited.set_step_note(tour, &note.step, note.note.clone())?;
            }
        }
        for step in &edit.removed {
            let _removed = edited.remove_step(tour, step)?;
        }
        for added in &edit.added {
            let _added =
                edited.add_steps_under(index, tour, &added.under, &added.steps, edit.author)?;
        }
        edited.tour_mut(tour)?.set_kind(edit.kind);
        let _grouped = edited.set_group(tour, edit.group)?;
        let _noted = edited.set_tour_note(tour, edit.note)?;
        if &edit.name != tour {
            let _renamed = edited.rename(tour, edit.name)?;
        }
        *self = edited;
        Ok(Changed)
    }

    fn add_steps_under(
        &mut self,
        index: &Index,
        tour: &TourName,
        under: &StepId,
        steps: &[AddedStep],
        author: Author,
    ) -> Result<Changed, MapError> {
        for added in steps {
            let symbol = index.symbol(added.symbol).ok_or(MapError::NoSuchSymbol)?;
            let id = self.add_step(
                index,
                tour,
                added.symbol.file(),
                symbol.span(),
                author,
                Some(under),
            )?;
            let _noted = self.set_step_note(tour, &id, added.note.clone())?;
            let _below = self.add_steps_under(index, tour, &id, &added.below, author)?;
        }
        Ok(Changed)
    }

    pub fn resolve_all(&mut self, index: &Index) {
        for tour in &mut self.tours {
            for step in tour.steps_mut() {
                step.resolve(index);
            }
        }
    }

    pub fn rows(&self) -> Vec<Row> {
        self.rows_where(|_| true)
    }

    pub fn rows_where(&self, wanted: impl Fn(&Tour) -> bool) -> Vec<Row> {
        let mut rows = Vec::new();
        let members: Vec<&Tour> = self.tours.iter().filter(|tour| wanted(tour)).collect();
        level(&members, None, Depth::default(), &mut rows);
        rows
    }

    pub fn links_to(&self, target: &TourName) -> Vec<StepAddress> {
        self.addresses(|step| step.link() == Some(target))
    }

    pub fn dangling_links(&self) -> Vec<StepAddress> {
        self.addresses(|step| step.link().is_some_and(|link| self.tour(link).is_none()))
    }

    fn addresses(&self, wanted: impl Fn(&Step) -> bool) -> Vec<StepAddress> {
        self.tours
            .iter()
            .flat_map(|tour| {
                tour.steps()
                    .iter()
                    .filter(|step| wanted(step))
                    .map(|step| StepAddress {
                        tour: tour.name().clone(),
                        step: step.id().clone(),
                    })
            })
            .collect()
    }

    pub fn coverage(&self) -> Coverage<'_> {
        let mut spans: BTreeMap<&RelativePath, Vec<Span>> = BTreeMap::new();
        for step in self.tours.iter().flat_map(Tour::steps) {
            if !step.is_stale() {
                spans.entry(step.file()).or_default().push(step.span());
            }
        }
        Coverage { spans }
    }

    pub fn diff(&self, base: &Self) -> Vec<TourDiff> {
        let mut diffs: Vec<TourDiff> = self
            .tours
            .iter()
            .map(|tour| match base.tour(tour.name()) {
                Some(old) => TourDiff::between(tour, old),
                None => TourDiff::added(tour),
            })
            .collect();
        diffs.extend(
            base.tours
                .iter()
                .filter(|old| self.tour(old.name()).is_none())
                .map(TourDiff::removed_tour),
        );
        diffs
    }
}

fn missing_step(name: &TourName, step: &StepId) -> MapError {
    MapError::NoSuchStep(StepAddress {
        tour: name.clone(),
        step: step.clone(),
    })
}

fn swap_places(steps: &mut [Step], one: &StepId, other: &StepId) {
    let position = |id: &StepId| steps.iter().position(|step| step.id() == id);
    let (Some(first), Some(second)) = (position(one), position(other)) else {
        return;
    };
    let orders = steps
        .get(first)
        .map(Step::order)
        .zip(steps.get(second).map(Step::order));
    steps.swap(first, second);
    if let Some((first_order, second_order)) = orders {
        if let Some(step) = steps.get_mut(first) {
            step.set_order(first_order);
        }
        if let Some(step) = steps.get_mut(second) {
            step.set_order(second_order);
        }
    }
}

pub(crate) struct Pinned {
    pub(crate) anchor: Anchor,
    pub(crate) resolution: Resolution,
}

pub(crate) fn pin_span(index: &Index, file: FileId, span: Span) -> Result<Pinned, MapError> {
    let source = index.file(file).ok_or(MapError::NoSuchFile)?;
    let anchor = Anchor::at(source, span).ok_or(MapError::OutsideFile)?;
    let symbol = source
        .innermost(span)
        .map(|symbol| SymbolId::new(file, symbol));
    Ok(Pinned {
        anchor,
        resolution: Resolution {
            span,
            freshness: Freshness::Current,
            symbol,
        },
    })
}

fn level(members: &[&Tour], prefix: Option<&GroupName>, depth: Depth, rows: &mut Vec<Row>) {
    let mut groups: BTreeMap<GroupName, Vec<&Tour>> = BTreeMap::new();
    let mut here = Vec::new();
    for tour in members {
        match tour.group().and_then(|group| group.child_under(prefix)) {
            Some(child) => groups.entry(child).or_default().push(*tour),
            None => here.push(tour.name().clone()),
        }
    }
    for (group, inside) in groups {
        rows.push(Row::Group {
            group: group.clone(),
            depth,
            tours: TourCount::of(inside.len()),
        });
        level(&inside, Some(&group), depth.deeper(), rows);
    }
    rows.extend(here.into_iter().map(|name| Row::Tour { name, depth }));
}

fn push_added(changes: &mut Vec<EditChange>, under: &AddedUnder, steps: &[AddedStep]) {
    for added in steps {
        changes.push(EditChange::Added {
            symbol: added.symbol,
            under: under.clone(),
        });
        push_added(changes, &AddedUnder::Added(added.symbol), &added.below);
    }
}

#[cfg(test)]
mod tests;
