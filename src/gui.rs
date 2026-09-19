//! The human's surface, built from `ui` elements and drawn by `gfx`. One selection (a symbol,
//! with a step behind it when reached through a path) drives every view; the panels are
//! functions that open and close elements each frame, and every click becomes an `Action`
//! applied once the frame is built.

use crate::cli;
use crate::gfx::{self, Color, Gfx, Rect};
use crate::index::{self, Index, ServerFile, SymRef};
use crate::map::{Author, Change, Kind as PathKind, Map, PathDiff, StepChange};
use crate::ui::{self, Align, Id, Interaction, Key, Kind, Layout, Measure, Style, Text, Ui, BORDER_BOTTOM, BORDER_LEFT, BORDER_RIGHT, BORDER_TOP};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
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

/// One source line as coloured runs, with its number in the gutter.
pub fn code_runs(f: &index::File, li: usize, gutter: bool) -> Vec<(String, Color)> {
    let mut out = Vec::new();
    if gutter {
        out.push((format!("{:5} ", li + 1), WEAK));
    }
    let line = &f.lines[li];
    let mut at = 0;
    for &(s, e, class) in f.hl.get(li).map(Vec::as_slice).unwrap_or(&[]) {
        let (s, e) = (s as usize, e.min(line.len() as u32) as usize);
        if s > at && s <= line.len() {
            out.push((line[at..s].to_owned(), TEXT));
        }
        if s < e {
            out.push((line[s..e].to_owned(), hl_color(class)));
        }
        at = e.max(at);
    }
    if at < line.len() {
        out.push((line[at..].to_owned(), TEXT));
    }
    out
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_owned() } else { s.chars().take(n.saturating_sub(1)).chain(std::iter::once('…')).collect() }
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
                    let line = if keep_on_enter { self.text.clone() } else { std::mem::take(&mut self.text) };
                    if !keep_on_enter {
                        self.cursor = 0;
                    }
                    self.hist_at = None;
                    if !line.trim().is_empty() {
                        self.history.push(line.clone());
                        submitted = Some(line);
                    }
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
                Key::Left => self.cursor = self.cursor.saturating_sub(1),
                Key::Right => self.cursor = (self.cursor + 1).min(self.text.chars().count()),
                Key::Home => self.cursor = 0,
                Key::End => self.cursor = self.text.chars().count(),
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
        vec![(self.text[..i].to_owned(), TEXT), (if self.focused { "▏" } else { "" }.to_owned(), ACCENT), (self.text[i..].to_owned(), TEXT)]
    }
}

/// What the language-server thread sends back.
enum Msg {
    File(ServerFile),
    Progress(String),
    Failed(&'static index::Lang, String),
    Done,
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
    SelectPath(usize),
    ShowPath(usize),
    SelectStep(usize, usize, bool), // (path, step, clicked inside the document)
    ToggleStep(usize, usize),       // show the whole symbol / just the slice in the document
    ToggleCode(usize, usize),       // hide / show a step's code
    ToggleFold(usize, usize),       // hide / show a step's subtree
    CollapseAll(usize, bool),
    DeleteStep(usize, usize),
    DeletePath(usize),
    GoTo(usize, usize),        // (file, line) in the listing
    Jump(usize, usize, usize), // (file, line, column): to the definition of the identifier there
    SelectLine(usize, bool),   // (line, extend) in the listing
    Peek(SymRef),
    ClosePeek,
    ToggleDir(String),
    Tab(Tab),
    Back,
    Forward,
    Save,
    NewPath,
    PinSelection,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok().and_then(|m| m.modified().ok())
}

pub struct App {
    pub idx: Index,
    backend: Option<Receiver<Msg>>, // a server thread is running
    backend_progress: String,
    restart_backend: bool, // the index changed while a server thread ran: run again when it ends
    pub map: Map,
    base: Option<Map>,                       // the map at the parent revision, for the diff
    base_rx: Option<Receiver<Option<Map>>>, // jj is being asked for it
    map_path: PathBuf,
    map_mtime: Option<SystemTime>,
    dirty: bool,
    last_poll: Instant,
    warned_disk: bool,

    // the selection
    pub focus: Option<SymRef>,     // the selected symbol
    pub sel_path: Option<usize>,   // the path being read: outline expanded, document shown
    pub sel_anchor: Option<usize>, // the selected step of it, when the selection came through the path
    pub cur_file: Option<usize>,
    pub sel: Option<(usize, usize)>,     // (anchor line, active line) of the line selection
    scroll_to: Option<usize>,            // the listing scrolls this line into view next frame
    scroll_to_step: Option<(usize, u8)>, // the document scrolls this step's header to the top; tries left
    top_step: Option<usize>,             // the step whose header is topmost in the document viewport

    expanded_steps: HashSet<(usize, usize)>, // (path, step) showing the whole enclosing symbol
    collapsed: HashSet<(usize, usize)>,      // (path, step) with its code hidden
    folded: HashSet<(usize, usize)>,         // (path, step) with its subtree hidden
    dir_toggled: HashSet<String>,            // directories in the Files tab whose default open state is flipped
    peek: Option<SymRef>,                    // a definition pinned in the right panel
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

    search: Field,
    new_path: Field,
    cmd: Field,
    sym_filter: Field,
    goto_line: Field,
    results: Vec<(usize, usize)>, // (file, line)
    output: String,
    status: String,
    shot: Option<(PathBuf, u32)>, // screenshot mode: write the window to this file after a few frames, then quit
}

impl App {
    pub fn new(root: &Path) -> App {
        let idx = index::build(root);
        let map_path = root.join(".codemap");
        let map = Map::load(&map_path);
        let mut status = format!("{} files, {} symbols indexed", idx.files.len(), idx.files.iter().map(|f| f.symbols.len()).sum::<usize>());
        if map.is_none() && map_path.exists() {
            status = ".codemap is unreadable or an old format: starting from an empty map, saving overwrites it".into();
        }
        let mut map = map.unwrap_or_default();
        map.resolve_all(&idx);
        let first_path = if map.paths.is_empty() { None } else { Some(0) };
        let mut app = App {
            idx,
            backend: None,
            backend_progress: String::new(),
            restart_backend: false,
            map,
            base: None,
            base_rx: None,
            map_mtime: mtime(&map_path),
            map_path,
            dirty: false,
            last_poll: Instant::now(),
            warned_disk: false,
            focus: None,
            sel_path: None,
            sel_anchor: None,
            cur_file: None,
            sel: None,
            scroll_to: None,
            scroll_to_step: None,
            top_step: None,
            expanded_steps: HashSet::new(),
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
            search: Field::default(),
            new_path: Field::default(),
            cmd: Field { focused: true, ..Default::default() },
            sym_filter: Field::default(),
            goto_line: Field::default(),
            results: Vec::new(),
            output: "type 'help' for commands; roots and promote live here\n".into(),
            status,
            shot: std::env::var_os("CODEMAP_SHOT").map(|p| (PathBuf::from(p), 0)),
        };
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
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(Map::base_from_vcs(&root));
        });
        self.base_rx = Some(rx);
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

    /// Ask every pending language's server on a thread; answers arrive through `poll_backend`.
    fn start_backend(&mut self) {
        if self.backend.is_some() {
            self.restart_backend = true;
            return;
        }
        let pending = self.idx.pending();
        if pending.is_empty() {
            return;
        }
        self.backend_progress = pending.iter().map(|(l, f)| format!("{}: 0/{}", l.server, f.len())).collect::<Vec<_>>().join("  ");
        let root = self.idx.root.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for (lang, files) in pending {
                let n = files.len();
                let mut done = 0;
                let run = index::query_server(&root, lang, &files, |f| {
                    done += 1;
                    let _ = tx.send(Msg::Progress(format!("{}: {done}/{n}", lang.server)));
                    let _ = tx.send(Msg::File(f));
                });
                if let Err(e) = run {
                    let _ = tx.send(Msg::Failed(lang, e));
                }
            }
            let _ = tx.send(Msg::Done);
        });
        self.backend = Some(rx);
    }

    /// Merge whatever the server thread has answered since the last frame.
    fn poll_backend(&mut self) {
        let Some(rx) = self.backend.take() else { return };
        let mut files = Vec::new();
        let mut done = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::File(f) => files.push(f),
                Msg::Progress(p) => self.backend_progress = p,
                Msg::Failed(lang, e) => {
                    self.idx.give_up(lang);
                    self.status = format!("{e}: {} files keep the tree-sitter resolver", lang.server);
                }
                Msg::Done => done = true,
            }
        }
        if !files.is_empty() {
            self.with_index_change(|app| {
                for f in files {
                    app.idx.apply(f);
                }
                app.idx.link();
            });
        }
        if done {
            self.idx.save_cache();
            self.backend_progress.clear();
            if std::mem::take(&mut self.restart_backend) {
                self.start_backend();
            }
        } else {
            self.backend = Some(rx);
        }
    }

    /// Once a second: pick up a map written by the CLI, and re-index when a source file changed.
    fn poll_disk(&mut self) {
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
                self.map = Map::load(&self.map_path).unwrap_or_default();
                self.map.resolve_all(&self.idx);
                self.map_mtime = mt;
                self.sel_anchor = None;
                self.expanded_steps.clear();
                self.collapsed.clear();
                self.folded.clear();
                if self.sel_path.is_some_and(|pi| pi >= self.map.paths.len()) {
                    self.sel_path = None;
                }
                self.load_base();
                self.status = "map reloaded (changed on disk)".into();
            }
        }
        if self.idx.changed() {
            self.reindex();
        }
    }

    /// Run `change` on the index and carry the selection, the listing and the map's anchors
    /// over by (file, symbol) identity, since symbol indices do not survive it.
    fn with_index_change(&mut self, change: impl FnOnce(&mut App)) {
        let focus = self.focus.map(|r| self.idx.key(r));
        let cur_file = self.cur_file.map(|fi| self.idx.files[fi].path.clone());
        let peek = self.peek.map(|r| self.idx.key(r));
        change(self);
        self.peek = peek.and_then(|k| self.idx.by_key(&k));
        self.map.resolve_all(&self.idx);
        self.focus = focus.and_then(|k| self.idx.by_key(&k));
        self.cur_file = cur_file.and_then(|p| self.idx.find_file(&p));
    }

    /// Rebuild the index from disk and the cache, then ask the servers about what changed.
    fn reindex(&mut self) {
        self.with_index_change(|app| app.idx = index::build(&app.idx.root));
        self.results.clear();
        self.status = format!("re-indexed: {} files, {} symbols (source changed)", self.idx.files.len(), self.idx.files.iter().map(|f| f.symbols.len()).sum::<usize>());
        self.start_backend();
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

    /// Restore a place without it counting as a navigation.
    fn go(&mut self, loc: Loc) {
        self.tab = loc.tab;
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
        self.last_loc = Some(self.here());
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
        let a = &self.map.paths[pi].anchors[ai];
        let (file, sym, ls, le) = (a.file.clone(), a.sym, a.line_start, a.line_end);
        match (self.idx.find_file(&file), sym) {
            (Some(fi), Some(si)) => {
                self.focus(SymRef { file: fi, sym: si });
                self.sel = Some((ls, le));
                self.scroll_to = Some(ls);
            }
            (Some(fi), None) => {
                self.cur_file = Some(fi);
                self.sel = Some((ls, le));
                self.scroll_to = Some(ls);
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
        if self.sel_anchor.is_none() && !matches!(self.tab, Tab::Graph | Tab::Path) {
            self.tab = Tab::Listing;
        }
    }

    fn open_line(&mut self, file: usize, line: usize) {
        self.cur_file = Some(file);
        self.sel = Some((line, line));
        self.scroll_to = Some(line);
        self.tab = Tab::Listing;
    }

    /// The identifier at `col` of a line.
    fn word_at(&self, fi: usize, li: usize, col: usize) -> Option<String> {
        let chars: Vec<char> = self.idx.files[fi].lines.get(li)?.chars().collect();
        let is_id = |c: &char| c.is_alphanumeric() || *c == '_';
        if !chars.get(col).is_some_and(is_id) {
            return None;
        }
        let start = (0..col).rev().take_while(|&i| is_id(&chars[i])).last().unwrap_or(col);
        let end = (col..chars.len()).take_while(|&i| is_id(&chars[i])).last().unwrap_or(col);
        Some(chars[start..=end].iter().collect())
    }

    /// The symbol the identifier at `col` names, if it is defined here: a definition that lists
    /// this line among its references wins, then one in the same file, then any.
    pub fn symbol_at(&self, fi: usize, li: usize, col: usize) -> Option<SymRef> {
        let word = self.word_at(fi, li, col)?;
        let cands = self.idx.find_symbols(&word);
        let here = (self.idx.files[fi].path.clone(), li as u32);
        cands.iter().copied().find(|r| self.idx.sym(*r).refs.contains(&here)).or_else(|| cands.iter().copied().find(|r| r.file == fi)).or_else(|| cands.first().copied())
    }

    fn jump_to(&mut self, fi: usize, li: usize, col: usize) {
        match self.symbol_at(fi, li, col) {
            Some(r) => self.select_symbol(r),
            None => self.status = format!("no definition of '{}' in this repo", self.word_at(fi, li, col).unwrap_or_default()),
        }
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
        self.output.push_str(&format!("> {line}\n"));
        let cmd = match cli::parse(&cli::tokenize(&line)) {
            Ok(cmd) => cmd,
            Err(e) => {
                self.output.push_str(&e.to_string());
                return;
            }
        };
        match cli::exec(&self.idx, &mut self.map, cmd, Author::Human, &mut self.output) {
            Ok(true) => {
                self.dirty = true;
                self.output.push_str("(map changed, ctrl+s to save)\n");
            }
            Ok(false) => {}
            Err(e) => self.output.push_str(&format!("error: {e}\n")),
        }
        self.scrolls.insert(ui::id("output"), i32::MAX);
    }

    fn run_search(&mut self) {
        self.results.clear();
        let re = match regex::Regex::new(&self.search.text) {
            Ok(re) => re,
            Err(e) => {
                self.status = format!("bad regex: {e}");
                return;
            }
        };
        // ponytail: single-threaded scan of the in-memory index; rayon it when it takes >100ms.
        'outer: for (fi, f) in self.idx.files.iter().enumerate() {
            for (li, line) in f.lines.iter().enumerate() {
                if re.is_match(line) {
                    self.results.push((fi, li));
                    if self.results.len() >= 5000 {
                        break 'outer;
                    }
                }
            }
        }
        self.status = format!("{} hits for /{}/", self.results.len(), self.search.text);
        self.tab = Tab::Results;
    }

    fn create_path(&mut self) {
        let name = self.new_path.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        self.sel_path = Some(self.map.add_path(&name, PathKind::Flow, Author::Human));
        self.sel_anchor = None;
        self.new_path.text.clear();
        self.new_path.cursor = 0;
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
            Action::SelectPath(pi) => {
                self.select_path(pi);
                self.tab = Tab::Path;
            }
            Action::ShowPath(pi) => {
                if self.sel_path != Some(pi) {
                    self.select_path(pi);
                }
                self.tab = Tab::Graph;
            }
            Action::SelectStep(pi, ai, in_doc) => self.select_step(pi, ai, in_doc),
            Action::ToggleStep(pi, ai) => {
                if !self.expanded_steps.remove(&(pi, ai)) {
                    self.expanded_steps.insert((pi, ai));
                }
            }
            Action::ToggleCode(pi, ai) => {
                if !self.collapsed.remove(&(pi, ai)) {
                    self.collapsed.insert((pi, ai));
                }
            }
            Action::ToggleFold(pi, ai) => {
                if !self.folded.remove(&(pi, ai)) {
                    self.folded.insert((pi, ai));
                }
            }
            Action::CollapseAll(pi, hide) => {
                self.collapsed.retain(|&(p, _)| p != pi);
                if hide {
                    self.collapsed.extend((0..self.map.paths[pi].anchors.len()).map(|ai| (pi, ai)));
                }
            }
            // ponytail: no undo
            Action::DeleteStep(pi, ai) => {
                self.map.remove_anchor(pi, ai);
                self.sel_anchor = None;
                self.expanded_steps.clear();
                self.collapsed.clear();
                self.folded.clear();
                self.dirty = true;
            }
            Action::DeletePath(pi) => {
                self.map.paths.remove(pi);
                self.sel_path = None;
                self.sel_anchor = None;
                self.expanded_steps.clear();
                self.collapsed.clear();
                self.folded.clear();
                self.dirty = true;
            }
            Action::GoTo(fi, line) => self.open_line(fi, line),
            Action::Jump(fi, line, col) => {
                self.jump_to(fi, line, col);
                if self.tab != Tab::Graph {
                    self.tab = Tab::Listing;
                }
            }
            Action::SelectLine(li, extend) => {
                self.sel = match (extend, self.sel) {
                    (true, Some((a, _))) => Some((a, li)),
                    _ => Some((li, li)),
                };
            }
            Action::Peek(r) => self.peek = Some(r),
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
            Action::NewPath => self.create_path(),
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
        let it = self.ui.interaction_of(id);
        self.ui.leaf(text(s, self.px, if it.hovered { TEXT } else { WEAK }), Layout::row().pad(2), Style::bg(if it.hovered { HOVER } else { FIELD }).border(ui::BORDER_ALL, BORDER), Some(id))
    }

    /// A selectable row: a full-width line of text with a hover and selection background.
    fn row(&mut self, runs_: Vec<(String, Color)>, id: Id, selected: bool) -> Interaction {
        let it = self.ui.interaction_of(id);
        let bg = if selected { Some(SELECTED) } else if it.hovered { Some(HOVER) } else { None };
        self.ui.leaf(runs(runs_, self.px), Layout::row().grow_x().pad(2), Style { bg, ..Default::default() }, Some(id))
    }

    /// A text field; clicking it takes the keyboard.
    fn field(&mut self, which: Which, hint: &str, width: i32) -> Interaction {
        let id = ui::id_with(ui::id("field"), hint);
        let f = self.field_mut(which);
        let focused = f.focused;
        let r = f.runs(hint);
        let it = self.ui.leaf(runs(r, self.px), Layout::row().w(width).pad(3), Style::bg(FIELD).border(ui::BORDER_ALL, if focused { ACCENT } else { BORDER }), Some(id));
        if it.clicked {
            self.take_focus(which);
        }
        it
    }

    fn field_mut(&mut self, which: Which) -> &mut Field {
        match which {
            Which::Search => &mut self.search,
            Which::NewPath => &mut self.new_path,
            Which::Cmd => &mut self.cmd,
            Which::SymFilter => &mut self.sym_filter,
            Which::GotoLine => &mut self.goto_line,
        }
    }

    fn take_focus(&mut self, which: Which) {
        for w in [Which::Search, Which::NewPath, Which::Cmd, Which::SymFilter, Which::GotoLine] {
            self.field_mut(w).focused = w == which;
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

    /// A code row: a syntax-coloured line with hover, double-click and ctrl-click to jump,
    /// alt-click to peek.
    pub fn code_row(&mut self, fi: usize, li: usize, id: Id, bg: Option<Color>) -> Interaction {
        let px = self.px;
        let r = code_runs(&self.idx.files[fi], li, true);
        let it = self.ui.leaf(runs(r, px), Layout::row().grow_x(), Style { bg, ..Default::default() }, Some(id));
        if it.hovered {
            if let Some(rect) = it.rect {
                let col = (((self.ui.input.mouse.0 - rect.x) / self.cell.0.max(1)) as usize).saturating_sub(6);
                let mods = self.ui.input.mods;
                if it.clicked && mods.alt {
                    if let Some(r) = self.symbol_at(fi, li, col) {
                        self.actions.push(Action::Peek(r));
                    }
                } else if it.double_clicked || (it.clicked && mods.ctrl) {
                    self.actions.push(Action::Jump(fi, li, col));
                }
            }
        }
        it
    }

    // ---- panels ----

    fn top_bar(&mut self) {
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(6).cross(Align::Center), Style::bg(PANEL).border(BORDER_BOTTOM, BORDER), None);
        self.label("search", WEAK);
        self.field(Which::Search, "regex", 24 * self.cell.0);
        if self.button("<", ui::id("back"), false).clicked {
            self.actions.push(Action::Back);
        }
        if self.button(">", ui::id("forward"), false).clicked {
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
        let p = self.map_path.display().to_string();
        self.label(&p, WEAK);
        let s = self.status.clone();
        self.label(&s, TEXT);
        if !self.backend_progress.is_empty() {
            let b = self.backend_progress.clone();
            self.label(&b, WEAK);
        }
        self.ui.close();
    }

    fn output_panel(&mut self, h: i32) {
        let px = self.px;
        self.ui.open(Kind::None, Layout::col().grow_x().h(h), Style::bg(FIELD).border(BORDER_TOP, BORDER), None);
        self.scroll_open(ui::id("output"), Layout::col().grow().pad(4), Style::default());
        for line in self.output.lines() {
            self.ui.leaf(text(line, px, TEXT), Layout::row(), Style::default(), None);
        }
        self.ui.close();
        let focus = self.cmd.focused;
        let it = self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(4).cross(Align::Center), Style::bg(if focus { PANEL } else { FIELD }).border(BORDER_TOP, BORDER), Some(ui::id("cmd")));
        if it.clicked {
            self.take_focus(Which::Cmd);
        }
        self.ui.leaf(text(">", px, WEAK), Layout::row(), Style::default(), None);
        let r = self.cmd.runs("");
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
        self.scroll_open(base, Layout::col().grow().pad(4), Style::default());
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
            let it = self.row(vec![(format!("{mark}{name} [{kind}]{tag}  {n} steps"), color)], ui::id_n(base, pi), selected);
            if it.clicked {
                self.actions.push(Action::SelectPath(pi));
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
                let it = self.row(vec![(line, if stale { RED } else if at_top { TEXT } else { WEAK }), (format!("  {file}"), dim(WEAK, 140))], ui::id_n(ui::id("outline"), ai), at_top || self.sel_anchor == Some(ai));
                if it.clicked {
                    self.actions.push(Action::SelectStep(pi, ai, false));
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
        let filter = self.sym_filter.text.to_lowercase();
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
        // virtual rows: only the visible window is built
        let row_h = self.cell.1 + 4;
        let off = self.scrolls.get(&id).copied().unwrap_or(0);
        let visible = it.rect.map_or(40, |r| r.h / row_h + 2) as usize;
        let first = (off / row_h).max(0) as usize;
        self.ui.leaf(Kind::None, Layout::row().h(first as i32 * row_h), Style::default(), None);
        for &r in rows.iter().skip(first).take(visible) {
            let s = self.idx.sym(r);
            let covered = if self.map.covers(&self.idx.files[r.file].path, s.start, s.end) { "+" } else { " " };
            let line = format!("{covered} {}{:<26} {:<12} {}:{}", if s.depth > 0 { "  " } else { "" }, trunc(&s.name, 26), trunc(&s.kind, 12), self.idx.files[r.file].path, s.start + 1);
            let pending = self.idx.files[r.file].pending;
            let it = self.row(vec![(line, if pending { dim(WEAK, 120) } else { TEXT })], ui::id_n(ui::id("sym"), r.file * 100_000 + r.sym), self.focus == Some(r));
            if it.clicked {
                self.actions.push(Action::Focus(r));
            }
        }
        let rest = rows.len().saturating_sub(first + visible) as i32 * row_h;
        self.ui.leaf(Kind::None, Layout::row().h(rest), Style::default(), None);
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
                let sub: Vec<usize> = files[i..j].to_vec();
                self.files_tree(&sub, depth + 1, cov);
            }
            i = j;
        }
    }

    fn xrefs_panel(&mut self, w: i32) {
        self.ui.open(Kind::None, Layout::col().w(w).grow_y(), Style::bg(PANEL).border(BORDER_LEFT, BORDER), None);
        if let Some(r) = self.peek.filter(|r| r.file < self.idx.files.len() && r.sym < self.idx.files[r.file].symbols.len()) {
            let s = self.idx.sym(r);
            let (name, place, start, end) = (s.name.clone(), format!("{}:{}", self.idx.files[r.file].path, s.start + 1), s.start, s.end);
            self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(6).cross(Align::Center), Style::default(), None);
            self.label(&format!("Peek: {name}"), TEXT);
            self.label(&place, WEAK);
            self.ui.leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
            if self.small_button("go", ui::id("peek-go")).clicked {
                self.actions.push(Action::Focus(r));
            }
            if self.small_button("x", ui::id("peek-x")).clicked {
                self.actions.push(Action::ClosePeek);
            }
            self.ui.close();
            let h = (self.ui.size.1 / 3).max(100);
            self.scroll_open(ui::id("peek"), Layout::col().grow_x().h(h).pad(4), Style::bg(FIELD));
            let end = end.min(self.idx.files[r.file].lines.len().saturating_sub(1));
            for li in start..=end {
                self.code_row(r.file, li, ui::id_n(ui::id("peekrow"), li), None);
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
        self.label(&format!("Xrefs to ({})", callers.len()), WEAK);
        for (i, r) in callers.iter().enumerate() {
            let it = self.row(vec![(cli::describe(&self.idx, *r), dimmed)], ui::id_n(ui::id("xto"), i), false);
            if it.clicked && !pending {
                self.actions.push(Action::Focus(*r));
            }
        }
        self.label(&format!("Xrefs from ({})", callees.len()), WEAK);
        for (i, r) in callees.iter().enumerate() {
            let it = self.row(vec![(cli::describe(&self.idx, *r), dimmed)], ui::id_n(ui::id("xfrom"), i), false);
            if it.clicked && !pending {
                self.actions.push(Action::Focus(*r));
            }
        }
        self.label(&format!("References ({})", refs.len()), WEAK);
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
            self.actions.push(Action::ShowPath(pi));
        }
        if self.small_button("collapse all", ui::id("doc-collapse")).clicked {
            self.actions.push(Action::CollapseAll(pi, true));
        }
        if self.small_button("expand all", ui::id("doc-expand")).clicked {
            self.actions.push(Action::CollapseAll(pi, false));
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
            if gone.is_none() && self.small_button(if collapsed { "code" } else { "hide code" }, ui::id_n(ui::id("hide"), ai)).clicked {
                self.actions.push(Action::ToggleCode(pi, ai));
            }
            if sym.is_some_and(|s| s != (ls, le)) {
                let whole = self.expanded_steps.contains(&(pi, ai));
                if self.small_button(if whole { "slice" } else { "whole symbol" }, ui::id_n(ui::id("whole"), ai)).clicked {
                    self.actions.push(Action::ToggleStep(pi, ai));
                }
            }
            if self.small_button("delete", ui::id_n(ui::id("del"), ai)).clicked {
                self.actions.push(Action::DeleteStep(pi, ai));
            }
            self.ui.close();
            // note
            self.ui.open(Kind::None, Layout::row().grow_x(), Style::default(), None);
            self.ui.leaf(Kind::None, Layout::row().w(indent + 4 + 3 * self.cell.0), Style::default(), None);
            self.ui.leaf(wrapped(if anote.is_empty() { "(no note)" } else { &anote }, px, if anote.is_empty() { dim(WEAK, 120) } else { GREEN }), Layout::row().grow_x().pad(2), Style::default(), None);
            self.ui.close();
            // code
            if let (Some(fi), None, false) = (fi, gone, collapsed) {
                let whole = self.expanded_steps.contains(&(pi, ai));
                let (lo, hi) = if whole { sym.unwrap_or((ls, le)) } else { (ls, le) };
                let hi = hi.min(self.idx.files[fi].lines.len().saturating_sub(1));
                self.ui.open(Kind::None, Layout::col().grow_x(), Style { bg: None, border: if selected { BORDER_LEFT } else { 0 }, border_color: ACCENT }, None);
                for li in lo..=hi {
                    let bg = if whole && li >= ls && li <= le { Some(dim(SELECTED, 120)) } else { None };
                    self.ui.open(Kind::None, Layout::row().grow_x(), Style::default(), None);
                    self.ui.leaf(Kind::None, Layout::row().w(indent + 4), Style::default(), None);
                    self.code_row(fi, li, ui::id_n(ui::id_n(ui::id("docrow"), ai), li), bg);
                    self.ui.close();
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
        let off = self.scrolls.get(&id).copied().unwrap_or(0);
        let visible = it.rect.map_or(60, |r| r.h / row_h + 2) as usize;
        let first = (off / row_h).max(0) as usize;
        let (lo, hi) = self.sel.map(|(a, b)| (a.min(b), a.max(b))).unwrap_or((usize::MAX, usize::MAX));
        let anchors: Vec<(usize, usize, bool)> = self.sel_path.map(|pi| self.map.paths[pi].anchors.iter().filter(|a| a.file == self.idx.files[fi].path).map(|a| (a.line_start, a.line_end, a.stale)).collect()).unwrap_or_default();
        self.ui.leaf(Kind::None, Layout::row().h(first as i32 * row_h), Style::default(), None);
        for li in first..(first + visible).min(n) {
            let selected = li >= lo && li <= hi;
            let bar = anchors.iter().find(|&&(s, e, _)| li >= s && li <= e).map(|&(_, _, stale)| if stale { RED } else { GREEN });
            self.ui.open(Kind::None, Layout::row().grow_x(), Style { bg: if selected { Some(dim(SELECTED, 160)) } else { None }, border: if bar.is_some() { BORDER_LEFT } else { 0 }, border_color: bar.unwrap_or(BORDER) }, None);
            let it = self.code_row(fi, li, ui::id_n(ui::id("line"), li), None);
            if it.clicked && !self.ui.input.mods.ctrl && !self.ui.input.mods.alt && !it.double_clicked {
                self.actions.push(Action::SelectLine(li, self.ui.input.mods.shift));
            }
            self.ui.close();
        }
        let rest = n.saturating_sub(first + visible) as i32 * row_h;
        self.ui.leaf(Kind::None, Layout::row().h(rest), Style::default(), None);
        self.ui.close();
    }

    fn results_view(&mut self) {
        let id = ui::id("results");
        let row_h = self.cell.1 + 4;
        let it = self.scroll_open(id, Layout::col().grow().pad(4), Style::default());
        let off = self.scrolls.get(&id).copied().unwrap_or(0);
        let visible = it.rect.map_or(60, |r| r.h / row_h + 2) as usize;
        let first = (off / row_h).max(0) as usize;
        self.ui.leaf(Kind::None, Layout::row().h(first as i32 * row_h), Style::default(), None);
        let rows: Vec<(usize, usize)> = self.results.iter().copied().skip(first).take(visible).collect();
        for (k, (fi, li)) in rows.into_iter().enumerate() {
            let f = &self.idx.files[fi];
            let line = f.lines[li].trim().to_owned();
            let it = self.row(vec![(format!("{}:{}: ", f.path, li + 1), WEAK), (line, TEXT)], ui::id_n(ui::id("hit"), first + k), false);
            if it.clicked {
                self.actions.push(Action::GoTo(fi, li));
            }
        }
        let rest = self.results.len().saturating_sub(first + visible) as i32 * row_h;
        self.ui.leaf(Kind::None, Layout::row().h(rest), Style::default(), None);
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
                if let Some(pi) = pi {
                    self.actions.push(Action::SelectPath(pi));
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

    fn graph_tab(&mut self, gfx: &mut Gfx) {
        let _ = gfx;
        self.label("the graph arrives in the next step", WEAK);
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

impl gfx::App for App {
    fn frame(&mut self, gfx: &mut Gfx, input: &mut ui::Input) -> gfx::Frame {
        self.px = (14.0 * gfx.scale).round().max(8.0) as u32;
        self.cell = gfx.cell(self.px);
        let mut quit = false;
        // Development aid: with CODEMAP_SHOT=<file.png> the window is written there once the
        // first frames have settled, and the app quits. CODEMAP_SHOT_TAB picks the centre tab.
        if let Some((path, frame)) = self.shot.as_mut() {
            *frame += 1;
            if *frame == 3 {
                self.tab = match std::env::var("CODEMAP_SHOT_TAB").as_deref() {
                    Ok("graph") => Tab::Graph,
                    Ok("listing") => Tab::Listing,
                    Ok("diff") => Tab::Diff,
                    _ => Tab::Path,
                };
            }
            if *frame == 20 {
                gfx.shot = Some(path.clone());
            }
            if *frame > 20 {
                quit = true;
            }
        }
        self.poll_backend();
        self.poll_base();
        self.poll_disk();

        // keys
        if input.key_with(Key::Char('s'), true, false) {
            self.save();
        }
        if input.key_with(Key::Left, false, true) || input.back {
            self.back();
        }
        if input.key_with(Key::Right, false, true) || input.forward {
            self.forward();
        }
        if let Some(line) = self.cmd.handle(input, false) {
            self.run_cmd(line);
        }
        if self.search.handle(input, true).is_some() {
            self.run_search();
        }
        if self.new_path.handle(input, false).is_some() {
            self.create_path();
        }
        self.sym_filter.handle(input, true);
        if let Some(l) = self.goto_line.handle(input, true) {
            if let (Ok(line), Some(fi)) = (l.trim().parse::<usize>(), self.cur_file) {
                let line = line.clamp(1, self.idx.files[fi].lines.len().max(1)) - 1;
                self.sel = Some((line, line));
                self.scroll_to = Some(line);
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
        self.ui.end(gfx);
        self.ui.draw(gfx, TEXT);

        for a in std::mem::take(&mut self.actions) {
            self.apply(a);
        }
        self.track_navigation();
        let busy = self.backend.is_some() || self.base_rx.is_some() || self.shot.is_some();
        gfx::Frame { redraw_after: Some(if busy { Duration::from_millis(50) } else { Duration::from_millis(1000) }), quit, clear: BG }
    }
}
