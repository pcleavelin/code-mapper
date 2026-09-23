//! The human's surface, built from `ui` elements and drawn by `gfx`. One selection (a symbol,
//! with a step behind it when reached through a path) drives every view; the panels are
//! functions that open and close elements each frame, and every click becomes an `Action`
//! applied once the frame is built.

mod document;
mod graph;
mod nav;
mod panels;
mod peek;
mod views;
mod widgets;
mod work;

use crate::cli;
use crate::gfx::{self, Color, Gfx, Glyphs, Rect};
use crate::index::{self, Index, ServerFile, SymRef};
use crate::lsp;
use crate::map::{Author, Change, Kind as PathKind, Map, PathDiff, StepChange};
use crate::ui::{self, Align, Id, Interaction, Key, Kind, Layout, Measure, Style, Text, Ui, BORDER_BOTTOM, BORDER_LEFT, BORDER_RIGHT, BORDER_TOP};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant, SystemTime};
use self::{document::*, nav::*, panels::*, peek::*, widgets::*, work::*};

/// The tab of the centre panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Path,
    Diff,
    Graph,
    Listing,
    Results,
}

impl Tab {
    /// The tab a script or CODEMAP_SHOT_TAB names; any other name is the path document.
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

/// What a click asked for; applied after the frame is built.
pub enum Action {
    Focus(SymRef),
    OpenPath(usize, Tab), // select the path unless it is the one being read, and show it in the tab
    SelectStep(usize, usize, bool), // (path, step, clicked inside the document)
    ToggleWhole(usize, usize),      // show the whole symbol / just the slice in the document
    ToggleCode(usize, usize),       // hide / show a step's code
    ToggleFold(usize, usize),       // hide / show a step's subtree
    CollapseAll(usize, bool), // (path, hide): hide every step's code, or show all and unfold all
    FoldAll(usize, bool),     // (path, fold): fold every step with children, or unfold all
    DeleteStep(usize, usize),
    DeletePath(usize),
    GoTo(usize, usize),        // (file, line) in the listing
    Jump(usize, usize, usize),   // (file, line, column): to the definition of the identifier there
    PeekAt(usize, usize, usize), // the same, pinned in the peek panel
    SelectLine(usize, bool),     // (line, extend) in the listing
    ClosePeek,
    Context(usize, usize, i8), // (path, step): more lines above (-1), below (1), or back to the slice (0)
    ToggleDir(String),
    Tab(Tab),
    Back,
    Forward,
    Save,
    PinSelection,
}

pub struct App {
    pub idx: Index,
    servers: HashMap<&'static str, (Sender<Req>, Receiver<Msg>)>, // a live server thread per language, by server name
    no_server: HashSet<&'static str>,                             // servers that failed to start: not tried again
    indexing: HashSet<&'static str>,                              // servers with a batch of files in flight
    backend_progress: String,
    restart_backend: bool, // the index changed while a batch ran: send the next when it ends
    merge_wait: Vec<ServerFile>, // server answers held back until the next merge
    last_merge: Instant,         // when the last batch of answers went into the index
    link_rx: Option<Receiver<Index>>, // a link of the index running on a thread
    relink: bool,                     // the index changed while that link ran: link again when it lands
    hovers: HashMap<Probe, Option<Option<String>>>, // asked (None) or answered (Some: text or nothing)
    hover_want: Option<(Probe, f64)>,               // the position under the pointer and since when
    hover_inflight: bool,                           // a hover request the server has not answered
    now: f64,                                       // the frame's time, seconds since the window opened
    grids: HashMap<(usize, u64, usize, usize), Rc<Glyphs>>, // (file, text hash, first line, last line) -> its drawn form
    hscroll: HashMap<Id, i32>,                             // horizontal offset of each code block, in pixels
    output_bottom: u8,                                     // frames left in which the log is pinned to its end
    asked: usize,                                    // hover and definition requests not yet answered
    want_def: Option<(Probe, Intent)>,               // the definition lookup whose answer is awaited
    pub map: Map,
    base: Option<Map>,                       // the map at the parent revision, for the diff
    base_rx: Option<Receiver<Option<Map>>>, // jj is being asked for it
    map_path: PathBuf,
    map_mtime: Option<SystemTime>,
    dirty: bool,
    last_poll: Instant,
    warned_disk: bool,
    watch_tx: Sender<Vec<(String, Option<SystemTime>)>>, // the file list the watcher thread compares against
    watch_rx: Receiver<()>,                              // a source file changed
    reindex_rx: Option<Receiver<Index>>,                 // a rebuild is running on a thread

    // the selection
    pub focus: Option<SymRef>,     // the selected symbol
    pub sel_path: Option<usize>,   // the path being read: outline expanded, document shown
    pub sel_anchor: Option<usize>, // the selected step of it, when the selection came through the path
    pub cur_file: Option<usize>,
    pub sel: Option<(usize, usize)>,     // (anchor line, active line) of the line selection
    scroll_to: Option<usize>,            // the listing scrolls this line into view next frame
    scroll_to_step: Option<(usize, u8)>, // the document scrolls this step's header to the top; tries left
    top_step: Option<usize>,             // the step whose header is topmost in the document viewport
    outline_shown: Option<usize>,        // the top step the outline last scrolled to keep in view

    steps: HashMap<(usize, usize), StepView>, // (path, step) -> how the step shows, when not the default
    dir_toggled: HashSet<String>,            // directories in the Files tab whose default open state is flipped
    peek: Option<Peek>,                      // a definition pinned in the right panel
    history: Vec<Loc>,
    forward: Vec<Loc>,
    last_loc: Option<Loc>,

    pub ui: Ui,
    pub px: u32, // the UI font size in pixels
    pub cell: (i32, i32),
    pub tab: Tab,
    left: LeftTab,
    pub scrolls: HashMap<Id, i32>,
    pub actions: Vec<Action>,
    pub graph: graph::Graph,
    pub tooltip: Option<(Tip, (i32, i32))>, // what to show at the pointer this frame
    pub tip_shown: Option<String>,         // the first line of last frame's tooltip, for dumps

    fields: [Field; 5],           // the text fields, indexed by `Which`
    results: Vec<(usize, usize)>, // (file, line)
    output: String,
    status: String,
    shot: Option<(PathBuf, u32)>, // screenshot mode: write the window to this file after a few frames, then quit
    shot_next: Option<PathBuf>,   // a script asked for a screenshot of the next frame
}

impl App {
    pub fn new(root: &Path) -> App {
        let idx = index::build(root);
        let map_path = root.join(".codemap");
        let map = Map::load(&map_path);
        let unreadable = map.is_none() && map_path.exists();
        let mut map = map.unwrap_or_default();
        map.resolve_all(&idx);
        let first_path = if map.paths.is_empty() { None } else { Some(0) };
        let (watch_tx, list_rx) = channel();
        let (changed_tx, watch_rx) = channel();
        std::thread::spawn({
            let root = idx.root.clone();
            move || watch(root, list_rx, changed_tx)
        });
        let mut app = App {
            idx,
            servers: HashMap::new(),
            no_server: HashSet::new(),
            indexing: HashSet::new(),
            backend_progress: String::new(),
            restart_backend: false,
            merge_wait: Vec::new(),
            last_merge: Instant::now(),
            link_rx: None,
            relink: false,
            hovers: HashMap::new(),
            hover_want: None,
            hover_inflight: false,
            now: 0.0,
            grids: HashMap::new(),
            hscroll: HashMap::new(),
            output_bottom: 0,
            asked: 0,
            want_def: None,
            map,
            base: None,
            base_rx: None,
            map_mtime: mtime(&map_path),
            map_path,
            dirty: false,
            last_poll: Instant::now(),
            warned_disk: false,
            watch_tx,
            watch_rx,
            reindex_rx: None,
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
            peek: None,
            history: Vec::new(),
            forward: Vec::new(),
            last_loc: None,
            ui: Ui::default(),
            px: 14,
            cell: (8, 16),
            tab: Tab::Path,
            left: LeftTab::Paths,
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
        app.status = if unreadable { ".codemap is unreadable or an old format: starting from an empty map, saving overwrites it".into() } else { app.indexed_status() };
        app.watch_files();
        if let Some(pi) = first_path {
            app.select_path(pi);
        }
        app.start_backend();
        app.load_base();
        app
    }

    fn save(&mut self) {
        match self.map.save(&self.map_path) {
            Ok(()) => {
                self.dirty = false;
                self.warned_disk = false;
                self.map_mtime = mtime(&self.map_path);
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
        match cli::exec(&self.idx, &mut self.map, cmd, Author::Human, &mut self.output) {
            Ok(true) => {
                self.dirty = true;
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
                self.status = format!("bad regex: {}", e.to_string().lines().last().unwrap_or("").trim());
                return;
            }
        };
        // ponytail: single-threaded scan of the in-memory index; rayon it when it takes >100ms.
        self.results = self
            .idx
            .files
            .iter()
            .enumerate()
            .flat_map(|(fi, f)| f.lines.iter().enumerate().map(move |(li, line)| (fi, li, line)))
            .filter(|(_, _, line)| re.is_match(line))
            .map(|(fi, li, _)| (fi, li))
            .take(5000)
            .collect();
        self.status = format!("{} hits for /{}/", self.results.len(), self.fields[Which::Search as usize].text);
        self.tab = Tab::Results;
    }

    /// A new empty flow from the top bar, selected so 'pin selection' lands in it.
    fn create_path(&mut self, name: String) {
        if name.is_empty() {
            return;
        }
        self.sel_path = Some(self.map.add_path(&name, PathKind::Flow, Author::Human));
        self.sel_anchor = None;
        self.tab = Tab::Path;
        self.status = format!("path '{name}' created (unsaved)");
        self.dirty = true;
    }

    /// Pin the listing's selected lines as a step of the selected path, under the selected
    /// step (else a root). The new step becomes the selected one so repeated pins build a chain.
    fn add_selection(&mut self) {
        let (Tab::Listing, Some(fi), Some((a, b))) = (self.tab, self.cur_file, self.sel) else {
            self.status = "select lines in the listing first".into();
            return;
        };
        let Some(pi) = self.sel_path else {
            self.status = "select a path first".into();
            return;
        };
        let parent = self.sel_anchor.filter(|&ai| ai < self.map.paths[pi].anchors.len());
        let ai = self.map.add_anchor(&self.idx, pi, fi, a.min(b), a.max(b), Author::Human, parent);
        self.sel_anchor = Some(ai);
        self.dirty = true;
        self.status = format!("step [{ai}] added to '{}' under [{}]", self.map.paths[pi].name, cli::step_number(parent));
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
                let with_kids: Vec<bool> = (0..n).map(|ai| fold && self.map.descendants(pi, ai) > 0).collect();
                for ai in 0..n {
                    self.steps.entry((pi, ai)).or_default();
                }
                for (&(p, ai), v) in &mut self.steps {
                    if p == pi {
                        v.folded = with_kids.get(ai).copied().unwrap_or(false);
                    }
                }
            }
            // ponytail: no undo
            Action::DeleteStep(pi, ai) => {
                let number = self.map.numbered(&self.idx, pi).iter().find(|(a, _, _)| *a == ai).map(|(_, _, n)| n.clone()).unwrap_or_default();
                let a = &self.map.paths[pi].anchors[ai];
                self.status = format!("deleted step {number} {} ({}) from '{}'; unsaved", a.symbol, a.file, self.map.paths[pi].name);
                self.map.remove_anchor(pi, ai);
                self.sel_anchor = None;
                self.step_removed(pi, ai);
                self.dirty = true;
            }
            Action::DeletePath(pi) => {
                let p = self.map.paths.remove(pi);
                self.status = format!("deleted path '{}' ({} steps); unsaved", p.name, p.anchors.len());
                self.sel_path = None;
                self.sel_anchor = None;
                self.path_removed(pi);
                self.dirty = true;
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
            Action::Tab(t) => self.tab = t,
            Action::Back => self.back(),
            Action::Forward => self.forward(),
            Action::Save => self.save(),
            Action::PinSelection => self.add_selection(),
        }
    }
}

/// A text field, as its index in `App::fields`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Which {
    Search,
    NewPath,
    Cmd,
    SymFilter,
    GotoLine,
}

impl App {
    /// Element names for `rect`, `click-id` and `hover-id`: `name`, `name/<n>` (an id_n) or
    /// `name@<suffix>` (an id_with).
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

impl gfx::App for App {
    /// Script commands: `tab <path|graph|listing|diff|results>`, `open <file> [line]`, `scroll <panel> <n>`,
    /// `idle` (waits until no server request, merge, re-index or link is in flight), `shot <file.png>`, `rect <id> [n]`
    /// (last frame's rectangle of an element by its id name), `dump` (state to stderr:
    /// selection, tab, scrolls, tooltip, peek, graph camera, node and button rectangles, the
    /// canvas rectangle).
    fn script(&mut self, line: &str) -> bool {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w[0] {
            "idle" => return !self.working() && self.merge_wait.is_empty(),
            "rect" => {
                if let Some(name) = w.get(1) {
                    eprintln!("DUMP rect {name} = {:?}", self.ui.interaction_of(Self::named_id(name)).rect);
                }
            }
            "tab" => self.tab = Tab::from_name(w.get(1).copied().unwrap_or("")),
            "scroll" => {
                if let (Some(name), Some(n)) = (w.get(1), w.get(2).and_then(|v| v.parse::<i32>().ok())) {
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
                eprintln!("DUMP tab={:?} path={:?} step={:?} focus={:?} file={:?} sel={:?}", self.tab, self.sel_path, self.sel_anchor, self.focus.map(|r| self.idx.sym(r).name.clone()), self.cur_file.map(|f| self.idx.files[f].path.clone()), self.sel);
                for name in ["document", "listing", "paths", "output"] {
                    let id = ui::id(name);
                    if let Some((c, r)) = self.ui.content_of(id) {
                        eprintln!("DUMP scroll {name} off={} rect={:?} content={:?}", self.scrolls.get(&id).copied().unwrap_or(0), r, c);
                    }
                }
                eprintln!("DUMP tip={:?} peek={:?} status={:?}", self.tip_shown, self.peek, self.status);
                eprintln!("DUMP backend progress={:?} indexing={:?} unmerged={} reindexing={} linking={}", self.backend_progress, self.indexing, self.merge_wait.len(), self.reindex_rx.is_some(), self.link_rx.is_some());
                eprintln!("DUMP graph zoom={:.3} pan={:?} canvas={:?} camera={}", self.graph.zoom, self.graph.pan, self.ui.content_of(ui::id("graph-canvas")).map(|(_, r)| r), self.graph.camera_state());
                eprintln!("DUMP input mouse={:?} down={:?} hot_is_canvas={} active_is_canvas={} drag={:?}", self.ui.input.mouse, self.ui.input.down, self.ui.hot() == Some(ui::id("graph-canvas")), self.ui.active() == Some(ui::id("graph-canvas")), self.graph.drag_state());
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
        self.ui.interaction_of(Self::named_id(name)).rect.map(|r| (r.x + r.w / 2, r.y + r.h / 2))
    }

    fn frame(&mut self, gfx: &mut Gfx, input: &mut ui::Input) -> gfx::Frame {
        self.px = (14.0 * gfx.scale).round().max(8.0) as u32;
        self.cell = gfx.cell(self.px);
        let mut quit = false;
        // Development aid: with CODEMAP_SHOT=<file.png> the window is written there once the
        // first frames have settled, and the app quits. CODEMAP_SHOT_TAB picks the centre tab.
        if let Some((path, frame)) = self.shot.as_mut() {
            *frame += 1;
            if *frame == 6 {
                if let Some(n) = std::env::var("CODEMAP_SHOT_SCROLL").ok().and_then(|v| v.parse::<i32>().ok()) {
                    self.scrolls.insert(ui::id("document"), n);
                }
            }
            if *frame == 3 {
                self.tab = Tab::from_name(std::env::var("CODEMAP_SHOT_TAB").as_deref().unwrap_or(""));
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

        // keys
        self.now = input.time;
        if input.key_with(Key::Char('s'), true, false) {
            self.save();
        }
        if input.key_with(Key::Left, false, true) || input.key_with(Key::Left, true, false) || input.back {
            self.back();
        }
        if input.key_with(Key::Right, false, true) || input.key_with(Key::Right, true, false) || input.forward {
            self.forward();
        }
        if let Some(line) = self.fields[Which::Cmd as usize].handle(input, false) {
            self.run_cmd(line);
        }
        if self.fields[Which::Search as usize].handle(input, true).is_some() {
            self.run_search();
        }
        if let Some(name) = self.fields[Which::NewPath as usize].handle(input, false) {
            self.create_path(name.trim().to_owned());
        }
        // with no field focused, up and down walk the path
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

        // the frame
        self.ui.begin(input);
        self.ui.open(Kind::None, Layout::col().grow(), Style::bg(BG), None);
        self.top_bar();
        self.ui.open(Kind::None, Layout::row().grow(), Style::default(), None);
        let (lw, rw) = ((44 * self.cell.0).min(self.ui.size.0 / 4), (40 * self.cell.0).min(self.ui.size.0 / 4));
        self.left_panel(lw);
        self.ui.open(Kind::None, Layout::col().grow(), Style::default(), None);
        match self.tab {
            Tab::Path => self.path_document(),
            Tab::Diff => self.diff_view(),
            Tab::Graph => self.graph_tab(gfx),
            Tab::Listing => self.listing(),
            Tab::Results => self.results_view(),
        }
        self.ui.close();
        self.xrefs_panel(rw);
        self.ui.close();
        let out_h = (self.ui.size.1 / 6).max(6 * self.cell.1);
        self.output_panel(out_h);
        self.status_bar();
        self.ui.close();
        self.tooltip_element();
        self.ui.end(gfx);
        self.ui.draw(gfx, TEXT);

        for a in std::mem::take(&mut self.actions) {
            self.apply(a);
        }
        self.track_navigation();
        let busy = self.working() || self.shot.is_some();
        gfx::Frame { redraw_after: Duration::from_millis(if busy { 50 } else { 1000 }), quit, clear: BG }
    }
}
