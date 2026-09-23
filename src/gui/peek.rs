//! What is known about the identifier under the pointer: the tooltip, go-to-definition and the
//! peek, answered by the language's server or by the index.

use super::*;

/// Why a definition was asked for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Intent {
    Jump,
    Peek,
}

/// Hover and definition requests to the servers and their answers.
#[derive(Default)]
pub(super) struct Lookup {
    pub(super) hovers: HashMap<Probe, Option<Option<String>>>, // asked (None) or answered (Some: text or nothing)
    pub(super) want: Option<(Probe, f64)>,                     // the position under the pointer and since when
    pub(super) inflight: bool,                                  // a hover request the server has not answered
    pub(super) asked: usize,                                    // hover and definition requests not yet answered
    pub(super) want_def: Option<(Probe, Intent)>,               // the definition lookup whose answer is awaited
    pub(super) refs_asked: HashSet<Probe>,                      // symbols whose references the server was asked for
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

/// The UTF-16 column of char index `col` in `line`, which is how servers count.
pub(super) fn utf16_col(line: &str, col: usize) -> u32 {
    line.chars().take(col).map(|c| c.len_utf16() as u32).sum()
}

impl App {
    /// The identifier at `col` of a line, as (first column, text).
    pub(super) fn word_at(&self, fi: usize, li: usize, col: usize) -> Option<(usize, String)> {
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
    pub(super) fn probe_for(&self, fi: usize, li: usize, col: usize) -> Result<(&'static index::Lang, Probe), bool> {
        let f = &self.idx.files[fi];
        match index::lang_for(&f.path).filter(|l| !self.work.no_server.contains(l.server)) {
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
            Ok((lang, p)) => match self.lookup.hovers.get(&p) {
                Some(Some(t)) => t.clone().map(Tip::Text),
                Some(None) => None,
                None => {
                    let since = match &self.lookup.want {
                        Some((w, t)) if *w == p => *t,
                        _ => {
                            self.lookup.want = Some((p.clone(), self.now));
                            self.now
                        }
                    };
                    if self.now - since >= 0.15 && !self.lookup.inflight {
                        if self.lookup.hovers.len() > 500 {
                            self.lookup.hovers.clear();
                        }
                        self.lookup.hovers.insert(p.clone(), None);
                        if let Some(tx) = self.server(lang) {
                            let _ = tx.send(Req::Hover(p));
                            self.lookup.asked += 1;
                            self.lookup.inflight = true;
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
    pub(super) fn probe_def(&mut self, fi: usize, li: usize, col: usize, intent: Intent) {
        let Some((start, word)) = self.word_at(fi, li, col) else { return };
        match self.probe_for(fi, li, start) {
            Ok((lang, p)) => {
                if let Some(tx) = self.server(lang) {
                    let _ = tx.send(Req::Def(p.clone()));
                    self.lookup.asked += 1;
                    self.lookup.want_def = Some((p, intent));
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
    pub(super) fn land(&mut self, found: Peek, intent: Intent) {
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
    pub(super) fn symbol_at(&self, fi: usize, li: usize, col: usize) -> Option<SymRef> {
        let (_, word) = self.word_at(fi, li, col)?;
        let cands = self.idx.find_symbols(&word);
        let here = (self.idx.files[fi].path.clone(), li as u32);
        cands.iter().copied().find(|r| self.idx.sym(*r).refs.contains(&here)).or_else(|| cands.iter().copied().find(|r| r.file == fi)).or_else(|| cands.first().copied())
    }
}

impl App {
    /// The floating tooltip under the pointer, drawn over everything: what the language's
    /// server says about the identifier, or the definition of the symbol of that name.
    pub fn tooltip_element(&mut self) {
        self.tip_shown = None;
        let Some((tip, (mx, my))) = self.tooltip.take() else { return };
        if self.ui.input.down[0] {
            return;
        }
        let px = self.px;
        let (w, h) = (self.ui.size.0, self.ui.size.1);
        let (cw, rh) = self.cell;
        let max_cols = ((w - 40) / cw.max(1)).max(20) as usize;
        // the box goes beside the pointer, flipping above or to the left when it would not fit
        let place_at = |cols: i32, rows: i32| {
            let (tw, th) = (cols * cw + 12, rows * (rh + 2) + 12);
            let x = if mx + 16 + tw <= w { mx + 16 } else { (mx - 16 - tw).max(0) };
            let y = if my + 16 + th <= h { my + 16 } else { (my - 16 - th).max(0) };
            (x, y)
        };
        match tip {
            Tip::Sym(r) if r.file < self.idx.files.len() => {
                let s = self.idx.sym(r);
                let (name, place, start, end) = (s.name.clone(), format!("{} {}:{}-{}", s.kind, self.idx.files[r.file].path, s.start + 1, s.end + 1), s.start, s.end);
                self.tip_shown = Some(name.clone());
                let f = &self.idx.files[r.file];
                let last = end.min(f.lines.len().saturating_sub(1)).min(start + 23);
                let widest = (start..=last).map(|li| f.lines[li].chars().count() + GUTTER).max().unwrap_or(0).max(place.len()).max(70) as i32;
                let (x, y) = place_at(widest, (last - start) as i32 + 5);
                self.ui.open(Kind::None, Layout::col().floating(x, y).pad(6).gap(2), Style::bg(PANEL).border(ui::BORDER_ALL, BORDER), None);
                self.label(&name, TEXT);
                self.label(&place, WEAK);
                self.code_block(r.file, start, last, ui::id("tip-code"), false, &|_| None, &|_| None);
                if last < end {
                    self.label(&format!("      … {} more lines", end - last), WEAK);
                }
                self.label("alt-click: pin in the peek panel   ctrl-click or double-click: go there", WEAK);
                self.ui.close();
            }
            Tip::Text(t) => {
                self.tip_shown = t.lines().next().map(str::to_owned);
                let lines: Vec<&str> = t.lines().collect();
                let widest = lines.iter().take(24).map(|l| l.chars().count()).max().unwrap_or(0).min(max_cols).max(84) as i32;
                let (x, y) = place_at(widest, lines.len().min(24) as i32 + 2);
                self.ui.open(Kind::None, Layout::col().floating(x, y).pad(6).gap(2), Style::bg(PANEL).border(ui::BORDER_ALL, BORDER), None);
                for l in lines.iter().take(24) {
                    let l: String = l.chars().take(max_cols).collect();
                    let weak = l.starts_with("---");
                    self.ui.leaf(Kind::Text(ui::Text { runs: vec![(if weak { "─".repeat(20) } else { l }, if weak { WEAK } else { TEXT })], px, wrap: false }), Layout::row(), Style::default(), None);
                }
                if lines.len() > 24 {
                    self.label(&format!("… {} more lines", lines.len() - 24), WEAK);
                }
                self.label("alt-click: pin the definition in the peek panel   ctrl-click or double-click: go to it", WEAK);
                self.ui.close();
            }
            _ => {}
        }
    }
}
