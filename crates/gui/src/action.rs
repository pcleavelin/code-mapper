use domain::{
    Author, Column, FileId, GroupName, Language, Line, LineCount, PathKind, PathName, Span,
    SymbolId,
};
use ui::{Count, Id, Label, Point, Px, Typed};

use crate::app::App;
use crate::dock::{DropTarget, Panel};
use crate::field::{Edit, Enter, FieldText, Which};
use crate::graph::GraphAction;
use crate::keys::{Extend, Walk};
use crate::model::{
    Context, Dirty, Hit, LeftTab, Openness, PathSlot, Readable, StepKey, StepSlot, Tab, ViewFlag,
    Warned,
};
use crate::nav::{Scrolling, Ticket, Tries};
use crate::peek::{HoverStep, Intent, Peek, Probe, Probing, WantedDefinition};
use crate::status::{Status, Under};
use crate::work::Request;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hide {
    Hide,
    Show,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fold {
    Fold,
    Unfold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContextChange {
    Above,
    Below,
    Reset,
}

pub(crate) enum Action {
    Focus(SymbolId),
    OpenPath(PathSlot, Tab),
    SelectStep(StepKey, Scrolling),
    Toggle(StepKey, ViewFlag),
    HideAll(PathSlot, Hide),
    FoldAll(PathSlot, Fold),
    RemoveStep(StepKey),
    RemovePath(PathSlot),
    GoTo(FileId, Line),
    Definition(FileId, Line, Column, Intent),
    SelectLine(Line, Extend),
    ClosePeek,
    Context(StepKey, ContextChange),
    ToggleDirectory(Label),
    OpenGroup(GroupName, Openness),
    Tab(Tab),
    LeftTab(LeftTab),
    Back,
    Forward,
    Save,
    PinSelection,
    Scroll(Id, Px),
    ScrollAcross(Id, Px),
    ScrolledToLine(Ticket),
    ScrolledToStep(Ticket, Option<Tries>),
    TopStep(Option<StepSlot>),
    OutlineShown(StepSlot),
    OutputScrolled,
    TipShown(Option<Label>),
    Status(Status),
    RefreshBase,
    Hover(Language, Probe),
    AskReferences(Language, Probe),
    FocusField(Which),
    Grab(Panel, Point),
    Resize(Panel, Px),
    DragDock(Point),
    Release(Option<DropTarget>),
    Type(Which, Vec<Edit>, Typed),
    WalkWhenIdle(Walk),
    Graph(GraphAction),
}

impl App {
    pub(crate) fn apply(&mut self, action: Action) {
        let model = &mut self.model;
        match action {
            Action::Focus(symbol) => model.go_to_symbol(symbol),
            Action::OpenPath(path, tab) => model.open_path(path, tab),
            Action::SelectStep(key, scrolling) => model.select_step(key, scrolling),
            Action::Toggle(key, flag) => model.views.entry(key).flags.toggle(flag),
            Action::HideAll(path, hide) => self.hide_all(path, hide),
            Action::FoldAll(path, fold) => self.fold_all(path, fold),
            Action::RemoveStep(key) => self.remove_step(key),
            Action::RemovePath(path) => self.remove_path(path),
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
            Action::Tab(tab) => model.set_tab(tab),
            Action::LeftTab(left) => model.set_left(left),
            Action::Back => model.back(),
            Action::Forward => model.forward(),
            Action::Save => self.save(),
            Action::PinSelection => self.pin_selection(),
            Action::Scroll(id, offset) => model.scrolls.set(id, offset),
            Action::ScrollAcross(id, offset) => model.across.set(id, offset),
            Action::ScrolledToLine(ticket) => model.scrolled_to_line(ticket),
            Action::ScrolledToStep(ticket, retry) => model.scrolled_to_step(ticket, retry),
            Action::TopStep(step) => model.set_top_step(step),
            Action::OutlineShown(step) => model.set_outline_shown(step),
            Action::OutputScrolled => {
                model.output_bottom = Count::new(model.output_bottom.get().saturating_sub(1));
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
            Action::Grab(panel, at) => model.dock.take_hold(panel, at),
            Action::Resize(panel, size) => model.dock.resize(panel, size),
            Action::DragDock(at) => model.dock.drag_to(at),
            Action::Release(target) => {
                if let Some(grab) = model.dock.release()
                    && let Some(target) = target
                {
                    model.dock.move_to(grab.panel, target.edge, target.at);
                }
            }
            Action::Type(which, edits, typed) => self.typed(which, &edits, &typed),
            Action::WalkWhenIdle(walk) => {
                if model.fields.focused().is_none() {
                    model.walk(walk);
                }
            }
            Action::Graph(action) => model.graph.apply(action),
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

    fn hide_all(&mut self, path: PathSlot, hide: Hide) {
        let model = &mut self.model;
        let count = model.step_count(path).get();
        for step in 0..count {
            model.views.entry(StepKey {
                path,
                step: StepSlot::new(step),
            });
        }
        let hide = hide == Hide::Hide;
        model.views.each_of_path(path, |step, view| {
            view.flags.set(ViewFlag::Hidden, hide && step.get() < count);
            let folded = view.flags.has(ViewFlag::Folded);
            view.flags.set(ViewFlag::Folded, folded && hide);
        });
    }

    fn fold_all(&mut self, path: PathSlot, fold: Fold) {
        let model = &mut self.model;
        let count = model.step_count(path).get();
        let with_children: Vec<bool> = (0..count)
            .map(|step| {
                fold == Fold::Fold
                    && model
                        .descendants(StepKey {
                            path,
                            step: StepSlot::new(step),
                        })
                        .get()
                        > 0
            })
            .collect();
        for step in 0..count {
            model.views.entry(StepKey {
                path,
                step: StepSlot::new(step),
            });
        }
        model.views.each_of_path(path, |step, view| {
            let folded = with_children.get(step.get()).copied().unwrap_or(false);
            view.flags.set(ViewFlag::Folded, folded);
        });
    }

    fn remove_step(&mut self, key: StepKey) {
        let model = &mut self.model;
        let number = model.number_of(key);
        let (Some(path), Some(step)) = (model.path(key.path), model.step(key)) else {
            return;
        };
        let name = path.name().clone();
        let id = step.id().clone();
        model.status = Status::StepRemoved {
            number,
            symbol: step.symbol().cloned(),
            file: step.file().clone(),
            path: name.clone(),
        };
        drop(model.map.remove_step(&name, &id));
        model.forget_step();
        model.step_removed(key);
        model.disk.dirty = Dirty::Unsaved;
    }

    fn remove_path(&mut self, slot: PathSlot) {
        let model = &mut self.model;
        let Some(name) = model.path(slot).map(|path| path.name().clone()) else {
            return;
        };
        match model.map.remove_path(&name) {
            Ok(removed) => {
                model.status = Status::PathRemoved {
                    name: removed.name().clone(),
                    steps: Count::new(removed.steps().len()),
                };
                model.forget_path();
                model.path_removed(slot);
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

    fn pin_selection(&mut self) {
        let model = &mut self.model;
        let (Tab::Listing, Some(file), Some(lines)) =
            (model.nav.tab(), model.nav.file(), model.nav.lines())
        else {
            model.status = Status::SelectLinesFirst;
            return;
        };
        let Some(path) = model.nav.path() else {
            model.status = Status::SelectPathFirst;
            return;
        };
        let Some(name) = model.path(path).map(|found| found.name().clone()) else {
            return;
        };
        let parent = model
            .nav
            .step()
            .filter(|step| step.get() < model.step_count(path).get());
        let parent_id = parent.and_then(|step| model.step_id(StepKey { path, step }));
        let Some(span) = Span::new(lines.low(), lines.high()) else {
            return;
        };
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
        model.step_created(step);
        model.disk.dirty = Dirty::Unsaved;
        let under = parent.map_or(Under::TopLevel, |parent| {
            Under::Step(model.number_of(StepKey { path, step: parent }))
        });
        model.status = Status::StepAdded {
            number: model.number_of(StepKey { path, step }),
            path: name,
            under,
        };
    }

    fn typed(&mut self, which: Which, edits: &[Edit], typed: &Typed) {
        let enter = match which {
            Which::Search | Which::SymbolFilter | Which::GoToLine => Enter::Keep,
            Which::NewPath | Which::Command => Enter::Clear,
        };
        let Some(line) = self.model.fields.handle(which, edits, typed, enter) else {
            return;
        };
        match which {
            Which::Command => self.run_command(&line),
            Which::Search => self.search(),
            Which::NewPath => self.create_path(&line),
            Which::GoToLine => self.go_to_line(&line),
            Which::SymbolFilter => {}
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
        model.results = model
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
            .take(5000)
            .collect();
        model.status = Status::Hits {
            count: Count::new(model.results.len()),
            pattern,
        };
        model.set_tab(Tab::Results);
    }

    fn create_path(&mut self, line: &FieldText) {
        let name = line.as_str().trim();
        if name.is_empty() {
            return;
        }
        let model = &mut self.model;
        let added = PathName::new(name).and_then(|valid| {
            model
                .map
                .add_path(valid.clone(), PathKind::Flow, Author::Human)
                .map(|_| valid)
        });
        let created = match added {
            Ok(created) => created,
            Err(error) => {
                model.status = Status::refused(&model.map, error);
                return;
            }
        };
        if let Some(slot) = model.find_path(&created) {
            model.path_created(slot);
        }
        model.status = Status::PathCreated(created);
        model.disk.dirty = Dirty::Unsaved;
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

    fn run_command(&mut self, line: &FieldText) {
        let line = line.as_str();
        let model = &mut self.model;
        if line.trim() == OutputCommand::Clear.name().as_str() {
            model.output.clear();
            return;
        }
        model.output.command(line);
        model.output_bottom = Count::new(2);
        let invocation = match cli::Invocation::parse(&cli::CommandLine::new(line).arguments()) {
            Ok(invocation) => invocation,
            Err(failure) => {
                let text = failure.text();
                model.status =
                    Status::CommandRejected(Label::new(text.as_str().lines().next().unwrap_or("")));
                model.output.rejected(text.as_str());
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
        model.output.append(output.as_str());
        match result {
            Ok(Some(_)) => {
                model.disk.dirty = Dirty::Unsaved;
                model.output.changed();
            }
            Ok(None) => {}
            Err(failure) => {
                let text = failure.to_string();
                model.status = Status::CommandFailed(Label::new(text.as_str()));
                model.output.failed(&text);
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
enum OutputCommand {
    Clear,
}

impl OutputCommand {
    const fn name(self) -> Literal {
        Literal(match self {
            Self::Clear => "clear",
        })
    }
}
