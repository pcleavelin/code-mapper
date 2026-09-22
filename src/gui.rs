//! The human's surface, built from `ui` elements and drawn by `gfx`. One selection (a symbol,
//! with a step behind it when reached through a path) drives every view; the panels are
//! functions that open and close elements each frame, and every click becomes an `Action`
//! applied once the frame is built.

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

// ---- theme ----

pub const BG: Color = [24, 24, 26, 255];
pub const PANEL: Color = [32, 32, 35, 255];
pub const FIELD: Color = [18, 18, 20, 255];
pub const BORDER: Color = [60, 60, 66, 255];
pub const TEXT: Color = [220, 220, 220, 255];
pub const WEAK: Color = [140, 140, 148, 255];
pub const ACCENT: Color = [90, 150, 240, 255];
pub const GREEN: Color = [90, 200, 120, 255];
pub const RED: Color = [240, 110, 110, 255];
pub const ORANGE: Color = [220, 160, 80, 255];
pub const HOVER: Color = [50, 50, 56, 255];
pub const SELECTED: Color = [45, 65, 100, 255];

pub fn dim(c: Color, a: u8) -> Color {
    [c[0], c[1], c[2], a]
}

/// Syntax classes from the index, as colours.
pub fn hl_color(class: u8) -> Color {
    match class {
        index::HL_KEYWORD => [197, 134, 192, 255],
        index::HL_STRING => [206, 145, 120, 255],
        index::HL_COMMENT => [106, 153, 85, 255],
        index::HL_FUNCTION => [220, 220, 170, 255],
        index::HL_TYPE => [78, 201, 176, 255],
        index::HL_CONSTANT => [181, 206, 168, 255],
        index::HL_PROPERTY => [156, 220, 254, 255],
        _ => TEXT,
    }
}

// ---- element builders ----

fn text(s: &str, px: u32, color: Color) -> Kind {
    Kind::Text(Text { runs: vec![(s.to_owned(), color)], px, wrap: false })
}

fn wrapped(s: &str, px: u32, color: Color) -> Kind {
    Kind::Text(Text { runs: vec![(s.to_owned(), color)], px, wrap: true })
}

fn runs(runs: Vec<(String, Color)>, px: u32) -> Kind {
    Kind::Text(Text { runs, px, wrap: false })
}

/// Columns of line number and a space in front of every code line.
pub const GUTTER: usize = 6;

/// Lines `lo..=hi` of a file as a grid: the line number in the gutter, then the line with
/// its syntax colours, one cell per character.
pub fn build_grid(f: &index::File, lo: usize, hi: usize) -> Glyphs {
    let w = (lo..=hi).map(|li| f.lines[li].chars().count()).max().unwrap_or(0) + GUTTER;
    let mut g = Glyphs::new(w, hi + 1 - lo);
    for (row, li) in (lo..=hi).enumerate() {
        for (x, c) in format!("{:5} ", li + 1).chars().enumerate() {
            g.set(x, row, c, WEAK);
        }
        let spans = f.hl.get(li).map(Vec::as_slice).unwrap_or(&[]);
        let mut si = 0;
        for (x, (b, c)) in f.lines[li].char_indices().enumerate() {
            while si < spans.len() && spans[si].1 as usize <= b {
                si += 1;
            }
            let color = match spans.get(si) {
                Some(&(s, _, class)) if s as usize <= b => hl_color(class),
                _ => TEXT,
            };
            g.set(GUTTER + x, row, c, color);
        }
    }
    g
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_owned() } else { s.chars().take(n.saturating_sub(1)).chain(std::iter::once('…')).collect() }
}

/// Cuts from the front, keeping the tail: what a path needs, since the file name is at the end.
fn trunc_left(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n { s.to_owned() } else { std::iter::once('…').chain(s.chars().skip(count + 1 - n.max(1))).collect() }
}

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LeftTab {
    Paths,
    Symbols,
    Files,
}

/// A single-line text field.
#[derive(Default)]
pub struct Field {
    pub text: String,
    cursor: usize, // in chars
    pub focused: bool,
    selected: bool, // the whole text is selected: the next character typed replaces it
    history: Vec<String>,
    hist_at: Option<usize>,
}

impl Field {
    /// Feed the frame's keys; returns the line when enter was pressed.
    fn handle(&mut self, input: &ui::Input, keep_on_enter: bool) -> Option<String> {
        if !self.focused {
            return None;
        }
        let mut submitted = None;
        for (k, m) in &input.keys {
            match k {
                Key::Enter => {
                    let line = if keep_on_enter {
                        self.selected = !self.text.is_empty(); // the next entry replaces this one
                        self.text.clone()
                    } else {
                        self.cursor = 0;
                        std::mem::take(&mut self.text)
                    };
                    self.hist_at = None;
                    if !line.trim().is_empty() {
                        self.history.push(line.clone());
                        submitted = Some(line);
                    }
                }
                Key::Backspace | Key::Delete if self.selected => {
                    self.text.clear();
                    self.cursor = 0;
                    self.selected = false;
                }
                Key::Backspace => {
                    if self.cursor > 0 {
                        let i = self.byte_at(self.cursor - 1);
                        self.text.remove(i);
                        self.cursor -= 1;
                    }
                }
                Key::Delete => {
                    if self.cursor < self.text.chars().count() {
                        let i = self.byte_at(self.cursor);
                        self.text.remove(i);
                    }
                }
                Key::Left | Key::Right if m.ctrl || m.alt => {} // those chords belong to the app
                Key::Left => {
                    self.cursor = self.cursor.saturating_sub(1);
                    self.selected = false;
                }
                Key::Right => {
                    self.cursor = (self.cursor + 1).min(self.text.chars().count());
                    self.selected = false;
                }
                Key::Home => {
                    self.cursor = 0;
                    self.selected = false;
                }
                Key::End => {
                    self.cursor = self.text.chars().count();
                    self.selected = false;
                }
                Key::Char('a') if m.ctrl => self.selected = !self.text.is_empty(),
                Key::Up | Key::Down => {
                    if !self.history.is_empty() {
                        let at = match (self.hist_at, k) {
                            (None, Key::Up) => self.history.len() - 1,
                            (Some(a), Key::Up) => a.saturating_sub(1),
                            (Some(a), Key::Down) if a + 1 < self.history.len() => a + 1,
                            _ => {
                                self.hist_at = None;
                                self.text.clear();
                                self.cursor = 0;
                                continue;
                            }
                        };
                        self.hist_at = Some(at);
                        self.text = self.history[at].clone();
                        self.cursor = self.text.chars().count();
                    }
                }
                Key::Escape => self.focused = false,
                Key::Char('u') if m.ctrl => {
                    self.text.clear();
                    self.cursor = 0;
                }
                _ => {}
            }
        }
        if !input.text.is_empty() {
            if std::mem::take(&mut self.selected) {
                self.text.clear();
                self.cursor = 0;
            }
            let i = self.byte_at(self.cursor);
            self.text.insert_str(i, &input.text);
            self.cursor += input.text.chars().count();
        }
        submitted
    }

    fn byte_at(&self, ch: usize) -> usize {
        self.text.char_indices().nth(ch).map(|(i, _)| i).unwrap_or(self.text.len())
    }

    /// The text with the caret, as runs.
    fn runs(&self, hint: &str) -> Vec<(String, Color)> {
        if self.text.is_empty() && !self.focused {
            return vec![(hint.to_owned(), WEAK)];
        }
        let i = self.byte_at(self.cursor);
        let color = if self.selected && self.focused { ACCENT } else { TEXT };
        vec![(self.text[..i].to_owned(), color), (if self.focused { "▏" } else { "" }.to_owned(), ACCENT), (self.text[i..].to_owned(), color)]
    }
}

/// A request to a language's server thread.
enum Req {
    Index(Vec<(String, u64)>),
    Hover(Probe),
    Def(Probe),
}

/// A position asked about: the file's path and text hash, so an answer for text that has
/// since changed is never used, and the 0-based line and UTF-16 column of an identifier.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct Probe {
    path: String,
    hash: u64,
    line: u32,
    col: u32,
}

/// A definition the server found: the file relative to the root when it is inside, the
/// absolute path either way, the 0-based line, and the lines around it drawn from the file
/// on disk (starting at line `first`) for a peek when the file is not indexed.
struct Def {
    rel: Option<String>,
    abs: PathBuf,
    line: usize,
    first: usize,
    grid: Glyphs,
}

/// What the language-server thread sends back.
enum Msg {
    File(ServerFile),
    Progress(String),
    Failed(&'static index::Lang, String),
    Done(&'static index::Lang),
    Hover(Probe, Option<String>),
    Def(Probe, Option<Def>),
}

/// Why a definition was asked for.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Intent {
    Jump,
    Peek,
}

/// What the tooltip under the pointer shows.
pub enum Tip {
    Sym(SymRef),
    Text(String),
}

/// What the peek panel shows, and what a definition lookup landed on.
#[derive(Clone)]
pub enum Peek {
    Sym(SymRef),
    Line(usize, usize), // (file, line): a definition inside a symbol, or in a file with none
    Outside(PathBuf, usize, usize, Rc<Glyphs>), // path, line, first line of the grid, the grid
}

impl std::fmt::Debug for Peek {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Peek::Sym(r) => write!(f, "Sym({r:?})"),
            Peek::Line(fi, li) => write!(f, "Line({fi}, {li})"),
            Peek::Outside(p, li, first, g) => write!(f, "Outside({}, {li}, {first}, {} rows)", p.display(), g.h),
        }
    }
}

/// Lines one press of a context button adds above or below a step's code.
pub const CONTEXT_LINES: usize = 10;

/// A language's server, alive on its own thread for the life of the window. It indexes the
/// files it is sent a batch at a time and answers hover and definition requests between
/// files, so the pointer never waits behind a batch.
fn serve(root: PathBuf, lang: &'static index::Lang, rx: Receiver<Req>, tx: Sender<Msg>) {
    let (mut c, abs) = match index::start_server(&root, lang) {
        Ok(x) => x,
        Err(e) => {
            let _ = tx.send(Msg::Failed(lang, e));
            return;
        }
    };
    let mut queue: VecDeque<(String, u64)> = VecDeque::new();
    let (mut n, mut done) = (0, 0);
    loop {
        let req = match rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) if queue.is_empty() => match rx.recv() {
                Ok(r) => Some(r),
                Err(_) => return,
            },
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => return,
        };
        match req {
            Some(Req::Index(files)) => {
                if queue.is_empty() {
                    (n, done) = (0, 0);
                }
                n += files.len();
                queue.extend(files);
            }
            Some(Req::Hover(p)) => {
                let t = c.hover(&abs.join(&p.path), p.line, p.col);
                let _ = tx.send(Msg::Hover(p, t));
            }
            Some(Req::Def(p)) => {
                let d = c.definition(&abs.join(&p.path), p.line, p.col).map(|(path, line, _)| {
                    let line = line as usize;
                    let text = std::fs::read_to_string(&path).unwrap_or_default().replace('\t', "    ");
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_owned();
                    let file = index::parse_file(&mut index::Parsers::default(), path.to_string_lossy().replace('\\', "/"), &text, &ext);
                    let first = line.saturating_sub(6);
                    let grid = if file.lines.is_empty() { Glyphs::new(0, 0) } else { build_grid(&file, first, (line + 24).min(file.lines.len() - 1)) };
                    Def { rel: lsp::relative(&path, &abs), abs: path, line, first, grid }
                });
                let _ = tx.send(Msg::Def(p, d));
            }
            None => {
                let Some((path, hash)) = queue.pop_front() else { continue };
                let f = index::index_file(&mut c, &abs, &path, hash);
                done += 1;
                let _ = tx.send(Msg::Progress(format!("{}: {done}/{n}", lang.server)));
                let _ = tx.send(Msg::File(f));
                if queue.is_empty() {
                    let _ = tx.send(Msg::Done(lang));
                }
            }
        }
    }
}

/// The UTF-16 column of char index `col` in `line`, which is how servers count.
fn utf16_col(line: &str, col: usize) -> u32 {
    line.chars().take(col).map(|c| c.len_utf16() as u32).sum()
}

/// A place in the centre panel, kept so the reader can go back and forward.
#[derive(Clone, PartialEq)]
struct Loc {
    tab: Tab,
    file: Option<String>,
    sel: Option<(usize, usize)>,
    focus: Option<(String, String)>,
    path: Option<usize>,
    step: Option<usize>,
}

impl Loc {
    /// Same place for history purposes: a changed line selection alone is not a navigation.
    fn same_place(&self, o: &Loc) -> bool {
        self.tab == o.tab && self.file == o.file && self.focus == o.focus && self.path == o.path && self.step == o.step
    }
}

/// What a click asked for; applied after the frame is built.
pub enum Action {
    Focus(SymRef),
    OpenPath(usize, Tab), // select the path unless it is the one being read, and show it in the tab
    SelectStep(usize, usize, bool), // (path, step, clicked inside the document)
    ToggleStep(usize, usize),       // show the whole symbol / just the slice in the document
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

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok().and_then(|m| m.modified().ok())
}

/// The indexed files' modification times, compared against the disk once a second off the main
/// thread. Each list the main thread sends is watched until a file differs from it, which is
/// reported once; the thread then waits for the refreshed list the main thread sends after the
/// re-index, so it never reports the same edit twice.
fn watch(root: PathBuf, rx: Receiver<Vec<(String, Option<SystemTime>)>>, tx: Sender<()>) {
    while let Ok(list) = rx.recv() {
        while list.iter().all(|(p, mt)| mtime(&root.join(p)) == *mt) {
            std::thread::sleep(Duration::from_secs(1));
        }
        if tx.send(()).is_err() {
            return;
        }
    }
}

/// Run `f` on its own thread; its result arrives on the returned channel.
fn bg<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx
}

/// A thread's result: None while it runs, Some(None) when it died without one, Some(Some(value))
/// when it answered. The receiver is dropped once either lands.
fn landed<T>(rx: &mut Option<Receiver<T>>) -> Option<Option<T>> {
    let got = match rx.as_ref()?.try_recv() {
        Err(TryRecvError::Empty) => return None,
        r => r.ok(),
    };
    *rx = None;
    Some(got)
}

/// Add `k` to `set`, or remove it when it is there.
fn toggle<T: Eq + std::hash::Hash>(set: &mut HashSet<T>, k: T) {
    if !set.remove(&k) {
        set.insert(k);
    }
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

    expanded_steps: HashSet<(usize, usize)>, // (path, step) showing the whole enclosing symbol
    context: HashMap<(usize, usize), (usize, usize)>, // (path, step) -> extra lines shown above and below
    collapsed: HashSet<(usize, usize)>,      // (path, step) with its code hidden
    folded: HashSet<(usize, usize)>,         // (path, step) with its subtree hidden
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
    pub graph: crate::graph::Graph,
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
            expanded_steps: HashSet::new(),
            context: HashMap::new(),
            collapsed: HashSet::new(),
            folded: HashSet::new(),
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
            graph: crate::graph::Graph::new(),
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

    // ---- background work ----

    /// Ask jj for the parent revision's map on a thread; `poll_base` picks it up.
    fn load_base(&mut self) {
        let root = self.idx.root.clone();
        self.base_rx = Some(bg(move || Map::base_from_vcs(&root)));
    }

    fn poll_base(&mut self) {
        if let Some(base) = self.base_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.base = base;
            self.base_rx = None;
        }
    }

    /// The working map against the parent revision's, when that is known.
    fn diffs(&self) -> Vec<PathDiff> {
        self.base.as_ref().map(|b| self.map.diff(b)).unwrap_or_default()
    }

    /// The live server for a language, started on first use. None once it has failed to start.
    fn server(&mut self, lang: &'static index::Lang) -> Option<&Sender<Req>> {
        if self.no_server.contains(lang.server) {
            return None;
        }
        if !self.servers.contains_key(lang.server) {
            let (rtx, rrx) = channel();
            let (mtx, mrx) = channel();
            let root = self.idx.root.clone();
            std::thread::spawn(move || serve(root, lang, rrx, mtx));
            self.servers.insert(lang.server, (rtx, mrx));
            self.status = format!("{}: starting", lang.server);
        }
        self.servers.get(lang.server).map(|(tx, _)| tx)
    }

    /// Send every pending language's files to its server; answers arrive through
    /// `poll_backend`. A language whose server cannot start keeps the tree-sitter answer.
    fn start_backend(&mut self) {
        if !self.indexing.is_empty() {
            self.restart_backend = true;
            return;
        }
        for (lang, files) in self.idx.pending() {
            let n = files.len();
            match self.server(lang) {
                Some(tx) => {
                    let _ = tx.send(Req::Index(files));
                    self.indexing.insert(lang.server);
                    self.backend_progress = format!("{}: 0/{n}", lang.server);
                }
                None => self.idx.give_up(lang),
            }
        }
    }

    /// Merge whatever the server threads have answered since the last frame.
    fn poll_backend(&mut self) {
        let mut msgs = Vec::new();
        for (_, rx) in self.servers.values() {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        let answered = !msgs.is_empty(); // any message at all means a server is past "starting"
        let mut files = Vec::new();
        let mut batch_ended = false;
        for m in msgs {
            match m {
                Msg::File(f) => files.push(f),
                Msg::Progress(p) => self.backend_progress = p,
                Msg::Failed(lang, e) => {
                    self.servers.remove(lang.server);
                    self.no_server.insert(lang.server);
                    self.indexing.remove(lang.server);
                    self.idx.give_up(lang);
                    self.hovers.clear();
                    self.asked = 0;
                    self.want_def = None;
                    self.status = format!("{e}: its files keep the tree-sitter resolver, no hover or go-to for them");
                    batch_ended = true;
                }
                Msg::Done(lang) => {
                    self.indexing.remove(lang.server);
                    batch_ended = true;
                }
                Msg::Hover(p, t) => {
                    self.asked = self.asked.saturating_sub(1);
                    self.hover_inflight = false;
                    self.hovers.insert(p, Some(t));
                }
                Msg::Def(p, d) => {
                    self.asked = self.asked.saturating_sub(1);
                    if self.want_def.as_ref().is_some_and(|(w, _)| *w == p) {
                        let (_, intent) = self.want_def.take().unwrap();
                        match d {
                            Some(d) => {
                                let found = match d.rel.as_deref().and_then(|r| self.idx.find_file(r)) {
                                    Some(fi) => match self.idx.by_line(&self.idx.files[fi].path, d.line).filter(|r| self.idx.sym(*r).start == d.line) {
                                        Some(r) => Peek::Sym(r),
                                        None => Peek::Line(fi, d.line),
                                    },
                                    None => Peek::Outside(d.abs, d.line, d.first, Rc::new(d.grid)),
                                };
                                self.land(found, intent);
                            }
                            None => self.status = "no definition found".into(),
                        }
                    }
                }
            }
        }
        // Every answer costs a map re-resolve and every drawn grid, so they are merged at most
        // once a second and once more when the batch ends; the re-link runs on a thread.
        self.merge_wait.append(&mut files);
        if !self.merge_wait.is_empty() && (batch_ended || self.last_merge.elapsed() >= Duration::from_secs(1)) {
            let files = std::mem::take(&mut self.merge_wait);
            self.with_index_change(|app| {
                for f in files {
                    app.idx.apply(f);
                }
            });
            self.start_link();
            self.last_merge = Instant::now();
            if self.status.ends_with(" symbols indexed") {
                self.status = self.indexed_status();
            }
        }
        if answered && self.status.ends_with(": starting") {
            self.status = self.indexed_status();
        }
        if batch_ended && self.indexing.is_empty() {
            self.idx.save_cache();
            self.backend_progress.clear();
            if std::mem::take(&mut self.restart_backend) {
                self.start_backend();
            }
        }
    }

    /// Once a second: pick up a map written by the CLI. Source changes are found by the watcher
    /// thread, so nothing here stats the tree; one it reports starts a rebuild of the index from
    /// disk and the cache on a thread, and the index in use is untouched until `poll_reindex`
    /// swaps the new one in.
    fn poll_disk(&mut self) {
        if self.watch_rx.try_recv().is_ok() {
            let root = self.idx.root.clone();
            self.reindex_rx = Some(bg(move || index::build(&root)));
            self.status = "re-indexing (source changed)".into();
        }
        if self.last_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_poll = Instant::now();
        let mt = mtime(&self.map_path);
        if mt != self.map_mtime {
            if self.dirty {
                if !self.warned_disk {
                    self.status = "map changed on disk while you have unsaved changes: save overwrites it, or use the command line to reload".into();
                    self.warned_disk = true;
                }
            } else {
                self.reload_map();
                self.map_mtime = mt;
                self.load_base();
                self.status = "map reloaded (changed on disk)".into();
            }
        }
    }

    /// Read `.codemap` again and keep as much of the reading position as the new map still
    /// supports: the path by name, the step when the step at that index is still the same lines
    /// of the same symbol, and a path's fold, collapse and context sets when its step count is
    /// unchanged. Paths are matched by name, so adding one does not shift another's state.
    fn reload_map(&mut self) {
        let was: Vec<(String, usize)> = self.map.paths.iter().map(|p| (p.name.clone(), p.anchors.len())).collect();
        let path_name = self.sel_path.map(|pi| self.map.paths[pi].name.clone());
        let step = self.sel_anchor.zip(self.sel_path).map(|(ai, pi)| {
            let a = &self.map.paths[pi].anchors[ai];
            (ai, a.file.clone(), a.symbol.clone(), a.off_start, a.off_end)
        });
        self.map = Map::load(&self.map_path).unwrap_or_default();
        self.map.resolve_all(&self.idx);
        self.sel_path = path_name.and_then(|name| self.map.paths.iter().position(|p| p.name == name));
        self.sel_anchor = match (self.sel_path, step) {
            (Some(pi), Some((ai, file, symbol, os, oe))) => self.map.paths[pi]
                .anchors
                .get(ai)
                .filter(|a| a.file == file && a.symbol == symbol && (a.off_start, a.off_end) == (os, oe))
                .map(|_| ai),
            _ => None,
        };
        // old path index -> new one, for the paths whose steps cannot have moved
        let kept: HashMap<usize, usize> = was
            .iter()
            .enumerate()
            .filter_map(|(old, (name, n))| self.map.paths.iter().position(|p| p.name == *name && p.anchors.len() == *n).map(|new| (old, new)))
            .collect();
        self.remap_steps(|(pi, ai)| kept.get(&pi).map(|&p| (p, ai)));
    }

    /// Carry the whole-symbol, code, fold and context records over to new (path, step) keys:
    /// `f` gives a record's new key, or None when the record goes.
    fn remap_steps(&mut self, f: impl Fn((usize, usize)) -> Option<(usize, usize)>) {
        for set in [&mut self.expanded_steps, &mut self.collapsed, &mut self.folded] {
            *set = set.drain().filter_map(&f).collect();
        }
        self.context = self.context.drain().filter_map(|(k, v)| f(k).map(|k| (k, v))).collect();
    }

    /// Run `change` on the index and carry the selection, the listing and the map's anchors
    /// over by (file, symbol) identity, since symbol indices do not survive it.
    fn with_index_change(&mut self, change: impl FnOnce(&mut App)) {
        let focus = self.focus.map(|r| self.idx.key(r));
        let cur_file = self.cur_file.map(|fi| self.idx.files[fi].path.clone());
        let peek_sym = match self.peek {
            Some(Peek::Sym(r)) => Some(self.idx.key(r)),
            _ => None,
        };
        let peek_line = match &self.peek {
            Some(Peek::Line(fi, li)) => Some((self.idx.files[*fi].path.clone(), *li)),
            _ => None,
        };
        let graph = self.graph.save(&self.idx);
        change(self);
        self.graph.restore(&self.idx, graph);
        if let Some(k) = peek_sym {
            self.peek = self.idx.by_key(&k).map(Peek::Sym);
        } else if let Some((p, li)) = peek_line {
            self.peek = self.idx.find_file(&p).map(|fi| Peek::Line(fi, li));
        }
        self.hovers.clear();
        self.grids.clear();
        self.map.resolve_all(&self.idx);
        self.focus = focus.and_then(|k| self.idx.by_key(&k));
        self.cur_file = cur_file.and_then(|p| self.idx.find_file(&p));
    }

    /// Give the watcher thread the indexed files' modification times so it starts looking.
    fn watch_files(&self) {
        let _ = self.watch_tx.send(self.idx.files.iter().map(|f| (f.path.clone(), f.mtime)).collect());
    }

    /// Take the rebuilt index, then ask the servers about what changed and give the watcher
    /// thread the new modification times so it starts looking again.
    fn poll_reindex(&mut self) {
        let Some(idx) = landed(&mut self.reindex_rx) else { return };
        match idx {
            Some(idx) => {
                self.with_index_change(|app| app.idx = idx);
                self.results.clear();
                self.status = format!("re-indexed: {} (source changed)", self.indexed_status());
            }
            None => self.status = "re-index failed; the index in use is the old one".into(), // the rebuild thread panicked
        }
        self.watch_files();
        self.start_backend();
    }

    /// Link the index on a thread over a text-free copy, since a link is a pass over every call
    /// site in the repo. One runs at a time; a change meanwhile queues another.
    fn start_link(&mut self) {
        if self.link_rx.is_some() {
            self.relink = true;
            return;
        }
        let mut snap = self.idx.symbols_only();
        self.link_rx = Some(bg(move || {
            snap.link();
            snap
        }));
    }

    /// Take a finished link. A copy whose symbol tables no longer match the index is dropped:
    /// the change that made it stale queued the link that replaces it.
    fn poll_link(&mut self) {
        let Some(done) = landed(&mut self.link_rx) else { return };
        if let Some(linked) = done {
            self.idx.take_edges(&linked);
        }
        if std::mem::take(&mut self.relink) {
            self.start_link();
        }
    }

    fn indexed_status(&self) -> String {
        format!("{} files, {} symbols indexed", self.idx.files.len(), self.idx.files.iter().map(|f| f.symbols.len()).sum::<usize>())
    }

    /// Work in flight: a server request or batch of files, or the parent map, a re-index or a
    /// link on a thread.
    fn working(&self) -> bool {
        self.asked > 0 || !self.indexing.is_empty() || self.base_rx.is_some() || self.reindex_rx.is_some() || self.link_rx.is_some()
    }

    // ---- history ----

    fn here(&self) -> Loc {
        Loc { tab: self.tab, file: self.cur_file.map(|fi| self.idx.files[fi].path.clone()), sel: self.sel, focus: self.focus.map(|r| self.idx.key(r)), path: self.sel_path, step: self.sel_anchor }
    }

    /// Once a frame: whatever moved the reader, the place they left goes into the history and
    /// the forward stack is dropped.
    fn track_navigation(&mut self) {
        let now = self.here();
        if let Some(prev) = self.last_loc.take() {
            if !prev.same_place(&now) {
                self.history.push(prev);
                self.forward.clear();
                if self.history.len() > 200 {
                    self.history.remove(0);
                }
            }
        }
        self.last_loc = Some(now);
    }

    fn back(&mut self) {
        let Some(loc) = self.history.pop() else { return };
        self.forward.push(self.here());
        self.go(loc);
    }

    fn forward(&mut self) {
        let Some(loc) = self.forward.pop() else { return };
        self.history.push(self.here());
        self.go(loc);
    }

    /// Restore a place without it counting as a navigation. The tab is set last, since
    /// selecting a step moves the tab on its own.
    fn go(&mut self, loc: Loc) {
        self.sel_path = loc.path.filter(|&pi| pi < self.map.paths.len());
        match (self.sel_path, loc.step) {
            (Some(pi), Some(ai)) if ai < self.map.paths[pi].anchors.len() => self.select_step(pi, ai, false),
            _ => {
                self.sel_anchor = None;
                if let Some(r) = loc.focus.and_then(|k| self.idx.by_key(&k)) {
                    self.focus(r);
                }
            }
        }
        self.cur_file = loc.file.and_then(|p| self.idx.find_file(&p));
        self.sel = loc.sel;
        if let Some((a, _)) = loc.sel {
            self.scroll_to = Some(a);
        }
        self.tab = loc.tab;
        self.last_loc = Some(self.here());
    }

    /// Every per-step record of path `pi` after its step `ai` was removed: the step's own
    /// entries go and the ones above it move down one, in the fold, code, whole-symbol and
    /// context sets and in the history.
    fn step_removed(&mut self, pi: usize, ai: usize) {
        let shift = |a: usize| if a == ai { None } else if a > ai { Some(a - 1) } else { Some(a) };
        self.remap_steps(|(p, a)| if p == pi { shift(a).map(|a| (p, a)) } else { Some((p, a)) });
        for loc in self.history.iter_mut().chain(self.forward.iter_mut()).chain(self.last_loc.iter_mut()) {
            if loc.path == Some(pi) {
                loc.step = loc.step.and_then(shift);
            }
        }
    }

    /// The same after path `pi` was removed: its records go, later paths move down one.
    fn path_removed(&mut self, pi: usize) {
        let shift = |p: usize| if p == pi { None } else if p > pi { Some(p - 1) } else { Some(p) };
        self.remap_steps(|(p, a)| shift(p).map(|p| (p, a)));
        self.history.retain(|l| l.path != Some(pi));
        self.forward.retain(|l| l.path != Some(pi));
        for loc in self.history.iter_mut().chain(self.forward.iter_mut()).chain(self.last_loc.iter_mut()) {
            loc.path = loc.path.and_then(shift);
        }
        self.top_step = None;
    }

    /// Move the selected step through the path in reading order.
    fn step_by(&mut self, d: i32) {
        let Some(pi) = self.sel_path.filter(|&pi| pi < self.map.paths.len()) else { return };
        let order = self.map.tree_order(pi);
        if order.is_empty() {
            return;
        }
        let at = self.sel_anchor.and_then(|ai| order.iter().position(|&(a, _)| a == ai)).map_or(0, |i| (i as i32 + d).clamp(0, order.len() as i32 - 1) as usize);
        self.select_step(pi, order[at].0, false);
    }

    // ---- selection ----

    /// Make `r` the selected symbol: listing position and xrefs. Moves the view, not the tab.
    fn focus(&mut self, r: SymRef) {
        let s = self.idx.sym(r);
        let (start, end) = (s.start, s.end);
        self.cur_file = Some(r.file);
        self.sel = Some((start, end));
        self.scroll_to = Some(start);
        self.focus = Some(r);
        if !std::mem::take(&mut self.graph.hold_look) {
            self.graph.want_look = true;
        }
    }

    /// Select a symbol reached outside any path. When it is a whole-symbol step of the path
    /// being read, that step is selected instead so every view agrees.
    pub fn select_symbol(&mut self, r: SymRef) {
        if let Some(pi) = self.sel_path {
            let file = &self.idx.files[r.file].path;
            if let Some(ai) = self.map.paths[pi].anchors.iter().position(|a| a.sym == Some(r.sym) && a.file == *file && a.off_start == 0) {
                self.select_step(pi, ai, false);
                return;
            }
        }
        self.sel_anchor = None;
        self.focus(r);
    }

    /// Select a step. The document scrolls its header to the top unless the click came from
    /// inside the document.
    pub fn select_step(&mut self, pi: usize, ai: usize, in_document: bool) {
        self.sel_path = Some(pi);
        self.sel_anchor = Some(ai);
        // a folded ancestor would hide the step: unfold the way down to it
        let mut up = self.map.paths[pi].anchors[ai].parent;
        while up >= 0 {
            self.folded.remove(&(pi, up as usize));
            up = self.map.paths[pi].anchors[up as usize].parent;
        }
        let a = &self.map.paths[pi].anchors[ai];
        let (file, sym, ls, le) = (a.file.clone(), a.sym, a.line_start, a.line_end);
        match (self.idx.find_file(&file), sym) {
            (Some(fi), Some(si)) => {
                self.focus(SymRef { file: fi, sym: si });
                self.sel = Some((ls, le));
                self.scroll_to = Some(ls);
            }
            (Some(fi), None) => {
                // lines with no symbol of their own: the right panel shows the one around them
                self.cur_file = Some(fi);
                self.sel = Some((ls, le));
                self.scroll_to = Some(ls);
                if let Some(r) = self.idx.by_line(&file, ls) {
                    self.focus = Some(r);
                }
            }
            _ => {}
        }
        if !in_document {
            self.scroll_to_step = Some((ai, 5));
        }
        if matches!(self.tab, Tab::Listing | Tab::Results | Tab::Diff) {
            self.tab = Tab::Path;
        }
    }

    /// Open a path for reading: its first step is selected.
    fn select_path(&mut self, pi: usize) {
        self.sel_path = Some(pi);
        match self.map.tree_order(pi).first() {
            Some(&(ai, _)) => self.select_step(pi, ai, false),
            None => self.sel_anchor = None,
        }
    }

    /// Select `r` and show its code: the graph or document follows when one is up, else the
    /// listing opens.
    fn go_to_symbol(&mut self, r: SymRef) {
        self.select_symbol(r);
        if self.sel_anchor.is_none() {
            if !matches!(self.tab, Tab::Graph | Tab::Path) {
                self.tab = Tab::Listing;
            } else if let Some(pi) = self.sel_path {
                self.status = format!("{} selected; not a step of '{}'", self.idx.sym(r).name, self.map.paths[pi].name);
            }
        }
    }

    fn open_line(&mut self, file: usize, line: usize) {
        self.cur_file = Some(file);
        self.sel = Some((line, line));
        self.scroll_to = Some(line);
        self.tab = Tab::Listing;
    }

    /// The identifier at `col` of a line, as (first column, text).
    fn word_at(&self, fi: usize, li: usize, col: usize) -> Option<(usize, String)> {
        let chars: Vec<char> = self.idx.files[fi].lines.get(li)?.chars().collect();
        let is_id = |c: &char| c.is_alphanumeric() || *c == '_';
        if !chars.get(col).is_some_and(is_id) {
            return None;
        }
        let start = (0..col).rev().take_while(|&i| is_id(&chars[i])).last().unwrap_or(col);
        let end = (col..chars.len()).take_while(|&i| is_id(&chars[i])).last().unwrap_or(col);
        Some((start, chars[start..=end].iter().collect()))
    }

    /// The position the language's server is asked about for the identifier at a column, when
    /// there is a server for the file. Err(true) when the file has a grammar and names are
    /// looked up in the index instead; Err(false) when nothing answers for it.
    fn probe_for(&self, fi: usize, li: usize, col: usize) -> Result<(&'static index::Lang, Probe), bool> {
        let f = &self.idx.files[fi];
        match index::lang_for(&f.path).filter(|l| !self.no_server.contains(l.server)) {
            Some(lang) => Ok((lang, Probe { path: f.path.clone(), hash: f.hash, line: li as u32, col: utf16_col(&f.lines[li], col) })),
            None => Err(index::lang_for(&f.path).is_some()),
        }
    }

    /// What is known about the identifier at a position. A language with a live server is
    /// asked once per position, after the pointer has rested on it for a moment and with no
    /// other hover in flight, and answers a frame or more later; a language with only a
    /// grammar answers with the symbol of that name; prose answers nothing.
    pub fn probe(&mut self, fi: usize, li: usize, col: usize) -> Option<Tip> {
        let (start, _) = self.word_at(fi, li, col)?;
        match self.probe_for(fi, li, start) {
            Ok((lang, p)) => match self.hovers.get(&p) {
                Some(Some(t)) => t.clone().map(Tip::Text),
                Some(None) => None,
                None => {
                    let since = match &self.hover_want {
                        Some((w, t)) if *w == p => *t,
                        _ => {
                            self.hover_want = Some((p.clone(), self.now));
                            self.now
                        }
                    };
                    if self.now - since >= 0.15 && !self.hover_inflight {
                        if self.hovers.len() > 500 {
                            self.hovers.clear();
                        }
                        self.hovers.insert(p.clone(), None);
                        if let Some(tx) = self.server(lang) {
                            let _ = tx.send(Req::Hover(p));
                            self.asked += 1;
                            self.hover_inflight = true;
                        }
                    }
                    None
                }
            },
            Err(true) => self.symbol_at(fi, li, col).map(Tip::Sym),
            Err(false) => None,
        }
    }

    /// Look up where the identifier at a position is defined and act on it: the server
    /// answers later through `poll_backend`, the index at once.
    fn probe_def(&mut self, fi: usize, li: usize, col: usize, intent: Intent) {
        let Some((start, word)) = self.word_at(fi, li, col) else { return };
        match self.probe_for(fi, li, start) {
            Ok((lang, p)) => {
                if let Some(tx) = self.server(lang) {
                    let _ = tx.send(Req::Def(p.clone()));
                    self.asked += 1;
                    self.want_def = Some((p, intent));
                }
            }
            Err(true) => match self.symbol_at(fi, li, col) {
                Some(r) => self.land(Peek::Sym(r), intent),
                None => self.status = format!("no definition of '{word}' in this repo"),
            },
            Err(false) => {}
        }
    }

    /// Go to, or pin, what a definition lookup found.
    /// A definition outside the repo is pinned whatever the intent, since only the peek shows it.
    fn land(&mut self, found: Peek, intent: Intent) {
        match (intent, found) {
            (Intent::Jump, Peek::Sym(r)) => {
                self.select_symbol(r);
                let on_step = self.sel_anchor.is_some() && self.tab == Tab::Path;
                if self.tab != Tab::Graph && !on_step {
                    self.tab = Tab::Listing;
                }
            }
            (Intent::Jump, Peek::Line(fi, li)) => self.open_line(fi, li),
            (_, found) => {
                if let Peek::Outside(p, li, _, _) = &found {
                    self.status = format!("defined outside the repo: {}:{}", p.display(), li + 1);
                }
                self.peek = Some(found);
            }
        }
    }

    /// The symbol the identifier at `col` names, by name: a definition that lists this line
    /// among its references wins, then one in the same file, then any. For files whose
    /// language has a grammar but no server.
    fn symbol_at(&self, fi: usize, li: usize, col: usize) -> Option<SymRef> {
        let (_, word) = self.word_at(fi, li, col)?;
        let cands = self.idx.find_symbols(&word);
        let here = (self.idx.files[fi].path.clone(), li as u32);
        cands.iter().copied().find(|r| self.idx.sym(*r).refs.contains(&here)).or_else(|| cands.iter().copied().find(|r| r.file == fi)).or_else(|| cands.first().copied())
    }

    // ---- edits ----

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
        let parent = self.sel_anchor.filter(|&ai| ai < self.map.paths[pi].anchors.len()).map_or(-1, |ai| ai as i32);
        let ai = self.map.add_anchor(&self.idx, pi, fi, a.min(b), a.max(b), Author::Human, parent);
        self.sel_anchor = Some(ai);
        self.dirty = true;
        self.status = format!("step [{ai}] added to '{}' under [{parent}]", self.map.paths[pi].name);
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
            Action::ToggleStep(pi, ai) => toggle(&mut self.expanded_steps, (pi, ai)),
            Action::ToggleCode(pi, ai) => toggle(&mut self.collapsed, (pi, ai)),
            Action::ToggleFold(pi, ai) => toggle(&mut self.folded, (pi, ai)),
            Action::CollapseAll(pi, hide) => {
                self.collapsed.retain(|&(p, _)| p != pi);
                if hide {
                    self.collapsed.extend((0..self.map.paths[pi].anchors.len()).map(|ai| (pi, ai)));
                } else {
                    self.folded.retain(|&(p, _)| p != pi);
                }
            }
            Action::FoldAll(pi, fold) => {
                self.folded.retain(|&(p, _)| p != pi);
                if fold {
                    let with_kids: Vec<usize> = (0..self.map.paths[pi].anchors.len()).filter(|&ai| self.map.descendants(pi, ai) > 0).collect();
                    self.folded.extend(with_kids.into_iter().map(|ai| (pi, ai)));
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
                if dir == 0 {
                    self.context.remove(&(pi, ai));
                } else {
                    let e = self.context.entry((pi, ai)).or_default();
                    if dir < 0 {
                        e.0 += CONTEXT_LINES;
                    } else {
                        e.1 += CONTEXT_LINES;
                    }
                }
            }
            Action::SelectLine(li, extend) => {
                self.sel = match (extend, self.sel) {
                    (true, Some((a, _))) => Some((a, li)),
                    _ => Some((li, li)),
                };
            }
            Action::ClosePeek => self.peek = None,
            Action::ToggleDir(d) => toggle(&mut self.dir_toggled, d),
            Action::Tab(t) => self.tab = t,
            Action::Back => self.back(),
            Action::Forward => self.forward(),
            Action::Save => self.save(),
            Action::PinSelection => self.add_selection(),
        }
    }

    // ---- widgets ----

    pub fn label(&mut self, s: &str, color: Color) {
        self.ui.leaf(text(s, self.px, color), Layout::row().pad(2), Style::default(), None);
    }

    pub fn button(&mut self, s: &str, id: Id, selected: bool) -> Interaction {
        let it = self.ui.interaction_of(id);
        let bg = if selected { SELECTED } else if it.hovered { HOVER } else { PANEL };
        self.ui.leaf(text(s, self.px, if selected || it.hovered { TEXT } else { WEAK }), Layout::row().pad(4), Style::bg(bg).border(ui::BORDER_ALL, BORDER), Some(id))
    }

    pub fn small_button(&mut self, s: &str, id: Id) -> Interaction {
        self.small_button_w(s, 0, id)
    }

    /// A small button `cols` cells wide whatever its label, so a label that changes on click
    /// does not move the buttons beside it; 0 fits the label.
    pub fn small_button_w(&mut self, s: &str, cols: usize, id: Id) -> Interaction {
        let it = self.ui.interaction_of(id);
        let layout = if cols > 0 { Layout::row().pad(2).w(cols as i32 * self.cell.0 + 4) } else { Layout::row().pad(2) };
        self.ui.leaf(text(s, self.px, if it.hovered { TEXT } else { WEAK }), layout, Style::bg(if it.hovered { HOVER } else { FIELD }).border(ui::BORDER_ALL, BORDER), Some(id))
    }

    /// A navigation button three cells wide, dimmed when it has nowhere to go.
    fn nav_button(&mut self, s: &str, id: Id, enabled: bool) -> Interaction {
        let it = self.ui.interaction_of(id);
        let color = if !enabled { dim(WEAK, 90) } else if it.hovered { TEXT } else { WEAK };
        self.ui.leaf(text(s, self.px, color), Layout::row().pad(4).w(3 * self.cell.0 + 8), Style::bg(if it.hovered && enabled { HOVER } else { PANEL }).border(ui::BORDER_ALL, BORDER), Some(id))
    }

    /// A selectable row: a full-width line of text with a hover and selection background.
    fn row(&mut self, runs_: Vec<(String, Color)>, id: Id, selected: bool) -> Interaction {
        self.row_bg(runs_, id, selected.then_some(SELECTED))
    }

    /// A row with its own background when it is marked; hover shows otherwise.
    fn row_bg(&mut self, runs_: Vec<(String, Color)>, id: Id, marked: Option<Color>) -> Interaction {
        let it = self.ui.interaction_of(id);
        let bg = marked.or(if it.hovered { Some(HOVER) } else { None });
        self.ui.leaf(runs(runs_, self.px), Layout::row().grow_x().pad(2), Style { bg, ..Default::default() }, Some(id))
    }

    /// A text field; clicking it takes the keyboard. The text is clipped to the box and
    /// scrolled so the caret stays in view.
    fn field(&mut self, which: Which, hint: &str, width: i32) {
        let id = ui::id_with(ui::id("field"), hint);
        let f = &self.fields[which as usize];
        let focused = f.focused;
        let caret = f.cursor as i32 + 1;
        let r = f.runs(hint);
        let x = (caret * self.cell.0 - (width - 6)).max(0);
        let it = self.ui.open(Kind::None, Layout::row().w(width).pad(3).scroll(x, 0), Style::bg(FIELD).border(ui::BORDER_ALL, if focused { ACCENT } else { BORDER }), Some(id));
        self.ui.leaf(runs(r, self.px), Layout::row(), Style::default(), None);
        self.ui.close();
        if it.clicked {
            self.take_focus(which);
        }
    }

    fn take_focus(&mut self, which: Which) {
        for (i, f) in self.fields.iter_mut().enumerate() {
            f.focused = i == which as usize;
        }
    }

    /// Open a scrolling column with `id`; the wheel moves it and the offset is clamped to what
    /// it showed last frame. Close it with `ui.close()`.
    pub fn scroll_open(&mut self, id: Id, layout: Layout, style: Style) -> Interaction {
        let mut off = self.scrolls.get(&id).copied().unwrap_or(0);
        let off = self.ui.scroll_by_wheel(id, &mut off);
        self.scrolls.insert(id, off);
        self.ui.open(Kind::None, layout.scroll(0, off), style, Some(id))
    }

    /// A virtual list of `n` rows `row_h` high in the scrolling column `id`, whose rectangle is
    /// `rect`: the first row in view and how many fit (`guess` before the column has a size).
    /// Only those rows are built. This emits the spacer standing for the rows above; the caller
    /// ends the list with a `spacer` for the rows below.
    fn rows_window(&mut self, id: Id, rect: Option<Rect>, n: usize, row_h: i32, guess: usize) -> (usize, usize) {
        let off = self.scrolls.get(&id).copied().unwrap_or(0);
        let visible = rect.map_or(guess, |r| (r.h / row_h + 2) as usize);
        let first = ((off / row_h).max(0) as usize).min(n);
        self.spacer(first as i32 * row_h);
        (first, visible)
    }

    /// Empty space `h` pixels high.
    fn spacer(&mut self, h: i32) {
        self.ui.leaf(Kind::None, Layout::row().h(h), Style::default(), None);
    }

    /// The drawn form of lines `lo..=hi` of a file, built once per (file text, range) and
    /// shared by every element that shows it. The cache is dropped whole when it grows large
    /// or the index changes.
    pub fn grid(&mut self, fi: usize, lo: usize, hi: usize) -> Rc<Glyphs> {
        let key = (fi, self.idx.files[fi].hash, lo, hi);
        if let Some(g) = self.grids.get(&key) {
            return g.clone();
        }
        if self.grids.len() > 512 {
            self.grids.clear();
        }
        let g = Rc::new(build_grid(&self.idx.files[fi], lo, hi));
        self.grids.insert(key, g.clone());
        g
    }

    /// A block of a file's lines as one element drawn from its grid. Hovering an identifier
    /// shows its tooltip, ctrl-click or double-click jumps to its definition, alt-click
    /// peeks. `bg(line)` paints a row, `bar(line)` its left edge; `wide` takes the full width
    /// rather than the grid's. Returns the interaction and, under the pointer, the line and
    /// the text column (None in the gutter).
    pub fn code_block(&mut self, fi: usize, lo: usize, hi: usize, id: Id, wide: bool, bg: &dyn Fn(usize) -> Option<Color>, bar: &dyn Fn(usize) -> Option<Color>) -> (Interaction, Option<(usize, Option<usize>)>) {
        let hi = hi.min(self.idx.files[fi].lines.len().saturating_sub(1));
        if hi < lo || self.idx.files[fi].lines.is_empty() {
            return (Interaction::default(), None);
        }
        let g = self.grid(fi, lo, hi);
        let px = self.px;
        let (cw, rh) = self.cell;
        let (w, h) = (g.w as i32 * cw, g.h as i32 * rh);
        // shift+wheel over the block scrolls it sideways when its lines are wider than it
        let prev = self.ui.interaction_of(id);
        let mut hx = self.hscroll.get(&id).copied().unwrap_or(0);
        if prev.hovered && self.ui.input.mods.shift && self.ui.input.wheel.1 != 0.0 {
            hx -= (self.ui.input.wheel.1 * 3.0 * cw as f32) as i32;
        }
        hx = hx.clamp(0, prev.rect.map_or(0, |r| (w - r.w).max(0)));
        self.hscroll.insert(id, hx);
        let rows: Vec<(Option<Color>, Option<Color>)> = (lo..=hi).map(|li| (bg(li), bar(li))).collect();
        let draw = move |gfx: &mut Gfx, r: Rect| {
            for (k, (bg, bar)) in rows.iter().enumerate() {
                let y = r.y + k as i32 * rh;
                if let Some(c) = bg {
                    gfx.rect(Rect::new(r.x, y, r.w, rh), *c);
                }
                if let Some(c) = bar {
                    gfx.rect(Rect::new(r.x, y, cw - 1, rh), *c);
                }
            }
            gfx.glyphs(r.x - hx, r.y, px, &g);
            if hx > 0 {
                gfx.rect(Rect::new(r.x, r.y, 2, r.h), WEAK); // the left edge is cut off
            }
        };
        let layout = if wide { Layout::row().grow_x().h(h) } else { Layout::row().w(w).h(h) };
        let it = self.ui.leaf(Kind::Custom(Box::new(draw)), layout, Style::default(), Some(id));
        let mut at = None;
        if let Some(rect) = it.rect.filter(|_| it.hovered) {
            let (mx, my) = self.ui.input.mouse;
            let row = ((my - rect.y) / rh).clamp(0, (hi - lo) as i32) as usize;
            let col = ((mx - rect.x + hx) / cw).max(0) as usize;
            at = Some((lo + row, col.checked_sub(GUTTER)));
        }
        if let Some((li, Some(col))) = at {
            let mods = self.ui.input.mods;
            if it.clicked && mods.alt {
                self.actions.push(Action::PeekAt(fi, li, col));
            } else if it.double_clicked || (it.clicked && mods.ctrl) {
                self.actions.push(Action::Jump(fi, li, col));
            } else if !self.ui.input.down[0] {
                if let Some(t) = self.probe(fi, li, col) {
                    self.tooltip = Some((t, self.ui.input.mouse));
                }
            }
        }
        (it, at)
    }

    // ---- panels ----

    fn top_bar(&mut self) {
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(6).cross(Align::Center), Style::bg(PANEL).border(BORDER_BOTTOM, BORDER), None);
        self.label("search", WEAK);
        self.field(Which::Search, "regex", 24 * self.cell.0);
        let (has_back, has_forward) = (!self.history.is_empty(), !self.forward.is_empty());
        if self.nav_button("<", ui::id("back"), has_back).clicked {
            self.actions.push(Action::Back);
        }
        if self.nav_button(">", ui::id("forward"), has_forward).clicked {
            self.actions.push(Action::Forward);
        }
        let n = self.results.len();
        let results = format!("Results ({n})");
        for (t, name) in [(Tab::Path, "Path"), (Tab::Diff, "Diff"), (Tab::Graph, "Graph"), (Tab::Listing, "Listing"), (Tab::Results, results.as_str())] {
            if self.button(name, ui::id_with(ui::id("tab"), name.split(' ').next().unwrap_or(name)), self.tab == t).clicked {
                self.actions.push(Action::Tab(t));
            }
        }
        self.ui.leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
        self.label("new path", WEAK);
        self.field(Which::NewPath, "name", 16 * self.cell.0);
        if self.button("pin selection", ui::id("pin"), false).clicked {
            self.actions.push(Action::PinSelection);
        }
        let save = if self.dirty { "save *" } else { "save" };
        if self.button(save, ui::id("save"), false).clicked {
            self.actions.push(Action::Save);
        }
        self.ui.close();
    }

    fn status_bar(&mut self) {
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(12).cross(Align::Center), Style::bg(PANEL).border(BORDER_TOP, BORDER), None);
        let p = self.map_path.display().to_string().replace('\\', "/");
        self.label(&p, WEAK);
        let s: String = self.status.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
        self.label(&s, TEXT);
        if !self.backend_progress.is_empty() {
            let b = self.backend_progress.clone();
            self.label(&b, WEAK);
        }
        self.ui.close();
    }

    fn output_panel(&mut self, h: i32) {
        let px = self.px;
        let id = ui::id("output");
        self.ui.open(Kind::None, Layout::col().grow_x().h(h), Style::bg(FIELD).border(BORDER_TOP, BORDER), None);
        // a command's output exists only after this frame's layout, so the pin to the end is
        // asked for two frames running: the second one lands on the new content
        if self.output_bottom > 0 {
            self.output_bottom -= 1;
            self.scrolls.insert(id, i32::MAX);
        }
        let it = self.scroll_open(id, Layout::col().grow().pad(4), Style::default());
        let row_h = self.cell.1;
        let n = self.output.lines().count();
        let (first, visible) = self.rows_window(id, it.rect, n, row_h, 20);
        for line in self.output.lines().skip(first).take(visible) {
            self.ui.leaf(text(line, px, TEXT), Layout::row(), Style::default(), None);
        }
        self.spacer(n.saturating_sub(first + visible) as i32 * row_h);
        self.ui.close();
        let focus = self.fields[Which::Cmd as usize].focused;
        let it = self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(4).cross(Align::Center), Style::bg(if focus { PANEL } else { FIELD }).border(BORDER_TOP, BORDER), Some(ui::id("cmd")));
        if it.clicked {
            self.take_focus(Which::Cmd);
        }
        self.ui.leaf(text(">", px, WEAK), Layout::row(), Style::default(), None);
        let r = self.fields[Which::Cmd as usize].runs("");
        self.ui.leaf(runs(r, px), Layout::row().grow_x(), Style::default(), None);
        self.ui.close();
        self.ui.close();
    }

    fn left_panel(&mut self, w: i32) {
        self.ui.open(Kind::None, Layout::col().w(w).grow_y(), Style::bg(PANEL).border(BORDER_RIGHT, BORDER), None);
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(4), Style::default(), None);
        for (t, name) in [(LeftTab::Paths, "Paths"), (LeftTab::Symbols, "Symbols"), (LeftTab::Files, "Files")] {
            if self.button(name, ui::id_with(ui::id("left"), name), self.left == t).clicked {
                self.left = t;
            }
        }
        self.ui.close();
        match self.left {
            LeftTab::Paths => self.paths_window(),
            LeftTab::Symbols => self.symbols_window(),
            LeftTab::Files => self.files_window(),
        }
        self.ui.close();
    }

    fn paths_window(&mut self) {
        let diffs = self.diffs();
        let base = ui::id("paths");
        let it = self.scroll_open(base, Layout::col().grow().pad(4), Style::default());
        let cols = it.rect.map_or(44, |r| ((r.w - 8 - ui::SCROLLBAR_W) / self.cell.0.max(1)).max(20)) as usize;
        for pi in 0..self.map.paths.len() {
            let (name, kind, tag, n, stale) = {
                let p = &self.map.paths[pi];
                (p.name.clone(), p.kind.name(), p.author.tag(), p.anchors.len(), p.anchors.iter().filter(|a| a.stale).count())
            };
            let mark = match diffs.iter().find(|d| d.name == name).map(|d| d.change) {
                Some(Change::Added) => "+ ",
                Some(Change::Changed) => "~ ",
                _ => "",
            };
            let color = if stale > 0 { RED } else if !mark.is_empty() { GREEN } else { TEXT };
            let selected = self.sel_path == Some(pi);
            let rest = format!(" [{kind}]{tag}  {n} steps");
            let it = self.row(vec![(format!("{mark}{}{rest}", trunc(&name, cols.saturating_sub(mark.len() + rest.chars().count()))), color)], ui::id_n(base, pi), selected);
            if it.clicked {
                self.actions.push(Action::OpenPath(pi, Tab::Path));
            }
            if !selected {
                continue;
            }
            // the outline: every step of the path being read, folded subtrees hidden
            let numbered = self.map.numbered(&self.idx, pi);
            let mut hide_below: Option<usize> = None;
            for (ai, depth, number) in numbered {
                if hide_below.is_some_and(|d| depth > d) {
                    continue;
                }
                hide_below = None;
                let a = &self.map.paths[pi].anchors[ai];
                let hidden = if self.folded.contains(&(pi, ai)) {
                    hide_below = Some(depth);
                    self.map.descendants(pi, ai)
                } else {
                    0
                };
                let name = if a.symbol.is_empty() { "(lines)" } else { a.symbol.as_str() };
                let file = a.file.rsplit('/').next().unwrap_or("").to_owned();
                let stale = a.stale;
                let line = format!("  {}{number}  {name}{}", "  ".repeat(depth), if hidden > 0 { format!("  +{hidden}") } else { String::new() });
                let at_top = self.top_step == Some(ai);
                let marked = if self.sel_anchor == Some(ai) { Some(SELECTED) } else if at_top { Some(dim(SELECTED, 110)) } else { None };
                let it = self.row_bg(vec![(line, if stale { RED } else if at_top { TEXT } else { WEAK }), (format!("  {file}"), dim(WEAK, 140))], ui::id_n(ui::id("outline"), ai), marked);
                if it.clicked {
                    self.actions.push(Action::SelectStep(pi, ai, false));
                }
            }
            // the outline follows the document: when the topmost step changes and its row is
            // out of view, scroll it to the upper third, never under a scrollbar drag
            if let (Some(ai), Some((_, view))) = (self.top_step, self.ui.content_of(base)) {
                if self.outline_shown != Some(ai) && !self.ui.dragging() {
                    if let Some(row) = self.ui.interaction_of(ui::id_n(ui::id("outline"), ai)).rect {
                        if row.y < view.y || row.bottom() > view.bottom() {
                            let off = self.scrolls.get(&base).copied().unwrap_or(0) + (row.y - view.y) - view.h / 3;
                            self.scrolls.insert(base, off.max(0));
                        }
                    }
                    self.outline_shown = Some(ai);
                }
            }
        }
        for d in diffs.iter().filter(|d| d.change == Change::Removed) {
            let s = format!("- {}  (removed, {} steps)", d.name, d.removed.len());
            self.label(&s, WEAK);
        }
        self.ui.close();
    }

    fn symbols_window(&mut self) {
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(4).cross(Align::Center), Style::default(), None);
        self.label("+ = in a path", WEAK);
        self.field(Which::SymFilter, "filter", 16 * self.cell.0);
        self.ui.close();
        let filter = self.fields[Which::SymFilter as usize].text.to_lowercase();
        let rows: Vec<SymRef> = self
            .idx
            .files
            .iter()
            .enumerate()
            .flat_map(|(file, f)| (0..f.symbols.len()).map(move |sym| SymRef { file, sym }))
            .filter(|r| filter.is_empty() || self.idx.sym(*r).name.to_lowercase().contains(&filter) || self.idx.files[r.file].path.to_lowercase().contains(&filter))
            .collect();
        let id = ui::id("symbols");
        let it = self.scroll_open(id, Layout::col().grow().pad(4), Style::default());
        let row_h = self.cell.1 + 4;
        let cols = it.rect.map_or(60, |r| ((r.w - 8 - ui::SCROLLBAR_W) / self.cell.0.max(1)).max(30)) as usize;
        let (first, visible) = self.rows_window(id, it.rect, rows.len(), row_h, 40);
        for &r in rows.iter().skip(first).take(visible) {
            let s = self.idx.sym(r);
            let covered = if self.map.covers(&self.idx.files[r.file].path, s.start, s.end) { "+" } else { " " };
            let indent = if s.depth > 0 { "  " } else { "" };
            let place = format!("{}:{}", self.idx.files[r.file].path, s.start + 1);
            let line = format!("{covered} {indent}{:<20} {:<8} {}", trunc(&s.name, 20), trunc(&s.kind, 8), trunc_left(&place, cols.saturating_sub(32 + indent.len())));
            let pending = self.idx.files[r.file].pending;
            let it = self.row(vec![(line, if pending { dim(WEAK, 120) } else { TEXT })], ui::id_n(ui::id("sym"), r.file * 100_000 + r.sym), self.focus == Some(r));
            if it.clicked {
                self.actions.push(Action::Focus(r));
            }
        }
        self.spacer(rows.len().saturating_sub(first + visible) as i32 * row_h);
        self.ui.close();
    }

    fn files_window(&mut self) {
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4), Style::default(), None);
        self.label("covered/total symbols", WEAK);
        self.ui.close();
        let cov: Vec<(usize, usize)> = self.idx.files.iter().map(|f| (f.symbols.iter().filter(|s| self.map.covers(&f.path, s.start, s.end)).count(), f.symbols.len())).collect();
        self.scroll_open(ui::id("files"), Layout::col().grow().pad(4), Style::default());
        let all: Vec<usize> = (0..self.idx.files.len()).collect();
        self.files_tree(&all, 0, &cov);
        self.ui.close();
    }

    /// `files` are sorted by path and share their first `depth` components; a run with the
    /// same next component is a directory.
    fn files_tree(&mut self, files: &[usize], depth: usize, cov: &[(usize, usize)]) {
        let comp = |app: &App, fi: usize| app.idx.files[fi].path.split('/').nth(depth).unwrap_or("").to_owned();
        let is_file = |app: &App, fi: usize| app.idx.files[fi].path.split('/').count() == depth + 1;
        let indent = "  ".repeat(depth);
        let mut i = 0;
        while i < files.len() {
            let fi = files[i];
            if is_file(self, fi) {
                let (c, t) = cov[fi];
                let label = if t > 0 { format!("{indent}{}  {c}/{t}", comp(self, fi)) } else { format!("{indent}{}", comp(self, fi)) };
                let it = self.row(vec![(label, if t > 0 && c == 0 { WEAK } else { TEXT })], ui::id_n(ui::id("file"), fi), self.cur_file == Some(fi));
                if it.clicked {
                    self.actions.push(Action::GoTo(fi, 0));
                }
                i += 1;
                continue;
            }
            let dir = comp(self, fi);
            let j = i + files[i..].iter().take_while(|&&g| !is_file(self, g) && comp(self, g) == dir).count();
            let (c, t) = files[i..j].iter().fold((0, 0), |acc, &g| (acc.0 + cov[g].0, acc.1 + cov[g].1));
            let prefix: String = self.idx.files[fi].path.split('/').take(depth + 1).collect::<Vec<_>>().join("/");
            let open = (depth == 0) ^ self.dir_toggled.contains(&prefix);
            let it = self.row(vec![(format!("{indent}{} {dir}/  {c}/{t}", if open { "▾" } else { "▸" }), TEXT)], ui::id_with(ui::id("dir"), &prefix), false);
            if it.clicked {
                self.actions.push(Action::ToggleDir(prefix.clone()));
            }
            if open {
                self.files_tree(&files[i..j], depth + 1, cov);
            }
            i = j;
        }
    }

    fn xrefs_panel(&mut self, w: i32) {
        self.ui.open(Kind::None, Layout::col().w(w).grow_y(), Style::bg(PANEL).border(BORDER_LEFT, BORDER), None);
        // the peek: a symbol's definition, a line inside one, or text from a file outside the repo
        let peek = match self.peek.clone() {
            Some(Peek::Sym(r)) if r.file < self.idx.files.len() && r.sym < self.idx.files[r.file].symbols.len() => {
                let s = self.idx.sym(r);
                Some((format!("Peek: {}", s.name), format!("{}:{}", self.idx.files[r.file].path, s.start + 1), Some(Action::Focus(r)), Ok((r.file, s.start, s.end, None))))
            }
            Some(Peek::Line(fi, li)) if fi < self.idx.files.len() => {
                let last = self.idx.files[fi].lines.len().saturating_sub(1);
                Some(("Peek".to_owned(), format!("{}:{}", self.idx.files[fi].path, li + 1), Some(Action::GoTo(fi, li)), Ok((fi, li.saturating_sub(6), (li + 20).min(last), Some(li)))))
            }
            Some(Peek::Outside(p, li, first, grid)) => Some(("Peek (outside)".to_owned(), format!("{}:{}", p.display().to_string().replace('\\', "/"), li + 1), None, Err((first, li, grid)))),
            _ => None,
        };
        if let Some((title, place, go, body)) = peek {
            // the header fits the panel whatever the path's length: the buttons never leave it
            let cols = (w / self.cell.0.max(1)) as usize;
            let title = trunc(&title, 24.min(cols / 2));
            let place = trunc_left(&place, cols.saturating_sub(title.chars().count() + 14));
            self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(6).cross(Align::Center), Style::default(), None);
            self.label(&title, TEXT);
            self.label(&place, WEAK);
            self.ui.leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
            if let Some(go) = go {
                if self.small_button("go", ui::id("peek-go")).clicked {
                    self.actions.push(go);
                }
            }
            if self.small_button_w("x", 3, ui::id("peek-x")).clicked {
                self.actions.push(Action::ClosePeek);
            }
            self.ui.close();
            let rows = match &body {
                Ok((fi, start, end, _)) => end.min(&self.idx.files[*fi].lines.len().saturating_sub(1)).saturating_sub(*start) + 1,
                Err((_, _, grid)) => grid.h,
            };
            let h = (rows as i32 * self.cell.1 + 8).min((self.ui.size.1 / 3).max(100));
            self.scroll_open(ui::id("peek"), Layout::col().grow_x().h(h).pad(4), Style::bg(FIELD));
            match body {
                Ok((fi, start, end, mark)) => {
                    self.code_block(fi, start, end, ui::id("peekcode"), true, &|li| (mark == Some(li)).then_some(dim(SELECTED, 120)), &|_| None);
                }
                Err((first, mark, grid)) => {
                    let px = self.px;
                    let rh = self.cell.1;
                    let g = grid.clone();
                    let draw = move |gfx: &mut Gfx, r: Rect| {
                        if mark >= first && mark < first + g.h {
                            gfx.rect(Rect::new(r.x, r.y + (mark - first) as i32 * rh, r.w, rh), dim(SELECTED, 120));
                        }
                        gfx.glyphs(r.x, r.y, px, &g);
                    };
                    self.ui.leaf(Kind::Custom(Box::new(draw)), Layout::row().grow_x().h(grid.h as i32 * rh), Style::default(), Some(ui::id("peekout")));
                }
            }
            self.ui.close();
        }
        let Some(cur) = self.focus else {
            self.label("no symbol selected", WEAK);
            self.ui.close();
            return;
        };
        let s = self.idx.sym(cur);
        let (name, place) = (s.name.clone(), format!("{} {}:{}-{}", s.kind, self.idx.files[cur.file].path, s.start + 1, s.end + 1));
        let pending = self.idx.files[cur.file].pending;
        let (callers, callees, refs) = (s.callers.clone(), s.callees.clone(), s.refs.clone());
        self.ui.open(Kind::None, Layout::col().grow_x().pad(4), Style::default(), None);
        self.label(&name, TEXT);
        self.label(&place, WEAK);
        if pending {
            let server = index::lang_for(&self.idx.files[cur.file].path).map_or("the server", |l| l.server);
            self.label(&format!("waiting for {server}"), WEAK);
        }
        self.ui.close();
        self.scroll_open(ui::id("xrefs"), Layout::col().grow().pad(4), Style::default());
        let dimmed = if pending { dim(WEAK, 120) } else { TEXT };
        for (title, list, name) in [("Xrefs to", &callers, "xto"), ("Xrefs from", &callees, "xfrom")] {
            self.label(&format!("{title} ({})", list.len()), WEAK);
            for (i, r) in list.iter().enumerate() {
                let it = self.row(vec![(cli::describe(&self.idx, *r), dimmed)], ui::id_n(ui::id(name), i), false);
                if it.clicked && !pending {
                    self.actions.push(Action::Focus(*r));
                }
            }
        }
        let shown = refs.iter().filter(|(p, _)| self.idx.find_file(p).is_some()).count();
        self.label(&format!("References ({shown})"), WEAK);
        for (i, (path, line)) in refs.iter().enumerate() {
            if let Some(fi) = self.idx.find_file(path) {
                let t = self.idx.files[fi].lines.get(*line as usize).map(|l| l.trim()).unwrap_or("").to_owned();
                let it = self.row(vec![(format!("{path}:{}: ", line + 1), WEAK), (t, dimmed)], ui::id_n(ui::id("xref"), i), false);
                if it.clicked {
                    self.actions.push(Action::GoTo(fi, *line as usize));
                }
            }
        }
        self.ui.close();
        self.ui.close();
    }

    /// The reader's landing view: the selected path as one document.
    fn path_document(&mut self) {
        let Some(pi) = self.sel_path.filter(|&pi| pi < self.map.paths.len()) else {
            self.label(if self.map.paths.is_empty() { "no paths yet: the agent writes them (path-new, path-add in the output panel)" } else { "pick a path on the left" }, WEAK);
            return;
        };
        let px = self.px;
        let diff = self.diffs().into_iter().find(|d| d.name == self.map.paths[pi].name);
        let doc_id = ui::id("document");
        let header_id = |ai: usize| ui::id_n(ui::id("step"), ai);
        let numbered = self.map.numbered(&self.idx, pi);

        // from last frame's rectangles: the step under the top of the viewport, and a pending
        // scroll to a selected step
        if let Some((_, doc_rect)) = self.ui.content_of(doc_id) {
            let mut top = None;
            for &(ai, _, _) in &numbered {
                if let Some(r) = self.ui.interaction_of(header_id(ai)).rect {
                    if top.is_none() || r.y <= doc_rect.y + 1 {
                        top = Some(ai);
                    }
                }
            }
            if top.is_some() {
                self.top_step = top;
            }
            if let Some((ai, tries)) = self.scroll_to_step.take() {
                match self.ui.interaction_of(header_id(ai)).rect {
                    Some(r) => {
                        let off = self.scrolls.get(&doc_id).copied().unwrap_or(0) + (r.y - doc_rect.y);
                        self.scrolls.insert(doc_id, off.max(0));
                    }
                    None if tries > 0 => self.scroll_to_step = Some((ai, tries - 1)),
                    None => {}
                }
            }
        }

        // header
        let (name, kind, tag, n, note) = {
            let p = &self.map.paths[pi];
            (p.name.clone(), p.kind.name(), p.author.tag(), p.anchors.len(), p.note.clone())
        };
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(8).cross(Align::Center), Style::default(), None);
        self.ui.leaf(text(&name, px + px / 3, TEXT), Layout::row(), Style::default(), None);
        self.label(&format!("[{kind}]{tag}  {n} steps"), WEAK);
        match diff.as_ref().map(|d| d.change) {
            Some(Change::Added) => self.label("new since the parent revision", GREEN),
            Some(Change::Changed) => self.label("changed since the parent revision", GREEN),
            _ => {}
        }
        self.ui.leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
        if self.small_button("graph", ui::id("doc-graph")).clicked {
            self.actions.push(Action::OpenPath(pi, Tab::Graph));
        }
        if self.small_button("hide all code", ui::id("doc-collapse")).clicked {
            self.actions.push(Action::CollapseAll(pi, true));
        }
        if self.small_button("show all", ui::id("doc-expand")).clicked {
            self.actions.push(Action::CollapseAll(pi, false));
        }
        if self.small_button("fold all", ui::id("doc-fold")).clicked {
            self.actions.push(Action::FoldAll(pi, true));
        }
        if self.small_button("unfold all", ui::id("doc-unfold")).clicked {
            self.actions.push(Action::FoldAll(pi, false));
        }
        if self.small_button("delete path", ui::id("doc-delete")).clicked {
            self.actions.push(Action::DeletePath(pi));
        }
        self.ui.close();
        self.ui.leaf(wrapped(if note.is_empty() { "(no path note)" } else { &note }, px, if note.is_empty() { WEAK } else { TEXT }), Layout::row().grow_x().pad(4), Style::default(), None);

        // breadcrumb: the ancestors of the step under the top of the viewport
        let number_of: HashMap<usize, String> = numbered.iter().map(|(ai, _, n)| (*ai, n.clone())).collect();
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(6).cross(Align::Center), Style::bg(PANEL).border(BORDER_TOP | BORDER_BOTTOM, BORDER), None);
        let mut chain = Vec::new();
        let mut cur = self.top_step.filter(|&ai| ai < self.map.paths[pi].anchors.len());
        while let Some(ai) = cur {
            chain.push(ai);
            cur = usize::try_from(self.map.paths[pi].anchors[ai].parent).ok().filter(|&p| p < self.map.paths[pi].anchors.len() && !chain.contains(&p));
        }
        if chain.is_empty() {
            self.label(" ", WEAK);
        }
        for (i, &ai) in chain.iter().rev().enumerate() {
            if i > 0 {
                self.label("›", WEAK);
            }
            let a = &self.map.paths[pi].anchors[ai];
            let name = if a.symbol.is_empty() { "(lines)".to_owned() } else { a.symbol.clone() };
            let file = a.file.rsplit('/').next().unwrap_or("").to_owned();
            let crumb = format!("{} {name}", number_of.get(&ai).map(String::as_str).unwrap_or(""));
            let cid = ui::id_n(ui::id("crumb"), ai);
            let hovered = self.ui.interaction_of(cid).hovered;
            let it = self.ui.leaf(runs(vec![(crumb, if hovered { ACCENT } else { TEXT }), (format!(" {file}"), dim(WEAK, 140))], px), Layout::row().pad(2), Style::default(), Some(cid));
            if it.clicked {
                self.actions.push(Action::SelectStep(pi, ai, false));
            }
        }
        self.ui.close();

        // the steps
        self.scroll_open(doc_id, Layout::col().grow().pad(6).gap(2), Style::default());
        let mut hide_below: Option<usize> = None;
        for &(ai, depth, ref number) in &numbered {
            if hide_below.is_some_and(|d| depth > d) {
                continue;
            }
            hide_below = None;
            let indent = depth as i32 * 3 * self.cell.0;
            let (file, symbol, ls, le, stale, tag, anote) = {
                let a = &self.map.paths[pi].anchors[ai];
                (a.file.clone(), a.symbol.clone(), a.line_start, a.line_end, a.stale, a.author.tag(), a.note.clone())
            };
            let fi = self.idx.find_file(&file);
            let sym = fi.zip(self.map.paths[pi].anchors[ai].sym).map(|(fi, si)| &self.idx.files[fi].symbols[si]).map(|s| (s.start, s.end));
            let gone = match (fi, symbol.is_empty(), sym) {
                (None, _, _) => Some("file gone"),
                (Some(_), false, None) => Some("symbol gone"),
                _ => None,
            };
            let name = if symbol.is_empty() { "(lines)".to_owned() } else { symbol.clone() };
            let place = match gone {
                Some(g) => format!("{file} ({g})"),
                None => format!("{file}:{}-{}", ls + 1, le + 1),
            };
            let selected = self.sel_anchor == Some(ai);
            let folded = self.folded.contains(&(pi, ai));
            let collapsed = self.collapsed.contains(&(pi, ai));
            let ctx = self.context.get(&(pi, ai)).copied().unwrap_or((0, 0));
            let kids = self.map.descendants(pi, ai);
            // header row
            self.ui.open(Kind::None, Layout::row().grow_x().gap(6).cross(Align::Center), Style { bg: None, border: if selected { BORDER_LEFT } else { 0 }, border_color: ACCENT }, None);
            self.ui.leaf(Kind::None, Layout::row().w(indent + 4), Style::default(), None);
            if kids > 0 {
                if self.small_button(if folded { "▸" } else { "▾" }, ui::id_n(ui::id("fold"), ai)).clicked {
                    self.actions.push(Action::ToggleFold(pi, ai));
                }
            } else {
                self.ui.leaf(Kind::None, Layout::row().w(3 * self.cell.0), Style::default(), None);
            }
            let title = format!("{number}  {}{name}  ", if stale { "STALE " } else { "" });
            let hid = header_id(ai);
            let hovered = self.ui.interaction_of(hid).hovered;
            let mut r = vec![(title, if stale { RED } else if hovered { ACCENT } else { TEXT }), (place, WEAK), (tag.to_owned(), WEAK)];
            if folded {
                r.push((format!("  +{kids}"), WEAK));
            }
            if let Some(c) = diff.as_ref().filter(|d| d.change == Change::Changed).and_then(|d| d.steps.get(ai).copied().flatten()) {
                r.push((format!("  {}", c.tag()), GREEN));
            }
            let it = self.ui.leaf(runs(r, px), Layout::row().pad(2), Style { bg: if selected { Some(SELECTED) } else { None }, ..Default::default() }, Some(hid));
            if it.clicked {
                self.actions.push(Action::SelectStep(pi, ai, true));
            }
            // toggles keep their width across labels, and the one destructive button sits alone
            // at the far right, so nothing slides under a pointer that clicks twice
            if gone.is_none() && self.small_button_w(if collapsed { "code" } else { "hide code" }, 9, ui::id_n(ui::id("hide"), ai)).clicked {
                self.actions.push(Action::ToggleCode(pi, ai));
            }
            if !collapsed && sym.is_some_and(|s| s != (ls, le)) {
                let whole = self.expanded_steps.contains(&(pi, ai));
                if self.small_button_w(if whole { "slice" } else { "whole symbol" }, 12, ui::id_n(ui::id("whole"), ai)).clicked {
                    self.actions.push(Action::ToggleStep(pi, ai));
                }
            }
            if ctx != (0, 0) && self.small_button("no context", ui::id_n(ui::id("ctx0"), ai)).clicked {
                self.actions.push(Action::Context(pi, ai, 0));
            }
            self.ui.leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
            if self.small_button("delete", ui::id_n(ui::id("del"), ai)).clicked {
                self.actions.push(Action::DeleteStep(pi, ai));
            }
            self.ui.close();
            // note
            self.ui.open(Kind::None, Layout::row().grow_x(), Style::default(), None);
            self.ui.leaf(Kind::None, Layout::row().w(indent + 4 + 3 * self.cell.0), Style::default(), None);
            self.ui.leaf(wrapped(if anote.is_empty() { "(no note)" } else { &anote }, px, if anote.is_empty() { dim(WEAK, 120) } else { GREEN }), Layout::row().grow_x().pad(2), Style::default(), None);
            self.ui.close();
            // code: the slice, or the whole symbol, plus the context asked for above and below;
            // the slice is highlighted whenever anything else shows
            if let (Some(fi), None, false) = (fi, gone, collapsed) {
                let whole = self.expanded_steps.contains(&(pi, ai));
                let (blo, bhi) = if whole { sym.unwrap_or((ls, le)) } else { (ls, le) };
                let last = self.idx.files[fi].lines.len().saturating_sub(1);
                let (lo, hi) = (blo.saturating_sub(ctx.0), (bhi + ctx.1).min(last));
                let marked = (lo, hi) != (ls, le);
                self.ui.open(Kind::None, Layout::col().grow_x(), Style { bg: None, border: if selected { BORDER_LEFT } else { 0 }, border_color: ACCENT }, None);
                if lo > 0 {
                    self.ctx_button(pi, ai, -1, indent);
                }
                self.ui.open(Kind::None, Layout::row().grow_x(), Style::default(), None);
                self.ui.leaf(Kind::None, Layout::row().w(indent + 4), Style::default(), None);
                self.code_block(fi, lo, hi, ui::id_n(ui::id("doccode"), ai), true, &|li| (marked && li >= ls && li <= le).then_some(dim(SELECTED, 120)), &|_| None);
                self.ui.close();
                if hi < last {
                    self.ctx_button(pi, ai, 1, indent);
                }
                self.ui.close();
            }
            if folded {
                hide_below = Some(depth);
            }
            self.ui.leaf(Kind::None, Layout::row().h(6), Style::default(), None);
        }
        if let Some(d) = diff.as_ref().filter(|d| !d.removed.is_empty()) {
            self.label("steps removed since the parent revision:", WEAK);
            for a in &d.removed {
                let s = format!("- {} {}{}", a.file, a.symbol, if a.note.is_empty() { String::new() } else { format!("  -- {}", a.note) });
                self.label(&s, WEAK);
            }
        }
        self.ui.close();
    }

    /// The row above (`dir` -1) or below (1) a step's code whose button shows more lines there.
    fn ctx_button(&mut self, pi: usize, ai: usize, dir: i8, indent: i32) {
        let (label, name) = if dir < 0 { (format!("▲ {CONTEXT_LINES} lines above"), "ctx-a") } else { (format!("▼ {CONTEXT_LINES} lines below"), "ctx-b") };
        self.ui.open(Kind::None, Layout::row().grow_x(), Style::default(), None);
        self.ui.leaf(Kind::None, Layout::row().w(indent + 4), Style::default(), None);
        if self.small_button(&label, ui::id_n(ui::id(name), ai)).clicked {
            self.actions.push(Action::Context(pi, ai, dir));
        }
        self.ui.close();
    }

    fn listing(&mut self) {
        let Some(fi) = self.cur_file else {
            self.label("click a symbol to open its file", WEAK);
            return;
        };
        let id = ui::id("listing");
        let row_h = self.cell.1;
        let n = self.idx.files[fi].lines.len();
        if let Some(line) = self.scroll_to.take() {
            let h = self.ui.content_of(id).map_or(600, |(_, r)| r.h);
            self.scrolls.insert(id, ((line as i32 - 3).max(0) * row_h).min((n as i32 * row_h - h).max(0)));
        }
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(8).cross(Align::Center), Style::default(), None);
        let p = self.idx.files[fi].path.clone();
        self.label(&p, TEXT);
        self.label("click a line, shift-click to extend, then 'pin selection'; double-click or ctrl-click an identifier to jump; alt-click to peek", WEAK);
        self.ui.leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
        self.label("line", WEAK);
        self.field(Which::GotoLine, "", 8 * self.cell.0);
        self.ui.close();
        let it = self.scroll_open(id, Layout::col().grow().pad(4), Style::bg(FIELD));
        let (lo, hi) = self.sel.map(|(a, b)| (a.min(b), a.max(b))).unwrap_or((usize::MAX, usize::MAX));
        let anchors: Vec<(usize, usize, bool)> = self.sel_path.map(|pi| self.map.paths[pi].anchors.iter().filter(|a| a.file == self.idx.files[fi].path).map(|a| (a.line_start, a.line_end, a.stale)).collect()).unwrap_or_default();
        let (first, visible) = self.rows_window(id, it.rect, n, row_h, 60);
        let last = (first + visible).min(n).saturating_sub(1);
        if n > 0 && first <= last {
            let bar = |li: usize| anchors.iter().find(|&&(s, e, _)| li >= s && li <= e).map(|&(_, _, stale)| if stale { RED } else { GREEN });
            let (it, at) = self.code_block(fi, first, last, ui::id("lines"), true, &|li| (li >= lo && li <= hi).then_some(dim(SELECTED, 160)), &bar);
            let mods = self.ui.input.mods;
            if let Some((li, _)) = at {
                if it.clicked && !mods.ctrl && !mods.alt && !it.double_clicked {
                    self.actions.push(Action::SelectLine(li, mods.shift));
                }
            }
        }
        self.spacer(n.saturating_sub(first + visible) as i32 * row_h + row_h); // a row of slack so the last line clears the edge
        self.ui.close();
    }

    fn results_view(&mut self) {
        if self.results.is_empty() {
            self.label(if self.fields[Which::Search as usize].text.is_empty() { "type a regex in the search box and press enter" } else { "no hits" }, WEAK);
            return;
        }
        let id = ui::id("results");
        let row_h = self.cell.1 + 4;
        let it = self.scroll_open(id, Layout::col().grow().pad(4), Style::default());
        let (first, visible) = self.rows_window(id, it.rect, self.results.len(), row_h, 60);
        let rows: Vec<(usize, usize)> = self.results.iter().copied().skip(first).take(visible).collect();
        for (k, (fi, li)) in rows.into_iter().enumerate() {
            let f = &self.idx.files[fi];
            let line = f.lines[li].trim().to_owned();
            let it = self.row(vec![(format!("{}:{}: ", f.path, li + 1), WEAK), (line, TEXT)], ui::id_n(ui::id("hit"), first + k), false);
            if it.clicked {
                self.actions.push(Action::GoTo(fi, li));
            }
        }
        self.spacer(self.results.len().saturating_sub(first + visible) as i32 * row_h);
        self.ui.close();
    }

    /// The map against the parent revision's: what an agent session changed.
    fn diff_view(&mut self) {
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(8).cross(Align::Center), Style::default(), None);
        self.label("Changes against the parent revision", TEXT);
        self.label("jj file show -r @- .codemap", WEAK);
        if self.small_button("refresh", ui::id("diff-refresh")).clicked {
            self.load_base();
        }
        self.ui.close();
        if self.base.is_none() {
            self.label(if self.base_rx.is_some() { "asking jj..." } else { "no map in the parent revision: this needs a jj repo with a committed .codemap" }, WEAK);
            return;
        }
        let diffs = self.diffs();
        if diffs.iter().all(|d| d.change == Change::Same) {
            self.label("no changes", WEAK);
            return;
        }
        self.scroll_open(ui::id("diff"), Layout::col().grow().pad(4), Style::default());
        for (k, d) in diffs.iter().enumerate() {
            let (mark, color) = match d.change {
                Change::Same => continue,
                Change::Added => ("+", GREEN),
                Change::Removed => ("-", RED),
                Change::Changed => ("~", GREEN),
            };
            let summary = match d.change {
                Change::Added => format!("{} steps", d.steps.len()),
                Change::Removed => format!("{} steps", d.removed.len()),
                _ => {
                    let count = |c: StepChange| d.steps.iter().filter(|s| **s == Some(c)).count();
                    let mut parts = Vec::new();
                    for (n, what) in [(count(StepChange::Added), "new"), (d.removed.len(), "removed"), (count(StepChange::Repinned), "re-pinned"), (count(StepChange::NoteEdited), "note edited")] {
                        if n > 0 {
                            parts.push(format!("{n} {what}"));
                        }
                    }
                    if d.note_changed {
                        parts.push("path note or kind changed".into());
                    }
                    parts.join(", ")
                }
            };
            let pi = self.map.find(&d.name);
            let it = self.row(vec![(format!("{mark} {}   ", d.name), color), (summary, WEAK)], ui::id_n(ui::id("diffrow"), k), false);
            if it.clicked {
                match pi {
                    Some(pi) => self.actions.push(Action::OpenPath(pi, Tab::Path)),
                    None => self.status = format!("'{}' exists only in the parent revision; its {} steps are listed under the path it was removed from", d.name, d.removed.len()),
                }
            }
            if let Some(pi) = pi.filter(|_| d.change == Change::Changed) {
                for (i, c) in d.steps.iter().enumerate() {
                    if let Some(c) = c {
                        let a = &self.map.paths[pi].anchors[i];
                        let s = format!("    {} [{i}] {} {}:{}-{}  {}", if *c == StepChange::Added { "+" } else { "~" }, a.symbol, a.file, a.line_start + 1, a.line_end + 1, c.tag());
                        self.label(&s, WEAK);
                    }
                }
            }
            for a in &d.removed {
                let s = format!("    - {} {} (removed)", a.file, a.symbol);
                self.label(&s, WEAK);
            }
        }
        self.ui.close();
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
