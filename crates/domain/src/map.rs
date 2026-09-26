mod diff;
mod error;
mod follow;
mod name;
mod path;
mod step;

use std::collections::{BTreeMap, BTreeSet};

use crate::index::{Depth, FileId, Index, SymbolId};
use crate::text::{RelativePath, Span};

pub use diff::{Change, PathDiff, StepChange, StepDiff};
pub use error::{InvalidName, MapError, StepAddress};
pub use follow::{Alignment, Followed, follow};
pub use name::{Author, GroupName, Note, PathCount, PathKind, PathName, TextFragment};
pub use path::{NumberedStep, ParentLabel, Path, PlacedStep, StepNumber};
pub use step::{Anchor, Freshness, Resolution, Step, StepId, StepOrder};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct Changed;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Group {
        group: GroupName,
        depth: Depth,
        paths: PathCount,
    },
    Path {
        name: PathName,
        depth: Depth,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Map {
    paths: Vec<Path>,
}

impl Map {
    pub fn new(paths: Vec<Path>) -> Result<Self, MapError> {
        let mut names = BTreeSet::new();
        for path in &paths {
            if !names.insert(path.name()) {
                return Err(MapError::NameTaken(path.name().clone()));
            }
        }
        Ok(Self { paths })
    }

    pub fn paths(&self) -> &[Path] {
        &self.paths
    }

    pub fn path(&self, name: &PathName) -> Option<&Path> {
        self.paths.iter().find(|path| path.name() == name)
    }

    pub fn step(&self, name: &PathName, step: &StepId) -> Option<&Step> {
        self.path(name)?.step(step)
    }

    fn path_mut(&mut self, name: &PathName) -> Result<&mut Path, MapError> {
        self.paths
            .iter_mut()
            .find(|path| path.name() == name)
            .ok_or_else(|| MapError::NoSuchPath(name.clone()))
    }

    fn step_mut(&mut self, name: &PathName, step: &StepId) -> Result<&mut Step, MapError> {
        self.path_mut(name)?
            .step_mut(step)
            .ok_or_else(|| missing_step(name, step))
    }

    fn free_name(&self, name: &PathName, except: Option<&PathName>) -> Result<(), MapError> {
        match self
            .paths
            .iter()
            .find(|path| path.name().same_letters(name))
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

    pub fn add_path(
        &mut self,
        name: PathName,
        kind: PathKind,
        author: Author,
    ) -> Result<Changed, MapError> {
        if self.path(&name).is_some() {
            return Ok(Changed);
        }
        self.free_name(&name, None)?;
        self.paths.push(Path::empty(name, kind, author));
        Ok(Changed)
    }

    pub fn rename(&mut self, old: &PathName, new: PathName) -> Result<Changed, MapError> {
        self.path(old)
            .ok_or_else(|| MapError::NoSuchPath(old.clone()))?;
        self.free_name(&new, Some(old))?;
        for step in self
            .paths
            .iter_mut()
            .flat_map(|path| path.steps_mut().iter_mut())
        {
            if step.link() == Some(old) {
                step.set_link(Some(new.clone()));
            }
        }
        self.path_mut(old)?.set_name(new);
        Ok(Changed)
    }

    pub fn set_group(
        &mut self,
        name: &PathName,
        group: Option<GroupName>,
    ) -> Result<Changed, MapError> {
        self.path_mut(name)?.set_group(group);
        Ok(Changed)
    }

    pub fn rename_group(
        &mut self,
        old: Option<&GroupName>,
        new: Option<&GroupName>,
    ) -> Result<PathCount, MapError> {
        let old = old.ok_or(MapError::NoGroupGiven)?;
        let mut moved = 0;
        for path in &mut self.paths {
            if let Some(group) = path.group().and_then(|group| group.moved(old, new)) {
                path.set_group(GroupName::new(&group));
                moved += 1;
            }
        }
        if moved == 0 {
            return Err(MapError::NoSuchGroup(old.clone()));
        }
        Ok(PathCount::of(moved))
    }

    pub fn set_path_note(
        &mut self,
        name: &PathName,
        note: Option<Note>,
    ) -> Result<Changed, MapError> {
        self.path_mut(name)?.set_note(note);
        Ok(Changed)
    }

    pub fn set_step_note(
        &mut self,
        name: &PathName,
        step: &StepId,
        note: Option<Note>,
    ) -> Result<Changed, MapError> {
        self.step_mut(name, step)?.set_note(note);
        Ok(Changed)
    }

    pub fn edit_path_note(
        &mut self,
        name: &PathName,
        old: &TextFragment,
        new: &TextFragment,
    ) -> Result<Option<Note>, MapError> {
        let path = self.path_mut(name)?;
        let note = old
            .replace_first(path.note(), new)
            .map(|text| Note::new(&text))
            .ok_or_else(|| MapError::NoteLacks(old.clone()))?;
        path.set_note(note.clone());
        Ok(note)
    }

    pub fn edit_step_note(
        &mut self,
        name: &PathName,
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
        name: &PathName,
        file: FileId,
        span: Span,
        author: Author,
        parent: Option<&StepId>,
    ) -> Result<StepId, MapError> {
        let path = self.path_mut(name)?;
        if let Some(parent) = parent
            && path.step(parent).is_none()
        {
            return Err(MapError::NoSuchParent);
        }
        let pinned = pin_span(index, file, span)?;
        let seed = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            path.name(),
            pinned.anchor.file(),
            pinned.anchor.symbol().map_or("", |symbol| symbol.as_str()),
            pinned.anchor.start(),
            pinned.anchor.end(),
            parent.map_or("", StepId::as_str)
        );
        let id = StepId::fresh(&seed, |id| path.step(id).is_some());
        let mut step = Step::fresh(id, author, pinned.anchor, pinned.resolution);
        let order = path
            .steps()
            .iter()
            .map(|other| other.order().next())
            .max()
            .unwrap_or_default();
        step.set_order(order);
        step.set_parent(parent.cloned());
        let added = step.id().clone();
        path.steps_mut().push(step);
        Ok(added)
    }

    pub fn pin(
        &mut self,
        index: &Index,
        name: &PathName,
        step: &StepId,
        file: FileId,
        span: Span,
        author: Author,
    ) -> Result<Changed, MapError> {
        let path = self.path_mut(name)?;
        let position = path
            .steps()
            .iter()
            .position(|other| other.id() == step)
            .ok_or_else(|| missing_step(name, step))?;
        let pinned = pin_span(index, file, span)?;
        if let Some(slot) = path.steps_mut().get_mut(position) {
            slot.repin(author, pinned.anchor, pinned.resolution);
        }
        Ok(Changed)
    }

    pub fn set_link(
        &mut self,
        name: &PathName,
        step: &StepId,
        target: Option<PathName>,
    ) -> Result<Changed, MapError> {
        self.step_mut(name, step)?;
        if let Some(target) = &target {
            if self.path(target).is_none() {
                return Err(MapError::NoSuchPath(target.clone()));
            }
            if target == name {
                return Err(MapError::LinkToOwnPath);
            }
        }
        self.step_mut(name, step)?.set_link(target);
        Ok(Changed)
    }

    pub fn reparent(
        &mut self,
        name: &PathName,
        step: &StepId,
        parent: Option<&StepId>,
    ) -> Result<Changed, MapError> {
        let path = self.path_mut(name)?;
        path.step(step).ok_or_else(|| missing_step(name, step))?;
        let mut visited = BTreeSet::new();
        let mut cursor = parent;
        while let Some(above) = cursor {
            if above == step {
                return Err(MapError::UnderItself);
            }
            if !visited.insert(above) {
                break;
            }
            cursor = path.step(above).ok_or(MapError::NoSuchParent)?.parent();
        }
        let parent = parent.cloned();
        path.step_mut(step)
            .ok_or_else(|| missing_step(name, step))?
            .set_parent(parent);
        Ok(Changed)
    }

    pub fn swap(
        &mut self,
        name: &PathName,
        one: &StepId,
        other: &StepId,
    ) -> Result<Changed, MapError> {
        let path = self.path_mut(name)?;
        let steps = path.steps_mut();
        let first = steps
            .iter()
            .position(|step| step.id() == one)
            .ok_or_else(|| missing_step(name, one))?;
        let second = steps
            .iter()
            .position(|step| step.id() == other)
            .ok_or_else(|| missing_step(name, other))?;
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
        Ok(Changed)
    }

    pub fn remove_step(&mut self, name: &PathName, step: &StepId) -> Result<Step, MapError> {
        let steps = self.path_mut(name)?.steps_mut();
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

    pub fn remove_path(&mut self, name: &PathName) -> Result<Path, MapError> {
        let position = self
            .paths
            .iter()
            .position(|path| path.name() == name)
            .ok_or_else(|| MapError::NoSuchPath(name.clone()))?;
        let from: Vec<StepAddress> = self
            .links_to(name)
            .into_iter()
            .filter(|address| &address.path != name)
            .collect();
        if !from.is_empty() {
            return Err(MapError::LinkedFrom {
                path: name.clone(),
                steps: from,
            });
        }
        Ok(self.paths.remove(position))
    }

    pub fn promote(
        &mut self,
        index: &Index,
        root: SymbolId,
        depth: Depth,
        name: Option<PathName>,
        author: Author,
    ) -> Result<PathName, MapError> {
        let root_symbol = index.symbol(root).ok_or(MapError::NoSuchSymbol)?;
        let name = name.unwrap_or_else(|| PathName::from_symbol(root_symbol.name()));
        let _added = self.add_path(name.clone(), PathKind::Flow, author)?;
        let mut stack: Vec<StepId> = Vec::new();
        for entry in index.call_tree(root, depth) {
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
            let existing = self.path(&name).and_then(|path| {
                path.steps()
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
                    &name,
                    entry.symbol.file(),
                    symbol.span(),
                    author,
                    parent.as_ref(),
                )?,
            };
            stack.truncate(level);
            stack.push(id);
        }
        Ok(name)
    }

    pub fn resolve_all(&mut self, index: &Index) {
        for path in &mut self.paths {
            for step in path.steps_mut() {
                step.resolve(index);
            }
        }
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let members: Vec<&Path> = self.paths.iter().collect();
        level(&members, None, Depth::default(), &mut rows);
        rows
    }

    pub fn links_to(&self, target: &PathName) -> Vec<StepAddress> {
        self.addresses(|step| step.link() == Some(target))
    }

    pub fn dangling_links(&self) -> Vec<StepAddress> {
        self.addresses(|step| step.link().is_some_and(|link| self.path(link).is_none()))
    }

    fn addresses(&self, wanted: impl Fn(&Step) -> bool) -> Vec<StepAddress> {
        self.paths
            .iter()
            .flat_map(|path| {
                path.steps()
                    .iter()
                    .filter(|step| wanted(step))
                    .map(|step| StepAddress {
                        path: path.name().clone(),
                        step: step.id().clone(),
                    })
            })
            .collect()
    }

    pub fn covers(&self, file: &RelativePath, span: Span) -> bool {
        self.paths
            .iter()
            .flat_map(Path::steps)
            .any(|step| !step.is_stale() && step.file() == file && step.span().overlaps(span))
    }

    pub fn diff(&self, base: &Self) -> Vec<PathDiff> {
        let mut diffs: Vec<PathDiff> = self
            .paths
            .iter()
            .map(|path| match base.path(path.name()) {
                Some(old) => PathDiff::between(path, old),
                None => PathDiff::added(path),
            })
            .collect();
        diffs.extend(
            base.paths
                .iter()
                .filter(|old| self.path(old.name()).is_none())
                .map(PathDiff::removed_path),
        );
        diffs
    }
}

fn missing_step(name: &PathName, step: &StepId) -> MapError {
    MapError::NoSuchStep(StepAddress {
        path: name.clone(),
        step: step.clone(),
    })
}

struct Pinned {
    anchor: Anchor,
    resolution: Resolution,
}

fn pin_span(index: &Index, file: FileId, span: Span) -> Result<Pinned, MapError> {
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

fn level(members: &[&Path], prefix: Option<&GroupName>, depth: Depth, rows: &mut Vec<Row>) {
    let mut groups: BTreeMap<GroupName, Vec<&Path>> = BTreeMap::new();
    let mut here = Vec::new();
    for path in members {
        match path.group().and_then(|group| group.child_under(prefix)) {
            Some(child) => groups.entry(child).or_default().push(*path),
            None => here.push(path.name().clone()),
        }
    }
    for (group, inside) in groups {
        rows.push(Row::Group {
            group: group.clone(),
            depth,
            paths: PathCount::of(inside.len()),
        });
        level(&inside, Some(&group), depth.deeper(), rows);
    }
    rows.extend(here.into_iter().map(|name| Row::Path { name, depth }));
}

#[cfg(test)]
mod tests;
