use domain::{Author, FileId, Map, Pruning, Span, StepId, SymbolId, SymbolName, TourName};
use ui::{Count, Label, Point, Px, Rect, Run};

use crate::app::App;
use crate::model::{Dirty, Model, StepKey, StepSlot, Tab, TourSlot};
use crate::status::{Status, Under};
use crate::theme::{
    ACCENT, DROP_BAND_WIDTH, GRAB_REACH, LABEL_PADDING, SMALL_BUTTON_EXTRA, SMALL_GAP, TEXT, WEAK,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Authoring {
    AddSymbol(SymbolId, Hang),
    AddOffered,
    AddAtTopLevel,
    GrabStep(StepKey, Point),
    DragStep(Point),
    DropStep(Option<StepDrop>),
    Promote(SymbolId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hang {
    Target,
    Under(StepSlot),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AddOffer {
    Lines(Label),
    Symbol(SymbolId, Label),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StripAct {
    AddAtTopLevel,
    AddOffered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StripButton {
    pub(crate) act: StripAct,
    pub(crate) text: Label,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TargetStrip {
    pub(crate) wordings: Vec<Vec<Run>>,
    pub(crate) buttons: Vec<StripButton>,
    pub(crate) aside: Option<Label>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StripRow<'strip> {
    pub(crate) runs: &'strip [Run],
    pub(crate) buttons: &'strip [StripButton],
    pub(crate) aside: Option<&'strip Label>,
}

fn text_width(cell_width: Px, text: &Label) -> Px {
    cell_width * Count::new(text.columns())
}

fn label_width(cell_width: Px, text: &Label) -> Px {
    text_width(cell_width, text) + LABEL_PADDING * 2
}

fn button_width(cell_width: Px, text: &Label) -> Px {
    text_width(cell_width, text) + SMALL_BUTTON_EXTRA
}

fn gapped_width(widths: impl IntoIterator<Item = Px>) -> Px {
    widths
        .into_iter()
        .enumerate()
        .fold(Px::ZERO, |sum, (position, width)| {
            sum + width + if position == 0 { Px::ZERO } else { SMALL_GAP }
        })
}

impl TargetStrip {
    pub(crate) fn rows(&self, cell_width: Px, room: Option<Px>) -> Vec<StripRow<'_>> {
        let fits = |width: Px| room.is_none_or(|room| width <= room);
        let wording_widths = |wording: &[Run]| -> Vec<Px> {
            wording
                .iter()
                .map(|run| label_width(cell_width, &run.text))
                .collect()
        };
        let buttons: Vec<Px> = self
            .buttons
            .iter()
            .map(|button| button_width(cell_width, &button.text))
            .collect();
        let aside_after = |mut widths: Vec<Px>| {
            self.aside.as_ref().filter(|aside| {
                widths.push(label_width(cell_width, aside));
                fits(gapped_width(widths))
            })
        };
        if let Some(wording) = self.wordings.iter().find(|wording| {
            fits(gapped_width(
                wording_widths(wording)
                    .into_iter()
                    .chain(buttons.iter().copied()),
            ))
        }) {
            let mut widths = wording_widths(wording);
            widths.extend(buttons.iter().copied());
            return vec![StripRow {
                runs: wording,
                buttons: &self.buttons,
                aside: aside_after(widths),
            }];
        }
        let wording = self
            .wordings
            .iter()
            .find(|wording| fits(gapped_width(wording_widths(wording))))
            .or_else(|| self.wordings.last())
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut rows = vec![StripRow {
            runs: wording,
            buttons: &[],
            aside: None,
        }];
        if self.buttons.is_empty() {
            return rows;
        }
        if fits(gapped_width(buttons.iter().copied())) {
            rows.push(StripRow {
                runs: &[],
                buttons: &self.buttons,
                aside: aside_after(buttons),
            });
        } else {
            rows.extend(self.buttons.chunks(1).map(|button| StripRow {
                runs: &[],
                buttons: button,
                aside: None,
            }));
        }
        rows
    }
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
    fn tour_name(&self, tour: TourSlot) -> Option<TourName> {
        self.tour(tour).map(|found| found.name().clone())
    }

    fn step_ids(&self, tour: TourSlot) -> Vec<StepId> {
        self.tour(tour).map_or_else(Vec::new, |found| {
            found.steps().iter().map(|step| step.id().clone()).collect()
        })
    }

    fn under_label(&self, tour: TourSlot, parent: Option<StepSlot>) -> Under {
        parent.map_or(Under::TopLevel, |step| {
            Under::Step(self.number_of(StepKey { tour, step }))
        })
    }

    pub(crate) fn target_under(&self) -> Option<StepSlot> {
        let tour = self.nav.tour()?;
        self.nav
            .target()
            .filter(|step| step.get() < self.step_count(tour).get())
    }

    pub(crate) fn target_strip(&self) -> TargetStrip {
        let Some((tour, found)) = self
            .nav
            .tour()
            .and_then(|tour| Some((tour, self.tour(tour)?)))
        else {
            return TargetStrip {
                wordings: vec![
                    vec![Run::new("open a tour to add steps to it", WEAK)],
                    vec![Run::new("no tour open", WEAK)],
                ],
                buttons: Vec::new(),
                aside: None,
            };
        };
        let name = found.name().as_str();
        let mut buttons = Vec::new();
        let wordings = match self.target_under() {
            Some(step) => {
                let key = StepKey { tour, step };
                let symbol = self
                    .step(key)
                    .and_then(domain::Step::symbol)
                    .map_or("(lines)", SymbolName::as_str);
                let place = format!("{} {symbol}", self.number_of(key).as_str());
                buttons.push(StripButton {
                    act: StripAct::AddAtTopLevel,
                    text: Label::new("top level"),
                });
                vec![
                    vec![
                        Run::new("adds to", WEAK),
                        Run::new(name, TEXT),
                        Run::new(format!("under {place}"), ACCENT),
                    ],
                    vec![
                        Run::new(name, TEXT),
                        Run::new(format!("\u{203a} {place}"), ACCENT),
                    ],
                    vec![Run::new(place, ACCENT)],
                ]
            }
            None => vec![
                vec![
                    Run::new("adds to", WEAK),
                    Run::new(name, TEXT),
                    Run::new("at the top level", ACCENT),
                ],
                vec![Run::new(name, TEXT), Run::new("\u{203a} top level", ACCENT)],
                vec![Run::new("top level", ACCENT)],
            ],
        };
        let aside = match self.add_offer() {
            Some(AddOffer::Lines(text) | AddOffer::Symbol(_, text)) => {
                buttons.push(StripButton {
                    act: StripAct::AddOffered,
                    text,
                });
                None
            }
            None => Some(Status::select_symbol_or_lines()),
        };
        TargetStrip {
            wordings,
            buttons,
            aside,
        }
    }

    pub(crate) fn add_offer(&self) -> Option<AddOffer> {
        self.nav.tour()?;
        if self.nav.tab() == Tab::Source
            && let Some(lines) = self.nav.lines()
        {
            let label = if lines.low() == lines.high() {
                format!("add line {} as a step", lines.low().number())
            } else {
                format!(
                    "add lines {}-{} as a step",
                    lines.low().number(),
                    lines.high().number()
                )
            };
            return Some(AddOffer::Lines(Label::new(label)));
        }
        let symbol = self.nav.focus()?;
        let on_step = self
            .nav
            .step_key()
            .and_then(|key| self.step(key))
            .and_then(domain::Step::resolved_symbol)
            == Some(symbol);
        if on_step {
            return None;
        }
        let name = self.index.symbol(symbol)?.name();
        Some(AddOffer::Symbol(
            symbol,
            Label::new(format!("add {name} as a step")),
        ))
    }
}

impl App {
    pub(crate) fn author(&mut self, action: Authoring) {
        match action {
            Authoring::AddSymbol(symbol, hang) => self.add_symbol(symbol, hang),
            Authoring::AddOffered => self.add_offered(),
            Authoring::AddAtTopLevel => self.add_at_top_level(),
            Authoring::GrabStep(key, from) => self.grab_step(key, from),
            Authoring::DragStep(mouse) => self.drag_step(mouse),
            Authoring::DropStep(target) => self.drop_step(target),
            Authoring::Promote(symbol) => self.promote(symbol),
        }
    }

    pub(crate) fn add_offered(&mut self) {
        match self.model.add_offer() {
            Some(AddOffer::Lines(_)) => self.add_lines(),
            Some(AddOffer::Symbol(symbol, _)) => self.add_symbol(symbol, Hang::Target),
            None if self.model.nav.tour().is_none() => {
                self.model.status = Status::SelectTourFirst;
            }
            None => self.model.status = Status::SelectSymbolOrLinesFirst,
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
        let Some(tour) = model.nav.tour() else {
            model.status = Status::SelectTourFirst;
            return;
        };
        let Some(name) = model.tour_name(tour) else {
            return;
        };
        let parent = match hang {
            Hang::Target => model.target_under(),
            Hang::Under(step) => {
                Some(step).filter(|step| step.get() < model.step_count(tour).get())
            }
        };
        let parent_id = parent.and_then(|step| model.step_id(StepKey { tour, step }));
        let file_path = model.index.file(file).map(|source| source.path().clone());
        let existing = model.tour(tour).and_then(|found| {
            found.steps().iter().position(|step| {
                Some(step.file()) == file_path.as_ref()
                    && step.span() == span
                    && step.parent() == parent_id.as_ref()
            })
        });
        if let Some(existing) = existing {
            model.status = Status::AlreadyStep {
                number: model.number_of(StepKey {
                    tour,
                    step: StepSlot::new(existing),
                }),
                tour: name,
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
        let Some(step) = model.step_slot(tour, &id) else {
            return;
        };
        model.disk.dirty = Dirty::Unsaved;
        model.status = Status::StepAdded {
            number: model.number_of(StepKey { tour, step }),
            tour: name,
            under: model.under_label(tour, parent),
        };
    }

    pub(crate) fn add_at_top_level(&mut self) {
        let model = &mut self.model;
        model.add_at_top_level();
        if let Some(name) = model.nav.tour().and_then(|tour| model.tour_name(tour)) {
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
        let tour = grab.key.tour;
        let onto_key = StepKey {
            tour,
            step: drop.onto,
        };
        let (Some(name), Some(moved), Some(onto)) = (
            model.tour_name(tour),
            model.step_id(grab.key),
            model.step_id(onto_key),
        ) else {
            return;
        };
        let before = model.step_ids(tour);
        let onto_parent = model.step(onto_key).and_then(|step| step.parent().cloned());
        let sibling_after = || {
            model.tour(tour).and_then(|found| {
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
        let after = model.step_ids(tour);
        model.steps_moved(tour, |slot| {
            before
                .get(slot.get())
                .and_then(|id| after.iter().position(|other| other == id))
                .map(StepSlot::new)
        });
        model.disk.dirty = Dirty::Unsaved;
        let Some(step) = model.step_slot(tour, &moved) else {
            return;
        };
        let parent = parent.and_then(|id| model.step_slot(tour, &id));
        model.status = Status::StepPlaced {
            number: model.number_of(StepKey { tour, step }),
            tour: name,
            under: model.under_label(tour, parent),
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
        if let Some(slot) = model.find_tour(&name) {
            model.status = Status::TourPromoted {
                name,
                steps: model.step_count(slot),
            };
            model.select_tour(slot);
            model.set_tab(Tab::Tour);
        }
    }
}
