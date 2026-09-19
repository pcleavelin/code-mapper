//! The human's surface, built from `ui` elements and drawn by `gfx`. One selection drives every
//! view; the panels are functions that open and close elements each frame.

use crate::gfx::{self, Color, Gfx};
use crate::index::{self, Index};
use crate::map::{Author, Map};
use crate::ui::{self, Align, Interaction, Key, Kind, Layout, Style, Text, Ui, BORDER_BOTTOM, BORDER_LEFT, BORDER_RIGHT, BORDER_TOP};
use crate::cli;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

fn dim(c: Color, a: u8) -> Color {
    [c[0], c[1], c[2], a]
}

// ---- widgets ----

fn text(s: &str, px: u32, color: Color) -> Kind {
    Kind::Text(Text { runs: vec![(s.to_owned(), color)], px, wrap: false })
}

fn wrapped(s: &str, px: u32, color: Color) -> Kind {
    Kind::Text(Text { runs: vec![(s.to_owned(), color)], px, wrap: true })
}

fn runs(runs: Vec<(String, Color)>, px: u32) -> Kind {
    Kind::Text(Text { runs, px, wrap: false })
}

/// The tab of the centre panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
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

/// A single-line text field: the command line.
#[derive(Default)]
struct Field {
    text: String,
    cursor: usize, // in chars
    focused: bool,
    history: Vec<String>,
    hist_at: Option<usize>,
}

impl Field {
    /// Feed the frame's keys; returns the line when enter was pressed.
    fn handle(&mut self, input: &ui::Input) -> Option<String> {
        if !self.focused {
            return None;
        }
        let mut submitted = None;
        for (k, m) in &input.keys {
            match k {
                Key::Enter => {
                    let line = std::mem::take(&mut self.text);
                    self.cursor = 0;
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
}

pub struct App {
    idx: Index,
    map: Map,
    map_path: PathBuf,
    dirty: bool,

    ui: Ui,
    px: u32, // the UI font size in pixels
    tab: Tab,
    left: LeftTab,

    cmd: Field,
    output: String,
    output_scroll: i32,
    status: String,
    last_frame: Instant,
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
        App {
            idx,
            map,
            map_path,
            dirty: false,
            ui: Ui::default(),
            px: 14,
            tab: Tab::Path,
            left: LeftTab::Paths,
            cmd: Field { focused: true, ..Default::default() },
            output: "type 'help' for commands; roots and promote live here\n".into(),
            output_scroll: i32::MAX,
            status,
            last_frame: Instant::now(),
            shot: std::env::var_os("CODEMAP_SHOT").map(|p| (PathBuf::from(p), 0)),
        }
    }

    fn save(&mut self) {
        match self.map.save(&self.map_path) {
            Ok(()) => {
                self.dirty = false;
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
        self.output_scroll = i32::MAX;
    }

    // ---- widgets that need the app's font size ----

    fn label(&mut self, s: &str, color: Color) {
        self.ui.leaf(text(s, self.px, color), Layout::row().pad(2), Style::default(), None);
    }

    fn button(&mut self, s: &str, id: ui::Id, selected: bool) -> Interaction {
        let it = self.ui.interaction_of(id);
        let bg = if selected { SELECTED } else if it.hovered { HOVER } else { PANEL };
        self.ui.leaf(text(s, self.px, if selected { TEXT } else { WEAK }), Layout::row().pad(4), Style::bg(bg).border(ui::BORDER_ALL, BORDER), Some(id))
    }

    fn top_bar(&mut self) {
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(6).cross(Align::Center), Style::bg(PANEL).border(BORDER_BOTTOM, BORDER), None);
        for (t, name) in [(Tab::Path, "Path"), (Tab::Diff, "Diff"), (Tab::Graph, "Graph"), (Tab::Listing, "Listing"), (Tab::Results, "Results")] {
            if self.button(name, ui::id_with(ui::id("tab"), name), self.tab == t).clicked {
                self.tab = t;
            }
        }
        self.ui.leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
        let save = if self.dirty { "save *" } else { "save" };
        if self.button(save, ui::id("save"), false).clicked {
            self.save();
        }
        self.ui.close();
    }

    fn status_bar(&mut self) {
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(12).cross(Align::Center), Style::bg(PANEL).border(BORDER_TOP, BORDER), None);
        let p = self.map_path.display().to_string();
        self.label(&p, WEAK);
        let s = self.status.clone();
        self.label(&s, TEXT);
        self.ui.close();
    }

    fn output_panel(&mut self, h: i32) {
        let px = self.px;
        let id = ui::id("output");
        let scroll = self.ui.scroll_by_wheel(id, &mut self.output_scroll);
        self.ui.open(Kind::None, Layout::col().grow_x().h(h), Style::bg(FIELD).border(BORDER_TOP, BORDER), None);
        // the log, scrolled
        self.ui.open(Kind::None, Layout::col().grow().pad(4).scroll(0, scroll), Style::default(), Some(id));
        for line in self.output.lines() {
            self.ui.leaf(text(line, px, TEXT), Layout::row(), Style::default(), None);
        }
        self.ui.close();
        // the command line
        let focus = self.cmd.focused;
        let it = self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(4).cross(Align::Center), Style::bg(if focus { PANEL } else { FIELD }).border(BORDER_TOP, BORDER), Some(ui::id("cmd")));
        if it.clicked {
            self.cmd.focused = true;
        }
        self.ui.leaf(text(">", px, WEAK), Layout::row(), Style::default(), None);
        let (before, after) = {
            let i = self.cmd.byte_at(self.cmd.cursor);
            (self.cmd.text[..i].to_owned(), self.cmd.text[i..].to_owned())
        };
        let caret = if focus { "▏" } else { " " };
        self.ui.leaf(runs(vec![(before, TEXT), (caret.into(), ACCENT), (after, TEXT)], px), Layout::row().grow_x(), Style::default(), None);
        self.ui.close();
        self.ui.close();
    }
}

impl gfx::App for App {
    fn frame(&mut self, gfx: &mut Gfx, input: &mut ui::Input) -> gfx::Frame {
        self.px = (14.0 * gfx.scale).round().max(8.0) as u32;
        let mut quit = false;
        // Development aid: with CODEMAP_SHOT=<file.png> the window is written there once the
        // first frames have settled, and the app quits.
        if let Some((path, frame)) = self.shot.as_mut() {
            *frame += 1;
            if *frame == 20 {
                gfx.shot = Some(path.clone());
            }
            if *frame > 20 {
                quit = true;
            }
        }
        if input.key_with(Key::Char('s'), true, false) {
            self.save();
        }
        if input.key_with(Key::Char('q'), true, false) {
            quit = true;
        }
        if let Some(line) = self.cmd.handle(input) {
            self.run_cmd(line);
        }

        self.ui.begin(input);
        self.ui.open(Kind::None, Layout::col().grow(), Style::bg(BG), None);
        self.top_bar();
        self.ui.open(Kind::None, Layout::row().grow(), Style::default(), None);
        // left panel
        self.ui.open(Kind::None, Layout::col().w((320.0 * gfx.scale) as i32).grow_y().pad(4), Style::bg(PANEL).border(BORDER_RIGHT, BORDER), None);
        self.label("Paths", TEXT);
        let n = self.map.paths.len();
        for pi in 0..n {
            let name = self.map.paths[pi].name.clone();
            self.label(&name, WEAK);
        }
        self.ui.close();
        // centre
        self.ui.open(Kind::None, Layout::col().grow().pad(8), Style::default(), None);
        self.label("(the document goes here)", WEAK);
        self.ui.close();
        // right panel
        self.ui.open(Kind::None, Layout::col().w((260.0 * gfx.scale) as i32).grow_y().pad(4), Style::bg(PANEL).border(BORDER_LEFT, BORDER), None);
        self.label("Xrefs", TEXT);
        self.ui.close();
        self.ui.close();
        let out_h = (180.0 * gfx.scale) as i32;
        self.output_panel(out_h);
        self.status_bar();
        self.ui.close();
        self.ui.end(gfx);
        self.ui.draw(gfx, TEXT);
        self.last_frame = Instant::now();
        gfx::Frame { redraw_after: Some(if self.shot.is_some() { Duration::from_millis(16) } else { Duration::from_millis(500) }), quit, clear: BG }
    }
}
