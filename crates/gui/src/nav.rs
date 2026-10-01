use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::mem;

use domain::{FileId, Line, RelativePath, StepId, SymbolId, SymbolKey, TourName};
use ui::{Count, Px};

use crate::field::Which;
use crate::graph::Camera;
use crate::ids;
use crate::keys::{Extend, Walk};
use crate::model::{LineSelection, Model, StepKey, StepSlot, Tab, TourSlot, ViewFlag};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Move {
    Select,
    Walk,
    Jump,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Going {
    Back,
    Forward,
}

#[derive(Clone, Debug, PartialEq)]
struct Viewport {
    tab: Tab,
    lines: Option<LineSelection>,
    document: Px,
    source: Px,
    camera: Camera,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Place {
    tour: Option<TourName>,
    step: Option<StepId>,
    focus: Option<SymbolKey>,
    file: Option<RelativePath>,
    view: Viewport,
}

impl Place {
    fn moved_from(&self, other: &Self, how: Move) -> bool {
        self.tour != other.tour
            || self.step != other.step
            || self.focus != other.focus
            || self.file != other.file
            || (how == Move::Jump && self.view.lines != other.view.lines)
    }
}

#[derive(Clone, Debug)]
struct History {
    back: Vec<Place>,
    forward: Vec<Place>,
    last: Option<Place>,
    last_move: Move,
}

impl Default for History {
    fn default() -> Self {
        Self {
            back: Vec::new(),
            forward: Vec::new(),
            last: None,
            last_move: Move::Select,
        }
    }
}

impl History {
    const LONGEST: Count = Count::new(200);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scrolling {
    Scroll,
    Stay,
}

#[derive(Clone, Debug)]
pub(crate) struct Nav {
    tab: Tab,
    tour: Option<TourSlot>,
    step: Option<StepSlot>,
    target: Option<StepSlot>,
    focus: Option<SymbolId>,
    file: Option<FileId>,
    lines: Option<LineSelection>,
    scroll_to: Option<LineRequest>,
    scroll_to_step: Option<StepRequest>,
    top_step: Option<StepSlot>,
    step_list_shown: Option<StepSlot>,
    ticket: Ticket,
    asked: Ticket,
    moving: Move,
    going: Option<Going>,
    history: History,
}

impl Default for Nav {
    fn default() -> Self {
        Self {
            tab: Tab::Tour,
            tour: None,
            step: None,
            target: None,
            focus: None,
            file: None,
            lines: None,
            scroll_to: None,
            scroll_to_step: None,
            top_step: None,
            step_list_shown: None,
            ticket: Ticket(0),
            asked: Ticket(0),
            moving: Move::Select,
            going: None,
            history: History::default(),
        }
    }
}

impl Nav {
    pub(crate) const fn tab(&self) -> Tab {
        self.tab
    }

    pub(crate) const fn tour(&self) -> Option<TourSlot> {
        self.tour
    }

    pub(crate) const fn step(&self) -> Option<StepSlot> {
        self.step
    }

    pub(crate) const fn target(&self) -> Option<StepSlot> {
        self.target
    }

    pub(crate) fn step_key(&self) -> Option<StepKey> {
        Some(StepKey {
            tour: self.tour?,
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

    pub(crate) const fn step_list_shown(&self) -> Option<StepSlot> {
        self.step_list_shown
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
            tour: nav
                .tour
                .and_then(|tour| self.tour(tour))
                .map(|tour| tour.name().clone()),
            step: nav.step_key().and_then(|key| self.step_id(key)),
            focus: nav.focus.and_then(|symbol| self.index.symbol_key(symbol)),
            file: nav
                .file
                .and_then(|file| self.index.file(file))
                .map(|file| file.path().clone()),
            view: Viewport {
                tab: nav.tab,
                lines: nav.lines,
                document: self.scrolls.get(ids::document()),
                source: self.scrolls.get(ids::source()),
                camera: self.graph.save_camera(&self.index),
            },
        }
    }

    pub(crate) const fn go_at_frame_end(&mut self, going: Going) {
        self.nav.going = Some(going);
    }

    pub(crate) fn track_navigation(&mut self) {
        match self.nav.going.take() {
            Some(Going::Back) => self.back(),
            Some(Going::Forward) => self.forward(),
            None => {}
        }
        let how = mem::replace(&mut self.nav.moving, Move::Select);
        let now = self.here();
        let history = &mut self.nav.history;
        if let Some(previous) = history.last.take()
            && now.moved_from(&previous, how)
        {
            let walking_on = how == Move::Walk && history.last_move == Move::Walk;
            if !walking_on {
                history.back.push(previous);
                if history.back.len() > History::LONGEST.get() {
                    history.back.remove(0);
                }
            }
            history.forward.clear();
            history.last_move = how;
        }
        history.last = Some(now);
    }

    pub(crate) const fn walked(&mut self) {
        self.nav.moving = Move::Walk;
    }

    fn reachable(&self, stack: &mut Vec<Place>) -> Option<Place> {
        let here = self.here();
        while let Some(place) = stack.pop() {
            let tour_exists = place
                .tour
                .as_ref()
                .is_none_or(|name| self.find_tour(name).is_some());
            if tour_exists && place.moved_from(&here, Move::Jump) {
                return Some(place);
            }
        }
        None
    }

    pub(crate) fn back(&mut self) {
        let mut back = mem::take(&mut self.nav.history.back);
        let found = self.reachable(&mut back);
        self.nav.history.back = back;
        let Some(place) = found else {
            return;
        };
        let here = self.here();
        self.nav.history.forward.push(here);
        self.go(place);
    }

    pub(crate) fn forward(&mut self) {
        let mut forward = mem::take(&mut self.nav.history.forward);
        let found = self.reachable(&mut forward);
        self.nav.history.forward = forward;
        let Some(place) = found else {
            return;
        };
        let here = self.here();
        self.nav.history.back.push(here);
        self.go(place);
    }

    fn go(&mut self, place: Place) {
        let tour = place.tour.as_ref().and_then(|name| self.find_tour(name));
        self.nav.tour = tour;
        let step = tour
            .zip(place.step.as_ref())
            .and_then(|(tour, id)| self.step_slot(tour, id));
        if let (Some(tour), Some(step)) = (tour, step) {
            self.select_step(StepKey { tour, step }, Scrolling::Stay);
        } else {
            self.nav.step = None;
            if let Some(symbol) = place.focus.and_then(|key| self.index.by_key(&key)) {
                self.focus_symbol(symbol);
            }
        }
        self.nav.file = place.file.and_then(|path| self.index.find_file(&path));
        self.nav.lines = place.view.lines;
        self.nav.scroll_to = None;
        self.nav.scroll_to_step = None;
        self.scrolls.set(ids::document(), place.view.document);
        self.scrolls.set(ids::source(), place.view.source);
        self.graph.restore_camera(&self.index, &place.view.camera);
        self.nav.show(place.view.tab);
        self.nav.history.last = Some(self.here());
    }

    pub(crate) fn step_removed(&mut self, removed: StepKey) {
        self.steps_moved(removed.tour, |step| match step.cmp(&removed.step) {
            Ordering::Equal => None,
            Ordering::Greater => Some(StepSlot::new(step.get() - 1)),
            Ordering::Less => Some(step),
        });
    }

    pub(crate) fn steps_moved(
        &mut self,
        tour: TourSlot,
        shift: impl Fn(StepSlot) -> Option<StepSlot>,
    ) {
        self.views.remap(|key| {
            if key.tour == tour {
                shift(key.step).map(|step| StepKey {
                    tour: key.tour,
                    step,
                })
            } else {
                Some(key)
            }
        });
        if self.nav.tour == Some(tour) {
            self.nav.step = self.nav.step.and_then(&shift);
            self.nav.target = self.nav.target.and_then(&shift);
            self.nav.top_step = self.nav.top_step.and_then(&shift);
            self.nav.step_list_shown = None;
        }
    }

    pub(crate) fn add_at_top_level(&mut self) {
        self.nav.target = None;
    }

    pub(crate) fn tour_removed(&mut self, removed: TourSlot) {
        let shift = |tour: TourSlot| match tour.cmp(&removed) {
            Ordering::Equal => None,
            Ordering::Greater => Some(TourSlot::new(tour.get() - 1)),
            Ordering::Less => Some(tour),
        };
        self.views.remap(|key| {
            shift(key.tour).map(|tour| StepKey {
                tour,
                step: key.step,
            })
        });
        self.nav.top_step = None;
    }

    pub(crate) fn walk(&mut self, walk: Walk) {
        self.walked();
        let Some(tour) = self
            .nav
            .tour
            .filter(|tour| tour.get() < self.tour_count().get())
        else {
            return;
        };
        let order = self.numbered(tour);
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
                    tour,
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
        if let Some(tour) = self.nav.tour {
            let on_tour = self.tour(tour).and_then(|found| {
                found.steps().iter().position(|step| {
                    step.resolved_symbol() == Some(symbol) && step.anchor().start().is_zero()
                })
            });
            if let Some(step) = on_tour {
                self.select_step(
                    StepKey {
                        tour,
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
        self.nav.tour = Some(key.tour);
        self.nav.step = Some(key.step);
        self.nav.target = Some(key.step);
        let mut seen = BTreeSet::new();
        let mut up = self.parent_of(key);
        while let Some(parent) = up.filter(|parent| seen.insert(*parent)) {
            let above = StepKey {
                tour: key.tour,
                step: parent,
            };
            if let Some(view) = self.views.existing(above) {
                view.flags.set(ViewFlag::Collapsed, false);
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
        if matches!(self.nav.tab, Tab::Source | Tab::Search | Tab::Diff) {
            self.nav.show(Tab::Tour);
        }
    }

    pub(crate) fn select_tour(&mut self, tour: TourSlot) {
        self.nav.tour = Some(tour);
        if let Some(placed) = self.tree_order(tour).first() {
            let step = placed.step;
            self.select_step(StepKey { tour, step }, Scrolling::Scroll);
        } else {
            self.nav.step = None;
            self.nav.target = None;
        }
    }

    pub(crate) fn open_tour(&mut self, tour: TourSlot, tab: Tab) {
        if self.nav.tour != Some(tour) {
            self.select_tour(tour);
        }
        self.nav.show(tab);
    }

    pub(crate) fn go_to_symbol(&mut self, symbol: SymbolId) {
        self.select_symbol(symbol);
        if self.nav.step.is_some() {
            return;
        }
        if !matches!(self.nav.tab, Tab::Graph | Tab::Tour) {
            self.nav.show(Tab::Source);
        } else if let Some(tour) = self.nav.tour {
            let name = self.index.symbol(symbol).map(|found| found.name().clone());
            let tour = self.tour(tour).map(|found| found.name().clone());
            if let (Some(name), Some(tour)) = (name, tour) {
                self.status = Status::OffTourSymbol { symbol: name, tour };
            }
        }
    }

    pub(crate) fn jumped_to_symbol(&mut self, symbol: SymbolId) {
        self.nav.moving = Move::Jump;
        self.select_symbol(symbol);
        let on_step = self.nav.step.is_some() && self.nav.tab == Tab::Tour;
        if self.nav.tab != Tab::Graph && !on_step {
            self.nav.show(Tab::Source);
        }
    }

    pub(crate) fn open_line(&mut self, file: FileId, line: Line) {
        self.nav.moving = Move::Jump;
        self.nav.file = Some(file);
        self.nav.lines = Some(LineSelection::one(line));
        self.nav.scroll_to_line(line);
        self.nav.show(Tab::Source);
    }

    pub(crate) fn go_to_line(&mut self, line: Line) {
        self.nav.moving = Move::Jump;
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

    pub(crate) fn tour_created(&mut self, tour: TourSlot) {
        self.nav.tour = Some(tour);
        self.nav.step = None;
        self.nav.target = None;
        self.nav.show(Tab::Tour);
    }

    pub(crate) fn forget_step(&mut self) {
        self.nav.step = None;
    }

    pub(crate) fn forget_tour(&mut self) {
        self.nav.tour = None;
        self.nav.step = None;
        self.nav.target = None;
    }

    pub(crate) fn reselect(&mut self, tour: Option<TourSlot>, step: Option<StepSlot>) {
        self.nav.tour = tour;
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

    pub(crate) fn set_step_list_shown(&mut self, step: StepSlot) {
        self.nav.step_list_shown = Some(step);
    }
}
