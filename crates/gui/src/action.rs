use domain::{Author, Column, FileId, GroupName, Language, Line, LineCount, SymbolId};
use ui::{Count, Id, Label, Point, Px, Typed};

use crate::app::App;
use crate::authoring::Authoring;
use crate::field::{Edit, Enter, FieldText, Which};
use crate::graph::{GraphAction, Heading};
use crate::ids;
use crate::keys::{Extend, PaletteKey, Walk};
use crate::model::{
    Context, Dirty, HIT_LIMIT, Hit, HitsShown, Measured, Openness, Readable, StepKey, StepSlot,
    Tab, TourSlot, ViewFlag, Warned,
};
use crate::nav::{Scrolling, Ticket, Tries};
use crate::palette::{Palette, PaletteAction};
use crate::panels::{BranchId, Direction, DropTarget, Ratio, View};
use crate::peek::{HoverStep, Intent, Peek, Probe, Probing, WantedDefinition};
use crate::status::Status;
use crate::work::Request;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hide {
    Hide,
    Show,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Collapse {
    Collapse,
    Expand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContextChange {
    Above,
    Below,
    Reset,
}

pub(crate) enum Action {
    Focus(SymbolId),
    Jump(SymbolId),
    OpenTour(TourSlot, Tab),
    SelectStep(StepKey, Scrolling),
    Toggle(StepKey, ViewFlag),
    HideAll(TourSlot, Hide),
    CollapseAll(TourSlot, Collapse),
    RemoveStep(StepKey),
    RemoveTour(TourSlot),
    GoTo(FileId, Line),
    Definition(FileId, Line, Column, Intent),
    SelectLine(Line, Extend),
    ClosePeek,
    Context(StepKey, ContextChange),
    ToggleDirectory(Label),
    OpenGroup(GroupName, Openness),
    ShowView(View),
    Back,
    Forward,
    Save,
    Authoring(Authoring),
    Scroll(Id, Px),
    ScrollAcross(Id, Px),
    Measured(Id, Measured),
    ScrolledToLine(Ticket),
    ScrolledToStep(Ticket, Option<Tries>),
    TopStep(Option<StepSlot>),
    StepListShown(StepSlot),
    ConsoleScrolled,
    TipShown(Option<Label>),
    Status(Status),
    RefreshBase,
    Hover(Language, Probe),
    AskReferences(Language, Probe),
    FocusField(Which),
    Grab(View, Point),
    Resize(BranchId, Ratio),
    DragView(Point),
    Release(Option<DropTarget>),
    SplitPanel(BranchId, Direction),
    ClosePanel(BranchId),
    TogglePicker(BranchId),
    Pick(BranchId, View),
    CloseView(View),
    ClosePicker,
    Palette(PaletteAction),
    Type(Which, Vec<Edit>, Typed),
    WalkWhenIdle(Walk),
    GraphWalk(Heading),
    Graph(GraphAction),
    Sequence(crate::sequence::SequenceAction),
    Atlas(crate::atlas::AtlasAction),
    Types(crate::types::TypesAction),
    Data(crate::dataflow::DataAction),
}

impl App {
    pub(crate) fn apply(&mut self, action: Action) {
        let model = &mut self.model;
        match action {
            Action::Focus(symbol) => model.go_to_symbol(symbol),
            Action::Jump(symbol) => model.jumped_to_symbol(symbol),
            Action::OpenTour(tour, tab) => model.open_tour(tour, tab),
            Action::SelectStep(key, scrolling) => model.select_step(key, scrolling),
            Action::Toggle(key, flag) => model.views.entry(key).flags.toggle(flag),
            Action::HideAll(tour, hide) => self.hide_all(tour, hide),
            Action::CollapseAll(tour, collapse) => self.collapse_all(tour, collapse),
            Action::RemoveStep(key) => self.remove_step(key),
            Action::RemoveTour(tour) => self.remove_tour(tour),
            Action::GoTo(file, line) => model.open_line(file, line),
            Action::Definition(file, line, column, intent) => {
                self.definition(file, line, column, intent);
            }
            Action::SelectLine(line, extend) => model.select_line(line, extend),
            Action::ClosePeek => model.peek = None,
            Action::Context(key, change) => {
                let context = &mut model.views.entry(key).context;
                let more =
                    |count: LineCount| LineCount::new(count.value() + Context::LINES.value());
                match change {
                    ContextChange::Reset => *context = Context::default(),
                    ContextChange::Above => context.above = more(context.above),
                    ContextChange::Below => context.below = more(context.below),
                }
            }
            Action::ToggleDirectory(directory) => {
                if !model.directories.remove(&directory) {
                    model.directories.insert(directory);
                }
            }
            Action::OpenGroup(group, openness) => {
                model.groups.insert(group, openness);
            }
            Action::ShowView(view) => model.show_view(view),
            Action::Back => model.back(),
            Action::Forward => model.forward(),
            Action::Save => self.save(),
            Action::Authoring(action) => self.author(action),
            Action::Scroll(id, offset) => model.scrolls.set(id, offset),
            Action::ScrollAcross(id, offset) => model.across.set(id, offset),
            Action::Measured(id, measured) => model.measures.set(id, measured),
            Action::ScrolledToLine(ticket) => model.scrolled_to_line(ticket),
            Action::ScrolledToStep(ticket, retry) => model.scrolled_to_step(ticket, retry),
            Action::TopStep(step) => model.set_top_step(step),
            Action::StepListShown(step) => model.set_step_list_shown(step),
            Action::ConsoleScrolled => {
                model.console_bottom = Count::new(model.console_bottom.get().saturating_sub(1));
            }
            Action::TipShown(tip) => model.tip_shown = tip,
            Action::Status(status) => model.status = status,
            Action::RefreshBase => self.load_base(),
            Action::Hover(language, probe) => self.hover(language, probe),
            Action::AskReferences(language, probe) => {
                if self.ask(language, Request::References(probe.clone())) {
                    self.model.queries.references_sent(probe);
                }
            }
            Action::FocusField(which) => model.fields.focus(which),
            Action::Grab(view, at) => model.panels.take_hold(view, at),
            Action::Resize(split, ratio) => model.panels.resize(split, ratio),
            Action::DragView(at) => model.panels.drag_to(at),
            Action::Release(target) => {
                if let Some(grab) = model.panels.release()
                    && let Some(target) = target
                {
                    model.panels.drop_view(grab.view, target);
                }
            }
            Action::SplitPanel(panel, direction) => {
                model.panels.split_panel(panel, direction);
                model.fields.start_empty(Which::ViewSearch);
            }
            Action::ClosePanel(panel) => model.panels.close(panel),
            Action::TogglePicker(panel) => {
                model.panels.toggle_picker(panel);
                if model.panels.picker().is_some() {
                    model.fields.start_empty(Which::ViewSearch);
                } else {
                    model.fields.release(Which::ViewSearch);
                }
            }
            Action::Pick(panel, view) => model.pick_view(panel, view),
            Action::CloseView(view) => model.panels.close_view(view),
            Action::ClosePicker => {
                model.panels.close_picker();
                model.fields.release(Which::ViewSearch);
            }
            Action::Palette(action) => self.palette(action),
            Action::Type(which, edits, typed) => self.typed(which, &edits, &typed),
            Action::WalkWhenIdle(walk) => {
                if model.fields.focused().is_none() {
                    model.walk(walk);
                }
            }
            Action::GraphWalk(heading) => self.graph_walk(heading),
            Action::Graph(action) => model.graph.apply(action),
            Action::Sequence(action) => model.sequence.apply(action),
            Action::Atlas(action) => model.atlas.apply(action),
            Action::Types(action) => model.types.apply(action),
            Action::Data(action) => model.data.apply(action),
        }
    }

    fn palette(&mut self, action: PaletteAction) {
        let model = &mut self.model;
        match action {
            PaletteAction::Toggle => {
                if model.palette.take().is_some() {
                    model.fields.release(Which::Palette);
                } else {
                    model.palette = Some(Palette::default());
                    model.fields.start_empty(Which::Palette);
                    model.refresh_palette();
                }
            }
            PaletteAction::Close => {
                model.palette = None;
                model.fields.release(Which::Palette);
            }
            PaletteAction::Key(PaletteKey::Move(walk)) => {
                if let Some(palette) = model.palette.as_mut() {
                    palette.walk(walk);
                }
            }
            PaletteAction::Key(PaletteKey::Run) => {
                let row = model.palette.as_ref().map(|palette| palette.selected);
                if let Some(row) = row {
                    self.run_palette(row);
                }
            }
            PaletteAction::Key(PaletteKey::Close) => self.palette(PaletteAction::Close),
            PaletteAction::Run(row) => self.run_palette(row),
        }
    }

    fn run_palette(&mut self, row: Count) {
        let goal = self
            .model
            .palette
            .as_ref()
            .and_then(|palette| palette.goal(row));
        self.palette(PaletteAction::Close);
        let Some(goal) = goal else {
            return;
        };
        for action in self.model.palette_actions(goal) {
            self.apply(action);
        }
    }

    fn hover(&mut self, language: Language, probe: Probe) {
        let now = self.model.now;
        if matches!(self.model.queries.hover_step(&probe, now), HoverStep::Ask)
            && self.ask(language, Request::Hover(probe))
        {
            self.model.queries.hover_sent();
        }
    }

    fn definition(&mut self, file: FileId, line: Line, column: Column, intent: Intent) {
        let Some(word) = self.model.word_at(file, line, column) else {
            return;
        };
        match self.model.probing(file, line, word.start) {
            Probing::Server { language, probe } => {
                if self.ask(language, Request::Definition(probe.clone())) {
                    self.model
                        .queries
                        .definition_sent(WantedDefinition { probe, intent });
                }
            }
            Probing::Resolver => match self.model.symbol_at(file, line, column) {
                Some(symbol) => self.land(Peek::Symbol(symbol), intent),
                None => self.model.status = Status::NoDefinitionOf(word.text),
            },
            Probing::Nothing => {}
        }
    }

    fn hide_all(&mut self, tour: TourSlot, hide: Hide) {
        let model = &mut self.model;
        let count = model.step_count(tour).get();
        for step in 0..count {
            model.views.entry(StepKey {
                tour,
                step: StepSlot::new(step),
            });
        }
        let hide = hide == Hide::Hide;
        model.views.each_of_tour(tour, |step, view| {
            view.flags.set(ViewFlag::Hidden, hide && step.get() < count);
            let collapsed = view.flags.has(ViewFlag::Collapsed);
            view.flags.set(ViewFlag::Collapsed, collapsed && hide);
        });
    }

    fn collapse_all(&mut self, tour: TourSlot, collapse: Collapse) {
        let model = &mut self.model;
        let count = model.step_count(tour).get();
        let with_children: Vec<bool> = (0..count)
            .map(|step| {
                collapse == Collapse::Collapse
                    && model
                        .descendants(StepKey {
                            tour,
                            step: StepSlot::new(step),
                        })
                        .get()
                        > 0
            })
            .collect();
        for step in 0..count {
            model.views.entry(StepKey {
                tour,
                step: StepSlot::new(step),
            });
        }
        model.views.each_of_tour(tour, |step, view| {
            let collapsed = with_children.get(step.get()).copied().unwrap_or(false);
            view.flags.set(ViewFlag::Collapsed, collapsed);
        });
    }

    fn remove_step(&mut self, key: StepKey) {
        let model = &mut self.model;
        let number = model.number_of(key);
        let (Some(tour), Some(step)) = (model.tour(key.tour), model.step(key)) else {
            return;
        };
        let name = tour.name().clone();
        let id = step.id().clone();
        model.status = Status::StepRemoved {
            number,
            symbol: step.symbol().cloned(),
            file: step.file().clone(),
            tour: name.clone(),
        };
        drop(model.map.remove_step(&name, &id));
        model.forget_step();
        model.step_removed(key);
        model.disk.dirty = Dirty::Unsaved;
    }

    fn remove_tour(&mut self, slot: TourSlot) {
        let model = &mut self.model;
        let Some(name) = model.tour(slot).map(|tour| tour.name().clone()) else {
            return;
        };
        match model.map.remove_tour(&name) {
            Ok(removed) => {
                model.status = Status::TourRemoved {
                    name: removed.name().clone(),
                    steps: Count::new(removed.steps().len()),
                };
                model.forget_tour();
                model.tour_removed(slot);
                model.disk.dirty = Dirty::Unsaved;
            }
            Err(error) => {
                model.status = Status::refused(&model.map, error);
            }
        }
    }

    pub(crate) fn save(&mut self) {
        let model = &mut self.model;
        if model.disk.readable == Readable::Broken {
            model.status = Status::SaveRefused;
            return;
        }
        match model.store.save(&model.map) {
            Ok(_) => {
                model.disk.dirty = Dirty::Clean;
                model.disk.warned = Warned::Quiet;
                model.disk.stamp = model.store.stamp();
                model.status = Status::Saved;
            }
            Err(error) => {
                model.status = Status::SaveFailed(Label::new(match &error {
                    io_map::MapSaveError::CreateDirectory { error, .. }
                    | io_map::MapSaveError::Write { error, .. }
                    | io_map::MapSaveError::Remove { error, .. } => error.to_string(),
                }));
            }
        }
    }

    fn typed(&mut self, which: Which, edits: &[Edit], typed: &Typed) {
        let enter = match which {
            Which::Search
            | Which::SymbolFilter
            | Which::TourFilter
            | Which::GoToLine
            | Which::NewTour
            | Which::NewGroup
            | Which::Palette => Enter::Keep,
            Which::Command | Which::ViewSearch => Enter::Clear,
        };
        let before = self.model.fields.get(which).text().clone();
        let entered = self.model.fields.handle(which, edits, typed, enter);
        if *self.model.fields.get(which).text() != before {
            match which {
                Which::SymbolFilter => self.model.scrolls.set(ids::symbols(), Px::ZERO),
                Which::TourFilter => self.model.scrolls.set(ids::tours(), Px::ZERO),
                _ => {}
            }
        }
        let Some(line) = entered else {
            return;
        };
        match which {
            Which::Command => self.run_command(&line),
            Which::Search => self.search(),
            Which::NewTour | Which::NewGroup => self.create_tour(),
            Which::GoToLine => self.go_to_line(&line),
            Which::ViewSearch => self.pick_first(&line),
            Which::SymbolFilter | Which::TourFilter | Which::Palette => {}
        }
    }

    fn search(&mut self) {
        let model = &mut self.model;
        let pattern = model.fields.get(Which::Search).text().label();
        let expression = match regex::Regex::new(pattern.as_str()) {
            Ok(expression) => expression,
            Err(error) => {
                let text = error.to_string();
                model.status =
                    Status::RegexRefused(Label::new(text.lines().last().unwrap_or("").trim()));
                return;
            }
        };
        model.hits = model
            .index
            .file_entries()
            .flat_map(|entry| {
                entry
                    .item
                    .text()
                    .all()
                    .iter()
                    .enumerate()
                    .filter(|pair| expression.is_match(pair.1.as_str()))
                    .map(move |pair| Hit {
                        file: entry.id,
                        line: Line::new(u32::try_from(pair.0).unwrap_or(0)),
                    })
                    .collect::<Vec<_>>()
            })
            .take(HIT_LIMIT.get() + 1)
            .collect();
        model.hits_shown = if model.hits.len() > HIT_LIMIT.get() {
            model.hits.truncate(HIT_LIMIT.get());
            HitsShown::First
        } else {
            HitsShown::All
        };
        model.status = Status::Hits {
            count: Count::new(model.hits.len()),
            pattern,
            shown: model.hits_shown,
        };
        model.set_tab(Tab::Search);
    }

    fn go_to_line(&mut self, line: &FieldText) {
        let text = line.as_str().trim();
        let model = &mut self.model;
        match (text.parse::<usize>(), model.nav.file()) {
            (Ok(wanted), Some(file)) => {
                let count = model
                    .index
                    .file(file)
                    .map_or(0, |source| source.text().count().value());
                let last = usize::try_from(count).unwrap_or(0).max(1);
                if wanted < 1 || wanted > last {
                    model.status = Status::LineOutside {
                        line: Label::new(wanted.to_string()),
                        last: LineCount::new(u32::try_from(last).unwrap_or(0)),
                    };
                }
                let at = wanted.clamp(1, last) - 1;
                model.go_to_line(Line::new(u32::try_from(at).unwrap_or(0)));
            }
            (Err(_), _) => model.status = Status::NoLineNumber(Label::new(text)),
            (_, None) => model.status = Status::NoFileOpen,
        }
    }

    fn pick_first(&mut self, line: &FieldText) {
        let model = &mut self.model;
        let Some(panel) = model.panels.picker() else {
            return;
        };
        if let Some(view) = View::matching(&line.label()).first() {
            model.pick_view(panel, *view);
        }
    }

    fn run_command(&mut self, line: &FieldText) {
        let line = line.as_str();
        let model = &mut self.model;
        if line.trim() == ConsoleCommand::Clear.name().as_str() {
            model.console.clear();
            return;
        }
        model.console.command(line);
        model.console_bottom = Count::new(2);
        let invocation = match cli::Invocation::parse(&cli::CommandLine::new(line).arguments()) {
            Ok(invocation) => invocation,
            Err(failure) => {
                let text = failure.text();
                model.status =
                    Status::CommandRejected(Label::new(text.as_str().lines().next().unwrap_or("")));
                model.console.rejected(text.as_str());
                return;
            }
        };
        let mut output = cli::Output::new();
        let result = cli::exec(
            &mut model.index,
            &mut model.map,
            invocation,
            Author::Human,
            None,
            &mut output,
        );
        model.console.append(output.as_str());
        match result {
            Ok(Some(_)) => {
                model.disk.dirty = Dirty::Unsaved;
                model.console.changed();
            }
            Ok(None) => {}
            Err(failure) => {
                let text = failure.to_string();
                model.status = Status::CommandFailed(Label::new(text.as_str()));
                model.console.failed(&text);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Literal(&'static str);

impl Literal {
    const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConsoleCommand {
    Clear,
}

impl ConsoleCommand {
    const fn name(self) -> Literal {
        Literal(match self {
            Self::Clear => "clear",
        })
    }
}
