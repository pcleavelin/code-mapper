mod dock;
mod document;
mod graph;
mod nav;
mod panels;
mod peek;
mod views;
mod widgets;
mod work;

use self::{dock::*, document::*, nav::*, panels::*, peek::*, widgets::*, work::*};
use crate::cli;
use crate::gfx::{Color, Gfx, Glyphs, Rect};
use crate::index::{self, Index, ServerFile, SymRef};
use crate::lsp;
use crate::map::{Author, Change, Kind as PathKind, Map, PathDiff, Row, StepChange};
use crate::ui::{
    self, Align, BORDER_BOTTOM, BORDER_LEFT, BORDER_RIGHT, BORDER_TOP, Id, Interaction, Key, Kind,
    Layout, Measure, Style, Text, Ui,
};
use crate::window;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Path,
    Diff,
    Graph,
    Listing,
    Results,
}

impl Tab {
    fn from_name(name: &str) -> Tab {
        match name {
            "graph" => Tab::Graph,
            "listing" => Tab::Listing,
            "diff" => Tab::Diff,
            "results" => Tab::Results,
            _ => Tab::Path,
        }
    }
}

pub enum Action {
    Focus(SymRef),
    OpenPath(usize, Tab),
    SelectStep(usize, usize, bool),
    ToggleWhole(usize, usize),
    ToggleCode(usize, usize),
    ToggleFold(usize, usize),
    ToggleLink(usize, usize),
    CollapseAll(usize, bool),
    FoldAll(usize, bool),
    DeleteStep(usize, usize),
    DeletePath(usize),
    GoTo(usize, usize),
    Jump(usize, usize, usize),
    PeekAt(usize, usize, usize),
    SelectLine(usize, bool),
    ClosePeek,
    Context(usize, usize, i8),
    ToggleDir(String),
    OpenGroup(String, bool),
    Tab(Tab),
    Back,
    Forward,
    Save,
    PinSelection,
}

pub struct App {
    pub idx: Index,
    work: Work,
    lookup: Lookup,
    now: f64,
    grids: HashMap<(usize, u64, usize, usize), Rc<Glyphs>>,
    hscroll: HashMap<Id, i32>,
    output_bottom: u8,
    pub map: Map,
    base: Option<Map>,
    base_why: String,
    file: MapFile,

    pub focus: Option<SymRef>,
    pub sel_path: Option<usize>,
    pub sel_anchor: Option<usize>,
    pub cur_file: Option<usize>,
    pub sel: Option<(usize, usize)>,
    scroll_to: Option<usize>,
    scroll_to_step: Option<(usize, u8)>,
    top_step: Option<usize>,
    outline_shown: Option<usize>,

    steps: HashMap<(usize, usize), StepView>,
    dir_toggled: HashSet<String>,
    groups_open: HashMap<String, bool>,
    peek: Option<Peek>,
    history: History,

    pub ui: Ui,
    pub px: u32,
    pub cell: (i32, i32),
    pub tab: Tab,
    left: LeftTab,
    dock: Dock,
    pub scrolls: HashMap<Id, i32>,
    pub actions: Vec<Action>,
    pub graph: graph::Graph,
    pub tooltip: Option<(Tip, (i32, i32))>,
    pub tip_shown: Option<String>,

    fields: [Field; 5],
    results: Vec<(usize, usize)>,
    output: String,
    status: String,
    shot: Option<(PathBuf, u32)>,
    shot_next: Option<PathBuf>,
}

impl App {
    pub fn new(root: &Path) -> App {
        let idx = index::build(root);
        let map_path = root.join(crate::map::MAP_DIR);
        let (mut map, unreadable) = match Map::load(&map_path) {
            Ok(m) => (m, None),
            Err(e) => (Map::default(), Some(e)),
        };
        map.resolve_all(&idx);
        let first_path = if map.paths.is_empty() { None } else { Some(0) };
        let mut app = App {
            work: Work::new(&idx.root),
            idx,
            lookup: Lookup::default(),
            now: 0.0,
            grids: HashMap::new(),
            hscroll: HashMap::new(),
            output_bottom: 0,
            map,
            base: None,
            base_why: String::new(),
            file: MapFile {
                stamp: crate::map::stamp(&map_path),
                broken: unreadable.is_some(),
                path: map_path,
                dirty: false,
                last_poll: Instant::now(),
                warned: false,
            },
            focus: None,
            sel_path: None,
            sel_anchor: None,
            cur_file: None,
            sel: None,
            scroll_to: None,
            scroll_to_step: None,
            top_step: None,
            outline_shown: None,
            steps: HashMap::new(),
            dir_toggled: HashSet::new(),
            groups_open: HashMap::new(),
            peek: None,
            history: History::default(),
            ui: Ui::default(),
            px: 14,
            cell: (8, 16),
            tab: Tab::Path,
            left: LeftTab::Paths,
            dock: Dock::default(),
            scrolls: HashMap::new(),
            actions: Vec::new(),
            graph: graph::Graph::new(),
            tooltip: None,
            tip_shown: None,
            fields: Default::default(),
            results: Vec::new(),
            output: "type 'help' for commands; roots and promote live here\n".into(),
            status: String::new(),
            shot: std::env::var_os("CODEMAP_SHOT").map(|p| (PathBuf::from(p), 0)),
            shot_next: None,
        };
        app.status = match unreadable {
            Some(e) => format!("{e}; the map is not shown and cannot be saved until it reads"),
            None => app.indexed_status(),
        };
        app.watch_files();
        if let Some(pi) = first_path {
            app.select_path(pi);
        }
        app.start_backend();
        app.load_base();
        app
    }

    fn save(&mut self) {
        if self.file.broken {
            self.status = "not saved: the map on disk does not read; fix it and it reloads".into();
            return;
        }
        match self.map.save(&self.file.path) {
            Ok(()) => {
                self.file.dirty = false;
                self.file.warned = false;
                self.file.stamp = crate::map::stamp(&self.file.path);
                self.status = "saved".into();
            }
            Err(e) => self.status = format!("save FAILED: {e}"),
        }
    }

    fn run_cmd(&mut self, line: String) {
        if line.trim() == "clear" {
            self.output.clear();
            return;
        }
        self.output.push_str(&format!("> {line}\n"));
        self.output_bottom = 2;
        let cmd = match cli::parse(&cli::tokenize(&line)) {
            Ok(cmd) => cmd,
            Err(e) => {
                let e = e.to_string();
                self.status = e.lines().next().unwrap_or("").to_owned();
                self.output.push_str(e.trim_end());
                self.output.push('\n');
                return;
            }
        };
        match cli::exec(
            &mut self.idx,
            &mut self.map,
            cmd,
            Author::Human,
            None,
            &mut self.output,
        ) {
            Ok(true) => {
                self.file.dirty = true;
                self.output.push_str("(map changed, ctrl+s to save)\n");
            }
            Ok(false) => {}
            Err(e) => {
                self.status = format!("error: {e}");
                self.output.push_str(&format!("error: {e}\n"));
            }
        }
    }

    fn run_search(&mut self) {
        let re = match regex::Regex::new(&self.fields[Which::Search as usize].text) {
            Ok(re) => re,
            Err(e) => {
                self.status = format!(
                    "bad regex: {}",
                    e.to_string().lines().last().unwrap_or("").trim()
                );
                return;
            }
        };
        self.results = self
            .idx
            .files
            .iter()
            .enumerate()
            .flat_map(|(fi, f)| {
                f.lines
                    .iter()
                    .enumerate()
                    .map(move |(li, line)| (fi, li, line))
            })
            .filter(|(_, _, line)| re.is_match(line))
            .map(|(fi, li, _)| (fi, li))
            .take(5000)
            .collect();
        self.status = format!(
            "{} hits for /{}/",
            self.results.len(),
            self.fields[Which::Search as usize].text
        );
        self.tab = Tab::Results;
    }

    fn create_path(&mut self, name: String) {
        if name.is_empty() {
            return;
        }
        match self.map.add_path(&name, PathKind::Flow, Author::Human) {
            Ok(pi) => self.sel_path = Some(pi),
            Err(e) => {
                self.status = e;
                return;
            }
        }
        self.sel_anchor = None;
        self.tab = Tab::Path;
        self.status = format!("path '{name}' created (unsaved)");
        self.file.dirty = true;
    }

    fn add_selection(&mut self) {
        let (Tab::Listing, Some(fi), Some((a, b))) = (self.tab, self.cur_file, self.sel) else {
            self.status = "select lines in the listing first".into();
            return;
        };
        let Some(pi) = self.sel_path else {
            self.status = "select a path first".into();
            return;
        };
        let parent = self
            .sel_anchor
            .filter(|&ai| ai < self.map.paths[pi].anchors.len());
        let ai = self
            .map
            .add_anchor(&self.idx, pi, fi, a.min(b), a.max(b), Author::Human, parent);
        self.sel_anchor = Some(ai);
        self.file.dirty = true;
        let under = parent.map_or_else(|| "the top level".into(), |p| self.step_label(pi, p));
        self.status = format!(
            "step {} added to '{}' under {under}",
            self.step_label(pi, ai),
            self.map.paths[pi].name
        );
    }

    fn step_label(&self, pi: usize, ai: usize) -> String {
        self.map
            .numbered(&self.idx, pi)
            .into_iter()
            .find(|(a, _, _)| *a == ai)
            .map_or_else(String::new, |(_, _, number)| number)
    }

    fn apply(&mut self, a: Action) {
        match a {
            Action::Focus(r) => self.go_to_symbol(r),
            Action::OpenPath(pi, tab) => {
                if self.sel_path != Some(pi) {
                    self.select_path(pi);
                }
                self.tab = tab;
            }
            Action::SelectStep(pi, ai, in_doc) => self.select_step(pi, ai, in_doc),
            Action::ToggleWhole(pi, ai) => self.steps.entry((pi, ai)).or_default().whole ^= true,
            Action::ToggleCode(pi, ai) => self.steps.entry((pi, ai)).or_default().hidden ^= true,
            Action::ToggleFold(pi, ai) => self.steps.entry((pi, ai)).or_default().folded ^= true,
            Action::ToggleLink(pi, ai) => self.steps.entry((pi, ai)).or_default().expanded ^= true,
            Action::CollapseAll(pi, hide) => {
                let n = self.map.paths[pi].anchors.len();
                for ai in 0..n {
                    self.steps.entry((pi, ai)).or_default();
                }
                for (&(p, ai), v) in &mut self.steps {
                    if p == pi {
                        v.hidden = hide && ai < n;
                        v.folded &= hide;
                    }
                }
            }
            Action::FoldAll(pi, fold) => {
                let n = self.map.paths[pi].anchors.len();
                let with_kids: Vec<bool> = (0..n)
                    .map(|ai| fold && self.map.descendants(pi, ai) > 0)
                    .collect();
                for ai in 0..n {
                    self.steps.entry((pi, ai)).or_default();
                }
                for (&(p, ai), v) in &mut self.steps {
                    if p == pi {
                        v.folded = with_kids.get(ai).copied().unwrap_or(false);
                    }
                }
            }
            Action::DeleteStep(pi, ai) => {
                let number = self
                    .map
                    .numbered(&self.idx, pi)
                    .iter()
                    .find(|(a, _, _)| *a == ai)
                    .map(|(_, _, n)| n.clone())
                    .unwrap_or_default();
                let a = &self.map.paths[pi].anchors[ai];
                self.status = format!(
                    "deleted step {number} {} ({}) from '{}'; unsaved",
                    a.symbol, a.file, self.map.paths[pi].name
                );
                self.map.remove_anchor(pi, ai);
                self.sel_anchor = None;
                self.step_removed(pi, ai);
                self.file.dirty = true;
            }
            Action::DeletePath(pi) => {
                let p = match self.map.remove_path(pi) {
                    Ok(p) => p,
                    Err(e) => {
                        self.status = e;
                        return;
                    }
                };
                self.status = format!(
                    "deleted path '{}' ({} steps); unsaved",
                    p.name,
                    p.anchors.len()
                );
                self.sel_path = None;
                self.sel_anchor = None;
                self.path_removed(pi);
                self.file.dirty = true;
            }
            Action::GoTo(fi, line) => self.open_line(fi, line),
            Action::Jump(fi, line, col) => self.probe_def(fi, line, col, Intent::Jump),
            Action::PeekAt(fi, line, col) => self.probe_def(fi, line, col, Intent::Peek),
            Action::Context(pi, ai, dir) => {
                let c = &mut self.steps.entry((pi, ai)).or_default().context;
                match dir {
                    0 => *c = (0, 0),
                    d if d < 0 => c.0 += CONTEXT_LINES,
                    _ => c.1 += CONTEXT_LINES,
                }
            }
            Action::SelectLine(li, extend) => {
                self.sel = match (extend, self.sel) {
                    (true, Some((a, _))) => Some((a, li)),
                    _ => Some((li, li)),
                };
            }
            Action::ClosePeek => self.peek = None,
            Action::ToggleDir(d) => {
                if !self.dir_toggled.remove(&d) {
                    self.dir_toggled.insert(d);
                }
            }
            Action::OpenGroup(g, open) => {
                self.groups_open.insert(g, open);
            }
            Action::Tab(t) => self.tab = t,
            Action::Back => self.back(),
            Action::Forward => self.forward(),
            Action::Save => self.save(),
            Action::PinSelection => self.add_selection(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Which {
    Search,
    NewPath,
    Cmd,
    SymFilter,
    GotoLine,
}

impl Which {
    fn name(self) -> &'static str {
        match self {
            Which::Search => "search",
            Which::NewPath => "new-path",
            Which::Cmd => "command",
            Which::SymFilter => "symbols",
            Which::GotoLine => "goto-line",
        }
    }
}

impl App {
    fn named_id(name: &str) -> Id {
        if let Some((base, n)) = name.split_once('/') {
            ui::id_n(ui::id(base), n.parse().unwrap_or(0))
        } else if let Some((base, s)) = name.split_once('@') {
            ui::id_with(ui::id(base), s)
        } else {
            ui::id(name)
        }
    }
}

impl window::App for App {
    fn script(&mut self, line: &str) -> bool {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w[0] {
            "idle" => return !self.working() && self.work.merge_wait.is_empty(),
            "rect" => {
                if let Some(name) = w.get(1) {
                    eprintln!(
                        "DUMP rect {name} = {:?}",
                        self.ui.interaction_of(Self::named_id(name)).rect
                    );
                }
            }
            "tab" => self.tab = Tab::from_name(w.get(1).copied().unwrap_or("")),
            "scroll" => {
                if let (Some(name), Some(n)) =
                    (w.get(1), w.get(2).and_then(|v| v.parse::<i32>().ok()))
                {
                    self.scrolls.insert(ui::id(name), n);
                }
            }
            "shot" => self.shot_next = w.get(1).map(PathBuf::from),
            "open" => {
                if let Some(fi) = w.get(1).and_then(|p| self.idx.find_file(p)) {
                    self.open_line(fi, w.get(2).and_then(|v| v.parse().ok()).unwrap_or(0));
                }
            }
            "dump" => {
                eprintln!(
                    "DUMP tab={:?} path={:?} step={:?} focus={:?} file={:?} sel={:?}",
                    self.tab,
                    self.sel_path,
                    self.sel_anchor,
                    self.focus.map(|r| self.idx.sym(r).name.clone()),
                    self.cur_file.map(|f| self.idx.files[f].path.clone()),
                    self.sel
                );
                for name in ["document", "listing", "paths", "output"] {
                    let id = ui::id(name);
                    if let Some((c, r)) = self.ui.content_of(id) {
                        eprintln!(
                            "DUMP scroll {name} off={} rect={:?} content={:?}",
                            self.scrolls.get(&id).copied().unwrap_or(0),
                            r,
                            c
                        );
                    }
                }
                eprintln!("DUMP dock {}", self.dock.describe());
                eprintln!(
                    "DUMP tip={:?} peek={:?} status={:?}",
                    self.tip_shown, self.peek, self.status
                );
                eprintln!(
                    "DUMP backend progress={:?} indexing={:?} unmerged={} reindexing={} linking={}",
                    self.work.progress,
                    self.work.indexing,
                    self.work.merge_wait.len(),
                    self.work.reindex_rx.is_some(),
                    self.work.link_rx.is_some()
                );
                eprintln!(
                    "DUMP graph zoom={:.3} pan={:?} canvas={:?} camera={}",
                    self.graph.zoom,
                    self.graph.pan,
                    self.ui.content_of(ui::id("graph-canvas")).map(|(_, r)| r),
                    self.graph.camera_state()
                );
                eprintln!(
                    "DUMP input mouse={:?} down={:?} hot_is_canvas={} active_is_canvas={} drag={:?}",
                    self.ui.input.mouse,
                    self.ui.input.down,
                    self.ui.hot() == Some(ui::id("graph-canvas")),
                    self.ui.active() == Some(ui::id("graph-canvas")),
                    self.graph.drag_state()
                );
                for (name, r) in self.graph.node_rects(&self.idx) {
                    eprintln!("DUMP node {name} rect={r:?}");
                }
                for (name, label, r) in self.graph.button_rects(&self.idx) {
                    eprintln!("DUMP button {name} '{label}' rect={r:?}");
                }
            }
            _ => eprintln!("script: unknown command '{line}'"),
        }
        true
    }

    fn locate(&mut self, name: &str) -> Option<(i32, i32)> {
        self.ui
            .interaction_of(Self::named_id(name))
            .rect
            .map(|r| (r.x + r.w / 2, r.y + r.h / 2))
    }

    fn frame(&mut self, gfx: &mut Gfx, input: &mut ui::Input) -> window::Frame {
        self.px = (14.0 * gfx.scale).round().max(8.0) as u32;
        self.cell = gfx.cell(self.px);
        let mut quit = false;
        if let Some((path, frame)) = self.shot.as_mut() {
            *frame += 1;
            if *frame == 6
                && let Some(n) = std::env::var("CODEMAP_SHOT_SCROLL")
                    .ok()
                    .and_then(|v| v.parse::<i32>().ok())
            {
                self.scrolls.insert(ui::id("document"), n);
            }
            if *frame == 3 {
                self.tab =
                    Tab::from_name(std::env::var("CODEMAP_SHOT_TAB").as_deref().unwrap_or(""));
            }
            if *frame == 20 {
                gfx.shot = Some(path.clone());
            }
            if *frame > 20 {
                quit = true;
            }
        }
        if let Some(p) = self.shot_next.take() {
            gfx.shot = Some(p);
        }
        self.poll_backend();
        self.poll_base();
        self.poll_reindex();
        self.poll_link();
        self.poll_disk();

        self.now = input.time;
        if input.key_with(Key::Char('s'), true, false) {
            self.save();
        }
        if input.key_with(Key::Left, false, true)
            || input.key_with(Key::Left, true, false)
            || input.back
        {
            self.back();
        }
        if input.key_with(Key::Right, false, true)
            || input.key_with(Key::Right, true, false)
            || input.forward
        {
            self.forward();
        }
        if let Some(line) = self.fields[Which::Cmd as usize].handle(input, false) {
            self.run_cmd(line);
        }
        if self.fields[Which::Search as usize]
            .handle(input, true)
            .is_some()
        {
            self.run_search();
        }
        if let Some(name) = self.fields[Which::NewPath as usize].handle(input, false) {
            self.create_path(name.trim().to_owned());
        }
        if !self.fields.iter().any(|f| f.focused) {
            if input.key_with(Key::Down, false, false) {
                self.step_by(1);
            }
            if input.key_with(Key::Up, false, false) {
                self.step_by(-1);
            }
        }
        self.fields[Which::SymFilter as usize].handle(input, true);
        if let Some(l) = self.fields[Which::GotoLine as usize].handle(input, true) {
            match (l.trim().parse::<usize>(), self.cur_file) {
                (Ok(line), Some(fi)) => {
                    let n = self.idx.files[fi].lines.len().max(1);
                    if line < 1 || line > n {
                        self.status = format!("line {line} is outside 1-{n}; went to the nearest");
                    }
                    let line = line.clamp(1, n) - 1;
                    self.sel = Some((line, line));
                    self.scroll_to = Some(line);
                }
                (Err(_), _) => self.status = format!("'{}' is not a line number", l.trim()),
                (_, None) => self.status = "no file open in the listing".into(),
            }
        }

        self.ui.begin(input);
        self.dock.cursor = Default::default();
        self.dock_input();
        self.ui
            .open(Kind::None, Layout::col().grow(), Style::bg(BG), None);
        self.top_bar();
        self.ui.open(
            Kind::None,
            Layout::col().grow(),
            Style::default(),
            Some(ui::id("body")),
        );
        self.ui.open(
            Kind::None,
            Layout::row().grow(),
            Style::default(),
            Some(ui::id("dock-row")),
        );
        let sizes = self.dock.sizes(self.ui.size, self.cell);
        self.docked(Edge::Left, &sizes);
        self.ui
            .open(Kind::None, Layout::col().grow(), Style::default(), None);
        match self.tab {
            Tab::Path => self.path_document(),
            Tab::Diff => self.diff_view(),
            Tab::Graph => self.graph_tab(gfx),
            Tab::Listing => self.listing(),
            Tab::Results => self.results_view(),
        }
        self.ui.close();
        self.docked(Edge::Right, &sizes);
        self.ui.close();
        self.docked(Edge::Bottom, &sizes);
        self.ui.close();
        self.status_bar();
        self.ui.close();
        self.drag_band();
        self.tooltip_element();
        self.ui.end(gfx);
        self.ui.draw(gfx, TEXT);

        for a in std::mem::take(&mut self.actions) {
            self.apply(a);
        }
        self.track_navigation();
        let busy = self.working() || self.shot.is_some();
        window::Frame {
            redraw_after: Duration::from_millis(if busy { 50 } else { 1000 }),
            quit,
            clear: BG,
            cursor: self.dock.cursor,
        }
    }
}
