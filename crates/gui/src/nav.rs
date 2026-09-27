use std::cmp::Ordering;
use std::collections::BTreeSet;

use domain::{FileId, Line, RelativePath, SymbolId, SymbolKey};
use ui::Count;

use crate::field::Which;
use crate::keys::{Extend, Walk};
use crate::model::{LineSelection, Model, PathSlot, StepKey, StepSlot, Tab, ViewFlag};
use crate::panels::{BranchId, View};
use crate::status::Status;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Ticket(u64);

impl Ticket {
    pub(crate) fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineRequest {
    pub(crate) line: Line,
    pub(crate) ticket: Ticket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tries(u8);

impl Tries {
    const FIRST: Self = Self(5);

    pub(crate) const fn left(self) -> bool {
        self.0 > 0
    }

    #[must_use]
    pub(crate) const fn fewer(self) -> Self {
        Self(self.0.saturating_sub(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StepRequest {
    pub(crate) step: StepSlot,
    pub(crate) tries: Tries,
    pub(crate) ticket: Ticket,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Place {
    tab: Tab,
    file: Option<RelativePath>,
    lines: Option<LineSelection>,
    focus: Option<SymbolKey>,
    path: Option<PathSlot>,
    step: Option<StepSlot>,
}

impl Place {
    fn same_place(&self, other: &Self) -> bool {
        self.tab == other.tab
            && self.file == other.file
            && self.focus == other.focus
            && self.path == other.path
            && self.step == other.step
    }
}

#[derive(Clone, Debug, Default)]
struct History {
    back: Vec<Place>,
    forward: Vec<Place>,
    last: Option<Place>,
}

impl History {
    const LONGEST: Count = Count::new(200);

    fn places_mut(&mut self) -> impl Iterator<Item = &mut Place> {
        self.back
            .iter_mut()
            .chain(self.forward.iter_mut())
            .chain(self.last.iter_mut())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scrolling {
    Scroll,
    Stay,
}

#[derive(Clone, Debug)]
pub(crate) struct Nav {
    tab: Tab,
    path: Option<PathSlot>,
    step: Option<StepSlot>,
    target: Option<StepSlot>,
    focus: Option<SymbolId>,
    file: Option<FileId>,
    lines: Option<LineSelection>,
    scroll_to: Option<LineRequest>,
    scroll_to_step: Option<StepRequest>,
    top_step: Option<StepSlot>,
    outline_shown: Option<StepSlot>,
    ticket: Ticket,
    asked: Ticket,
    history: History,
}

impl Default for Nav {
    fn default() -> Self {
        Self {
            tab: Tab::Path,
            path: None,
            step: None,
            target: None,
            focus: None,
            file: None,
            lines: None,
            scroll_to: None,
            scroll_to_step: None,
            top_step: None,
            outline_shown: None,
            ticket: Ticket(0),
            asked: Ticket(0),
            history: History::default(),
        }
    }
}

impl Nav {
    pub(crate) const fn tab(&self) -> Tab {
        self.tab
    }

    pub(crate) const fn path(&self) -> Option<PathSlot> {
        self.path
    }

    pub(crate) const fn step(&self) -> Option<StepSlot> {
        self.step
    }

    pub(crate) const fn target(&self) -> Option<StepSlot> {
        self.target
    }

    pub(crate) fn step_key(&self) -> Option<StepKey> {
        Some(StepKey {
            path: self.path?,
            step: self.step?,
        })
    }

    pub(crate) const fn focus(&self) -> Option<SymbolId> {
        self.focus
    }

    pub(crate) const fn file(&self) -> Option<FileId> {
        self.file
    }

    pub(crate) const fn lines(&self) -> Option<LineSelection> {
        self.lines
    }

    pub(crate) const fn scroll_to(&self) -> Option<LineRequest> {
        self.scroll_to
    }

    pub(crate) const fn scroll_to_step(&self) -> Option<StepRequest> {
        self.scroll_to_step
    }

    pub(crate) const fn top_step(&self) -> Option<StepSlot> {
        self.top_step
    }

    pub(crate) const fn outline_shown(&self) -> Option<StepSlot> {
        self.outline_shown
    }

    pub(crate) fn can_go_back(&self) -> bool {
        !self.history.back.is_empty()
    }

    pub(crate) fn can_go_forward(&self) -> bool {
        !self.history.forward.is_empty()
    }

    fn ticket(&mut self) -> Ticket {
        self.ticket = self.ticket.next();
        self.ticket
    }

    fn show(&mut self, tab: Tab) {
        self.tab = tab;
        self.asked = self.ticket();
    }

    fn scroll_to_line(&mut self, line: Line) {
        let ticket = self.ticket();
        self.scroll_to = Some(LineRequest { line, ticket });
    }
}

impl Model {
    fn here(&self) -> Place {
        let nav = &self.nav;
        Place {
            tab: nav.tab,
            file: nav
                .file
                .and_then(|file| self.index.file(file))
                .map(|file| file.path().clone()),
            lines: nav.lines,
            focus: nav.focus.and_then(|symbol| self.index.symbol_key(symbol)),
            path: nav.path,
            step: nav.step,
        }
    }

    pub(crate) fn track_navigation(&mut self) {
        let now = self.here();
        if let Some(previous) = self.nav.history.last.take()
            && !previous.same_place(&now)
        {
            let history = &mut self.nav.history;
            history.back.push(previous);
            history.forward.clear();
            if history.back.len() > History::LONGEST.get() {
                history.back.remove(0);
            }
        }
        self.nav.history.last = Some(now);
    }

    pub(crate) fn back(&mut self) {
        let Some(place) = self.nav.history.back.pop() else {
            return;
        };
        let here = self.here();
        self.nav.history.forward.push(here);
        self.go(place);
    }

    pub(crate) fn forward(&mut self) {
        let Some(place) = self.nav.history.forward.pop() else {
            return;
        };
        let here = self.here();
        self.nav.history.back.push(here);
        self.go(place);
    }

    fn go(&mut self, place: Place) {
        self.nav.path = place
            .path
            .filter(|path| path.get() < self.path_count().get());
        match (self.nav.path, place.step) {
            (Some(path), Some(step)) if step.get() < self.step_count(path).get() => {
                self.select_step(StepKey { path, step }, Scrolling::Scroll);
            }
            _ => {
                self.nav.step = None;
                if let Some(symbol) = place.focus.and_then(|key| self.index.by_key(&key)) {
                    self.focus_symbol(symbol);
                }
            }
        }
        self.nav.file = place.file.and_then(|path| self.index.find_file(&path));
        self.nav.lines = place.lines;
        if let Some(lines) = place.lines {
            self.nav.scroll_to_line(lines.from);
        }
        self.nav.show(place.tab);
        self.nav.history.last = Some(self.here());
    }

    pub(crate) fn step_removed(&mut self, removed: StepKey) {
        self.steps_moved(removed.path, |step| match step.cmp(&removed.step) {
            Ordering::Equal => None,
            Ordering::Greater => Some(StepSlot::new(step.get() - 1)),
            Ordering::Less => Some(step),
        });
    }

    pub(crate) fn steps_moved(
        &mut self,
        path: PathSlot,
        shift: impl Fn(StepSlot) -> Option<StepSlot>,
    ) {
        self.views.remap(|key| {
            if key.path == path {
                shift(key.step).map(|step| StepKey {
                    path: key.path,
                    step,
                })
            } else {
                Some(key)
            }
        });
        for place in self.nav.history.places_mut() {
            if place.path == Some(path) {
                place.step = place.step.and_then(&shift);
            }
        }
        if self.nav.path == Some(path) {
            self.nav.step = self.nav.step.and_then(&shift);
            self.nav.target = self.nav.target.and_then(&shift);
            self.nav.top_step = self.nav.top_step.and_then(&shift);
            self.nav.outline_shown = None;
        }
    }

    pub(crate) fn add_at_top_level(&mut self) {
        self.nav.target = None;
    }

    pub(crate) fn path_removed(&mut self, removed: PathSlot) {
        let shift = |path: PathSlot| match path.cmp(&removed) {
            Ordering::Equal => None,
            Ordering::Greater => Some(PathSlot::new(path.get() - 1)),
            Ordering::Less => Some(path),
        };
        self.views.remap(|key| {
            shift(key.path).map(|path| StepKey {
                path,
                step: key.step,
            })
        });
        let history = &mut self.nav.history;
        history.back.retain(|place| place.path != Some(removed));
        history.forward.retain(|place| place.path != Some(removed));
        for place in history.places_mut() {
            place.path = place.path.and_then(shift);
        }
        self.nav.top_step = None;
    }

    pub(crate) fn walk(&mut self, walk: Walk) {
        let Some(path) = self
            .nav
            .path
            .filter(|path| path.get() < self.path_count().get())
        else {
            return;
        };
        let order = self.tree_order(path);
        let Some(last) = order.len().checked_sub(1) else {
            return;
        };
        let at = self
            .nav
            .step
            .and_then(|step| order.iter().position(|placed| placed.step == step))
            .map_or(0, |position| {
                if walk == Walk::Down {
                    (position + 1).min(last)
                } else {
                    position.saturating_sub(1)
                }
            });
        if let Some(placed) = order.get(at) {
            self.select_step(
                StepKey {
                    path,
                    step: placed.step,
                },
                Scrolling::Scroll,
            );
        }
    }

    pub(crate) fn focus_symbol(&mut self, symbol: SymbolId) {
        let Some(span) = self.index.symbol(symbol).map(domain::Symbol::span) else {
            return;
        };
        self.nav.file = Some(symbol.file());
        self.nav.lines = Some(LineSelection {
            from: span.start(),
            to: span.end(),
        });
        self.nav.scroll_to_line(span.start());
        self.nav.focus = Some(symbol);
        self.graph.focused();
    }

    pub(crate) fn select_symbol(&mut self, symbol: SymbolId) {
        if let Some(path) = self.nav.path {
            let on_path = self.path(path).and_then(|found| {
                found.steps().iter().position(|step| {
                    step.resolved_symbol() == Some(symbol) && step.anchor().start().is_zero()
                })
            });
            if let Some(step) = on_path {
                self.select_step(
                    StepKey {
                        path,
                        step: StepSlot::new(step),
                    },
                    Scrolling::Scroll,
                );
                return;
            }
        }
        self.nav.step = None;
        self.focus_symbol(symbol);
    }

    pub(crate) fn select_step(&mut self, key: StepKey, scrolling: Scrolling) {
        self.nav.path = Some(key.path);
        self.nav.step = Some(key.step);
        self.nav.target = Some(key.step);
        let mut seen = BTreeSet::new();
        let mut up = self.parent_of(key);
        while let Some(parent) = up.filter(|parent| seen.insert(*parent)) {
            let above = StepKey {
                path: key.path,
                step: parent,
            };
            if let Some(view) = self.views.existing(above) {
                view.flags.set(ViewFlag::Folded, false);
            }
            up = self.parent_of(above);
        }
        let Some(step) = self.step(key) else {
            return;
        };
        let file = step.file().clone();
        let span = step.span();
        let symbol = step.resolved_symbol();
        let lines = LineSelection {
            from: span.start(),
            to: span.end(),
        };
        match (self.index.find_file(&file), symbol) {
            (Some(_), Some(symbol)) => {
                self.focus_symbol(symbol);
                self.nav.lines = Some(lines);
                self.nav.scroll_to_line(span.start());
            }
            (Some(found), None) => {
                self.nav.file = Some(found);
                self.nav.lines = Some(lines);
                self.nav.scroll_to_line(span.start());
                if let Some(enclosing) = self.index.by_line(&file, span.start()) {
                    self.nav.focus = Some(enclosing);
                }
            }
            (None, _) => {}
        }
        if scrolling == Scrolling::Scroll {
            let ticket = self.nav.ticket();
            self.nav.scroll_to_step = Some(StepRequest {
                step: key.step,
                tries: Tries::FIRST,
                ticket,
            });
        }
        if matches!(self.nav.tab, Tab::Listing | Tab::Results | Tab::Diff) {
            self.nav.show(Tab::Path);
        }
    }

    pub(crate) fn select_path(&mut self, path: PathSlot) {
        self.nav.path = Some(path);
        if let Some(placed) = self.tree_order(path).first() {
            let step = placed.step;
            self.select_step(StepKey { path, step }, Scrolling::Scroll);
        } else {
            self.nav.step = None;
            self.nav.target = None;
        }
    }

    pub(crate) fn open_path(&mut self, path: PathSlot, tab: Tab) {
        if self.nav.path != Some(path) {
            self.select_path(path);
        }
        self.nav.show(tab);
    }

    pub(crate) fn go_to_symbol(&mut self, symbol: SymbolId) {
        self.select_symbol(symbol);
        if self.nav.step.is_some() {
            return;
        }
        if !matches!(self.nav.tab, Tab::Graph | Tab::Path) {
            self.nav.show(Tab::Listing);
        } else if let Some(path) = self.nav.path {
            let name = self.index.symbol(symbol).map(|found| found.name().clone());
            let path = self.path(path).map(|found| found.name().clone());
            if let (Some(name), Some(path)) = (name, path) {
                self.status = Status::OffPathSymbol { symbol: name, path };
            }
        }
    }

    pub(crate) fn jumped_to_symbol(&mut self, symbol: SymbolId) {
        self.select_symbol(symbol);
        let on_step = self.nav.step.is_some() && self.nav.tab == Tab::Path;
        if self.nav.tab != Tab::Graph && !on_step {
            self.nav.show(Tab::Listing);
        }
    }

    pub(crate) fn open_line(&mut self, file: FileId, line: Line) {
        self.nav.file = Some(file);
        self.nav.lines = Some(LineSelection::one(line));
        self.nav.scroll_to_line(line);
        self.nav.show(Tab::Listing);
    }

    pub(crate) fn go_to_line(&mut self, line: Line) {
        self.nav.lines = Some(LineSelection::one(line));
        self.nav.scroll_to_line(line);
    }

    pub(crate) fn select_line(&mut self, line: Line, extend: Extend) {
        self.nav.lines = match (extend, self.nav.lines) {
            (Extend::Extend, Some(lines)) => Some(LineSelection {
                from: lines.from,
                to: line,
            }),
            _ => Some(LineSelection::one(line)),
        };
    }

    pub(crate) fn set_tab(&mut self, tab: Tab) {
        self.nav.show(tab);
    }

    pub(crate) fn show_view(&mut self, view: View) {
        match view.tab() {
            Some(tab) => self.set_tab(tab),
            None => self.panels.activate(view),
        }
    }

    pub(crate) fn pick_view(&mut self, panel: BranchId, view: View) {
        self.panels.pick(panel, view);
        self.fields.release(Which::ViewSearch);
        if let Some(tab) = view.tab() {
            self.set_tab(tab);
        }
    }

    pub(crate) fn reveal_tab(&mut self) {
        let (tab, asked) = (self.nav.tab, self.nav.asked);
        self.panels.reveal(tab, asked);
    }

    pub(crate) fn path_created(&mut self, path: PathSlot) {
        self.nav.path = Some(path);
        self.nav.step = None;
        self.nav.target = None;
        self.nav.show(Tab::Path);
    }

    pub(crate) fn forget_step(&mut self) {
        self.nav.step = None;
    }

    pub(crate) fn forget_path(&mut self) {
        self.nav.path = None;
        self.nav.step = None;
        self.nav.target = None;
    }

    pub(crate) fn reselect(&mut self, path: Option<PathSlot>, step: Option<StepSlot>) {
        self.nav.path = path;
        self.nav.step = step;
        self.nav.target = step;
    }

    pub(crate) fn refocus(&mut self, focus: Option<SymbolId>, file: Option<FileId>) {
        self.nav.focus = focus;
        self.nav.file = file;
    }

    pub(crate) fn scrolled_to_line(&mut self, ticket: Ticket) {
        if self
            .nav
            .scroll_to
            .is_some_and(|request| request.ticket == ticket)
        {
            self.nav.scroll_to = None;
        }
    }

    pub(crate) fn scrolled_to_step(&mut self, ticket: Ticket, retry: Option<Tries>) {
        let Some(request) = self
            .nav
            .scroll_to_step
            .filter(|request| request.ticket == ticket)
        else {
            return;
        };
        self.nav.scroll_to_step = retry.map(|tries| StepRequest { tries, ..request });
    }

    pub(crate) fn set_top_step(&mut self, step: Option<StepSlot>) {
        self.nav.top_step = step;
    }

    pub(crate) fn set_outline_shown(&mut self, step: StepSlot) {
        self.nav.outline_shown = Some(step);
    }
}
