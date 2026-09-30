use domain::{
    Author, FileId, GroupName, Map, MapError, PathKind, PathName, Pruning, Span, StepId, SymbolId,
};
use ui::{Label, Point, Rect};

use crate::app::App;
use crate::field::Which;
use crate::model::{Dirty, Model, PathSlot, StepKey, StepSlot, Tab};
use crate::status::{Status, Under};
use crate::theme::{DROP_BAND_WIDTH, GRAB_REACH};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Authoring {
    AddLines,
    AddSymbol(SymbolId, Hang),
    AddAtTopLevel,
    GrabStep(StepKey, Point),
    DragStep(Point),
    DropStep(Option<StepDrop>),
    Promote(SymbolId),
    ToggleNewPath,
    ChooseKind(PathKind),
    CreatePath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hang {
    Target,
    Under(StepSlot),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Moving {
    Stay,
    Moving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StepGrab {
    pub(crate) key: StepKey,
    from: Point,
    moving: Moving,
}

impl StepGrab {
    pub(crate) const fn new(key: StepKey, from: Point) -> Self {
        Self {
            key,
            from,
            moving: Moving::Stay,
        }
    }

    pub(crate) fn is_moving(self) -> bool {
        self.moving == Moving::Moving
    }

    #[must_use]
    pub(crate) fn dragged_to(self, mouse: Point) -> Self {
        let moved = (mouse.horizontal - self.from.horizontal).absolute()
            + (mouse.vertical - self.from.vertical).absolute();
        if moved > GRAB_REACH {
            Self {
                moving: Moving::Moving,
                ..self
            }
        } else {
            self
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    Before,
    Under,
    After,
}

impl Zone {
    pub(crate) fn of(row: Rect, mouse: Point) -> Self {
        let quarter = row.height / 4;
        if mouse.vertical < row.top + quarter {
            Self::Before
        } else if mouse.vertical >= row.bottom() - quarter {
            Self::After
        } else {
            Self::Under
        }
    }

    pub(crate) fn band(self, row: Rect) -> Rect {
        let half = DROP_BAND_WIDTH / 2;
        match self {
            Self::Before => Rect::new(row.left, row.top - half, row.width, DROP_BAND_WIDTH),
            Self::After => Rect::new(row.left, row.bottom() - half, row.width, DROP_BAND_WIDTH),
            Self::Under => row,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StepDrop {
    pub(crate) onto: StepSlot,
    pub(crate) zone: Zone,
    pub(crate) band: Rect,
}

impl Model {
    fn path_name(&self, path: PathSlot) -> Option<PathName> {
        self.path(path).map(|found| found.name().clone())
    }

    fn step_ids(&self, path: PathSlot) -> Vec<StepId> {
        self.path(path).map_or_else(Vec::new, |found| {
            found.steps().iter().map(|step| step.id().clone()).collect()
        })
    }

    fn under_label(&self, path: PathSlot, parent: Option<StepSlot>) -> Under {
        parent.map_or(Under::TopLevel, |step| {
            Under::Step(self.number_of(StepKey { path, step }))
        })
    }

    pub(crate) fn target_under(&self) -> Option<StepSlot> {
        let path = self.nav.path()?;
        self.nav
            .target()
            .filter(|step| step.get() < self.step_count(path).get())
    }
}

impl App {
    pub(crate) fn author(&mut self, action: Authoring) {
        match action {
            Authoring::AddLines => self.add_lines(),
            Authoring::AddSymbol(symbol, hang) => self.add_symbol(symbol, hang),
            Authoring::AddAtTopLevel => self.add_at_top_level(),
            Authoring::GrabStep(key, from) => self.grab_step(key, from),
            Authoring::DragStep(mouse) => self.drag_step(mouse),
            Authoring::DropStep(target) => self.drop_step(target),
            Authoring::Promote(symbol) => self.promote(symbol),
            Authoring::ToggleNewPath => self.toggle_new_path(),
            Authoring::ChooseKind(kind) => self.choose_kind(kind),
            Authoring::CreatePath => self.create_path(),
        }
    }

    pub(crate) fn add_lines(&mut self) {
        let model = &mut self.model;
        let (Some(file), Some(lines)) = (model.nav.file(), model.nav.lines()) else {
            model.status = Status::SelectLinesFirst;
            return;
        };
        if let Some(span) = Span::new(lines.low(), lines.high()) {
            self.add_step(file, span, Hang::Target);
        }
    }

    pub(crate) fn add_symbol(&mut self, symbol: SymbolId, hang: Hang) {
        if let Some(span) = self.model.index.symbol(symbol).map(domain::Symbol::span) {
            self.add_step(symbol.file(), span, hang);
        }
    }

    fn add_step(&mut self, file: FileId, span: Span, hang: Hang) {
        let model = &mut self.model;
        let Some(path) = model.nav.path() else {
            model.status = Status::SelectPathFirst;
            return;
        };
        let Some(name) = model.path_name(path) else {
            return;
        };
        let parent = match hang {
            Hang::Target => model.target_under(),
            Hang::Under(step) => {
                Some(step).filter(|step| step.get() < model.step_count(path).get())
            }
        };
        let parent_id = parent.and_then(|step| model.step_id(StepKey { path, step }));
        let file_path = model.index.file(file).map(|source| source.path().clone());
        let existing = model.path(path).and_then(|found| {
            found.steps().iter().position(|step| {
                Some(step.file()) == file_path.as_ref()
                    && step.span() == span
                    && step.parent() == parent_id.as_ref()
            })
        });
        if let Some(existing) = existing {
            model.status = Status::AlreadyStep {
                number: model.number_of(StepKey {
                    path,
                    step: StepSlot::new(existing),
                }),
                path: name,
            };
            return;
        }
        let added = model.map.add_step(
            &model.index,
            &name,
            file,
            span,
            Author::Human,
            parent_id.as_ref(),
        );
        let id = match added {
            Ok(id) => id,
            Err(error) => {
                model.status = Status::refused(&model.map, error);
                return;
            }
        };
        let Some(step) = model.step_slot(path, &id) else {
            return;
        };
        model.disk.dirty = Dirty::Unsaved;
        model.status = Status::StepAdded {
            number: model.number_of(StepKey { path, step }),
            path: name,
            under: model.under_label(path, parent),
        };
    }

    pub(crate) fn add_at_top_level(&mut self) {
        let model = &mut self.model;
        model.add_at_top_level();
        if let Some(name) = model.nav.path().and_then(|path| model.path_name(path)) {
            model.status = Status::TopLevelTarget(name);
        }
    }

    pub(crate) fn grab_step(&mut self, key: StepKey, from: Point) {
        self.model.step_grab = Some(StepGrab::new(key, from));
    }

    pub(crate) fn drag_step(&mut self, mouse: Point) {
        let model = &mut self.model;
        model.step_grab = model.step_grab.map(|grab| grab.dragged_to(mouse));
    }

    pub(crate) fn drop_step(&mut self, target: Option<StepDrop>) {
        let Some(grab) = self.model.step_grab.take() else {
            return;
        };
        let Some(drop) = target.filter(|drop| grab.is_moving() && drop.onto != grab.key.step)
        else {
            return;
        };
        let model = &mut self.model;
        let path = grab.key.path;
        let onto_key = StepKey {
            path,
            step: drop.onto,
        };
        let (Some(name), Some(moved), Some(onto)) = (
            model.path_name(path),
            model.step_id(grab.key),
            model.step_id(onto_key),
        ) else {
            return;
        };
        let before = model.step_ids(path);
        let onto_parent = model.step(onto_key).and_then(|step| step.parent().cloned());
        let sibling_after = || {
            model.path(path).and_then(|found| {
                found
                    .steps()
                    .iter()
                    .skip(drop.onto.get() + 1)
                    .find(|step| step.id() != &moved && step.parent() == onto_parent.as_ref())
                    .map(|step| step.id().clone())
            })
        };
        let (parent, ahead_of) = match drop.zone {
            Zone::Under => (Some(onto), None),
            Zone::Before => (onto_parent.clone(), Some(onto)),
            Zone::After => (onto_parent.clone(), sibling_after()),
        };
        if let Err(error) = model
            .map
            .place(&name, &moved, parent.as_ref(), ahead_of.as_ref())
        {
            model.status = Status::refused(&model.map, error);
            return;
        }
        let after = model.step_ids(path);
        model.steps_moved(path, |slot| {
            before
                .get(slot.get())
                .and_then(|id| after.iter().position(|other| other == id))
                .map(StepSlot::new)
        });
        model.disk.dirty = Dirty::Unsaved;
        let Some(step) = model.step_slot(path, &moved) else {
            return;
        };
        let parent = parent.and_then(|id| model.step_slot(path, &id));
        model.status = Status::StepPlaced {
            number: model.number_of(StepKey { path, step }),
            path: name,
            under: model.under_label(path, parent),
        };
    }

    pub(crate) fn promote(&mut self, symbol: SymbolId) {
        let model = &mut self.model;
        let promoted = model.map.promote(
            &model.index,
            symbol,
            Map::PROMOTE_DEPTH,
            None,
            Author::Human,
            Pruning::Pruned,
        );
        let name = match promoted {
            Ok(promoted) => promoted.name,
            Err(error) => {
                model.status = Status::refused(&model.map, error);
                return;
            }
        };
        model.disk.dirty = Dirty::Unsaved;
        if let Some(slot) = model.find_path(&name) {
            model.status = Status::PathPromoted {
                name,
                steps: model.step_count(slot),
            };
            model.select_path(slot);
            model.set_tab(Tab::Path);
        }
    }

    pub(crate) fn toggle_new_path(&mut self) {
        let model = &mut self.model;
        if model.new_path.take().is_some() {
            model.fields.release(Which::NewPath);
            model.fields.release(Which::NewGroup);
            return;
        }
        model.new_path = Some(PathKind::Flow);
        let group = model
            .nav
            .path()
            .and_then(|path| model.path(path))
            .and_then(|path| path.group())
            .map_or("", GroupName::as_str);
        model.fields.fill(Which::NewGroup, &Label::new(group));
        model.fields.start_empty(Which::NewPath);
    }

    pub(crate) fn choose_kind(&mut self, kind: PathKind) {
        let model = &mut self.model;
        if model.new_path.is_some() {
            model.new_path = Some(kind);
        }
    }

    pub(crate) fn create_path(&mut self) {
        let model = &mut self.model;
        let Some(kind) = model.new_path else {
            return;
        };
        let typed = model
            .fields
            .get(Which::NewPath)
            .text()
            .as_str()
            .trim()
            .to_owned();
        if typed.is_empty() {
            model.status = Status::NameThePath;
            model.fields.focus(Which::NewPath);
            return;
        }
        let group = GroupName::new(model.fields.get(Which::NewGroup).text().as_str());
        let created = PathName::new(&typed).and_then(|name| {
            if model.map.path(&name).is_some() {
                return Err(MapError::NameTaken(name));
            }
            let _added = model.map.add_path(name.clone(), kind, Author::Human)?;
            let _grouped = model.map.set_group(&name, group)?;
            Ok(name)
        });
        let name = match created {
            Ok(name) => name,
            Err(error) => {
                model.status = Status::refused(&model.map, error);
                return;
            }
        };
        model.new_path = None;
        model.fields.fill(Which::NewPath, &Label::default());
        model.fields.release(Which::NewPath);
        model.fields.release(Which::NewGroup);
        if let Some(slot) = model.find_path(&name) {
            model.path_created(slot);
        }
        model.status = Status::PathCreated(name);
        model.disk.dirty = Dirty::Unsaved;
    }
}
