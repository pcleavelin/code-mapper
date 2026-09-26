use std::env;
use std::path::PathBuf;
use std::time::Duration;

use domain::{Line, Map, RelativePath, Root};
use io_map::MapStore;
use platform::{Cursor, Exit, Frame as PlatformFrame, Outcome, Renderer, ScriptLine};
use ui::{Count, Id, Input, Label, Measure, Point, Px, Rect, Ui};

use crate::action::Action;
use crate::dump::{self, Context, Dump, DumpLines};
use crate::field::Which;
use crate::grid::Grids;
use crate::ids;
use crate::keys::{self, Going};
use crate::model::{Metrics, Model, PathSlot, Readable, Tab, TabName};
use crate::status::Status;
use crate::theme::{self, BACKGROUND, TEXT};
use crate::views;
use crate::widgets::{Frame, Overlay};
use crate::work::Services;

struct Shot {
    path: PathBuf,
    frame: Count,
}

pub(crate) struct App {
    pub(crate) model: Model,
    pub(crate) services: Services,
    pub(crate) ui: Ui,
    pub(crate) grids: Grids,
    shot: Option<Shot>,
    shot_next: Option<PathBuf>,
}

const SHOT_TAB_FRAME: Count = Count::new(3);
const SHOT_SCROLL_FRAME: Count = Count::new(6);
const SHOT_FRAME: Count = Count::new(20);
const BUSY: Duration = Duration::from_millis(50);
const IDLE: Duration = Duration::from_millis(1000);

impl App {
    pub(crate) fn new(root: &Root) -> Self {
        let indexed = index::build(root);
        let mut store = MapStore::new(root);
        let (mut map, unreadable) = match store.load() {
            Ok(map) => (map, None),
            Err(error) => (Map::default(), Some(error)),
        };
        map.resolve_all(&indexed.index);
        let readable = if unreadable.is_some() {
            Readable::Broken
        } else {
            Readable::Reads
        };
        let model = Model::new(indexed.index, map, store, readable);
        let mut app = Self {
            model,
            services: Services::new(root),
            ui: Ui::default(),
            grids: Grids::default(),
            shot: env::var_os("CODEMAP_SHOT").map(|path| Shot {
                path: PathBuf::from(path),
                frame: Count::ZERO,
            }),
            shot_next: None,
        };
        app.model.status = match unreadable {
            Some(error) => Status::MapUnreadable(Label::new(cli::Failure::Load(error).to_string())),
            None => app.indexed_status(),
        };
        app.services.watch(indexed.stamps);
        if !app.model.map.paths().is_empty() {
            app.model.select_path(PathSlot::new(0));
        }
        app.start_backend();
        app.load_base();
        app
    }

    #[cfg(test)]
    pub(crate) fn of_model(model: Model, root: &Root) -> Self {
        Self {
            model,
            services: Services::new(root),
            ui: Ui::default(),
            grids: Grids::default(),
            shot: None,
            shot_next: None,
        }
    }

    fn shoot(&mut self, renderer: &mut Renderer) -> Exit {
        let mut exit = Exit::Stay;
        if let Some(shot) = self.shot.as_mut() {
            shot.frame += Count::new(1);
            let frame = shot.frame;
            let path = shot.path.clone();
            if frame == SHOT_SCROLL_FRAME
                && let Some(offset) = env::var("CODEMAP_SHOT_SCROLL")
                    .ok()
                    .and_then(|value| value.parse::<i32>().ok())
            {
                self.apply(Action::Scroll(ids::document(), Px::new(offset)));
            }
            if frame == SHOT_TAB_FRAME {
                let name = env::var("CODEMAP_SHOT_TAB").unwrap_or_default();
                self.apply(Action::Tab(Tab::from_name(&TabName::new(&name))));
            }
            if frame == SHOT_FRAME {
                renderer.shoot(path);
            }
            if frame > SHOT_FRAME {
                exit = Exit::Quit;
            }
        }
        if let Some(path) = self.shot_next.take() {
            renderer.shoot(path);
        }
        exit
    }

    fn keys(&mut self, input: &Input) {
        let edits = keys::edits(input);
        let typed = &input.typed;
        let mut actions = Vec::new();
        if keys::save(input) {
            actions.push(Action::Save);
        }
        if keys::going(input, Going::Back) {
            actions.push(Action::Back);
        }
        if keys::going(input, Going::Forward) {
            actions.push(Action::Forward);
        }
        for which in [Which::Command, Which::Search, Which::NewPath] {
            actions.push(Action::Type(which, edits.clone(), typed.clone()));
        }
        for walk in keys::walks(input) {
            actions.push(Action::WalkWhenIdle(walk));
        }
        for which in [Which::SymbolFilter, Which::GoToLine] {
            actions.push(Action::Type(which, edits.clone(), typed.clone()));
        }
        for action in actions {
            self.apply(action);
        }
    }

    fn dump(&self) {
        let mut lines = DumpLines::default();
        let model = &self.model;
        let context = Context {
            model,
            ui: &self.ui,
            services: &self.services,
        };
        let parts: [&dyn Dump; 6] = [
            &model.nav,
            &model.scrolls,
            &model.dock,
            &model.status,
            &model.work,
            &model.graph,
        ];
        for part in parts {
            part.dump(&context, &mut lines);
        }
        lines.print();
    }
}

impl platform::App for App {
    fn frame(&mut self, renderer: &mut Renderer, input: &mut Input) -> PlatformFrame {
        let font = theme::font(renderer.scale());
        self.model.metrics = Metrics {
            font,
            cell: renderer.cell(font),
        };
        let exit = self.shoot(renderer);
        self.poll_backend();
        self.poll_base();
        self.poll_reindex();
        self.poll_link();
        self.poll_disk();
        self.model.now = input.time;
        self.keys(input);
        self.ui.begin(input);
        for action in views::dock_input(&self.model, &self.ui) {
            self.apply(action);
        }
        let graph = (self.model.nav.tab() == Tab::Graph).then(|| self.graph_phase(renderer));
        let mut queue = Vec::new();
        let mut frame = Frame {
            ui: &mut self.ui,
            queue: &mut queue,
            grids: &mut self.grids,
            metrics: self.model.metrics,
            overlay: Overlay::default(),
            tooltip: None,
            cursor: Cursor::Default,
        };
        views::build(&self.model, &mut frame, graph);
        let cursor = frame.cursor;
        self.ui.end(renderer);
        let drawing = self.ui.draw(renderer, TEXT);
        for action in queue {
            self.apply(action);
        }
        self.model.track_navigation();
        let busy = self.working() || self.shot.is_some();
        PlatformFrame {
            redraw_after: if busy { BUSY } else { IDLE },
            exit,
            clear: BACKGROUND,
            cursor,
            drawing,
        }
    }

    fn script(&mut self, line: &ScriptLine) -> Outcome {
        match line.word(0).unwrap_or_default() {
            "idle" => {
                if self.working() || self.model.work.unmerged().get() > 0 {
                    return Outcome::Retry;
                }
            }
            "rect" => {
                let mut lines = DumpLines::default();
                lines.rect(&self.ui, line);
                lines.print();
            }
            "tab" => {
                let name = TabName::new(line.word(1).unwrap_or_default());
                self.apply(Action::Tab(Tab::from_name(&name)));
            }
            "scroll" => {
                if let (Some(name), Some(offset)) = (
                    line.word(1),
                    line.word(2).and_then(|value| value.parse::<i32>().ok()),
                ) {
                    self.apply(Action::Scroll(Id::new(name), Px::new(offset)));
                }
            }
            "shot" => self.shot_next = line.word(1).map(PathBuf::from),
            "open" => {
                if let Some(file) = line
                    .word(1)
                    .and_then(|path| self.model.index.find_file(&RelativePath::new(path)))
                {
                    let number = line
                        .word(2)
                        .and_then(|value| value.parse::<u32>().ok())
                        .unwrap_or(0);
                    self.apply(Action::GoTo(file, Line::new(number)));
                }
            }
            "dump" => self.dump(),
            _ => dump::report(format_args!("script: unknown command '{line}'")),
        }
        Outcome::Done
    }

    fn locate(&mut self, id: Id) -> Option<Point> {
        self.ui.interaction(id).rect().map(Rect::center)
    }
}
