//! The theme and the handful of widgets the GUI is written in: text builders, the code grid,
//! the single-line field, buttons, rows, scrolling columns and the code block every view
//! draws code with.

use super::*;

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

pub(super) fn text(s: &str, px: u32, color: Color) -> Kind {
    Kind::Text(Text {
        runs: vec![(s.to_owned(), color)],
        px,
        wrap: false,
    })
}

pub(super) fn wrapped(s: &str, px: u32, color: Color) -> Kind {
    Kind::Text(Text {
        runs: vec![(s.to_owned(), color)],
        px,
        wrap: true,
    })
}

pub(super) fn runs(runs: Vec<(String, Color)>, px: u32) -> Kind {
    Kind::Text(Text {
        runs,
        px,
        wrap: false,
    })
}

/// Columns of line number and a space in front of every code line.
pub const GUTTER: usize = 6;

/// Lines `lo..=hi` of a file as a grid: the line number in the gutter, then the line with
/// its syntax colours, one cell per character.
pub fn build_grid(f: &index::File, lo: usize, hi: usize) -> Glyphs {
    let w = (lo..=hi)
        .map(|li| f.lines[li].chars().count())
        .max()
        .unwrap_or(0)
        + GUTTER;
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

pub(super) fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        s.chars()
            .take(n.saturating_sub(1))
            .chain(std::iter::once('…'))
            .collect()
    }
}

/// Cuts from the front, keeping the tail: what a path needs, since the file name is at the end.
pub(super) fn trunc_left(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        s.to_owned()
    } else {
        std::iter::once('…')
            .chain(s.chars().skip(count + 1 - n.max(1)))
            .collect()
    }
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
    pub(super) fn handle(&mut self, input: &ui::Input, keep_on_enter: bool) -> Option<String> {
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

    pub(super) fn byte_at(&self, ch: usize) -> usize {
        self.text
            .char_indices()
            .nth(ch)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    /// The text with the caret, as runs.
    pub(super) fn runs(&self, hint: &str) -> Vec<(String, Color)> {
        if self.text.is_empty() && !self.focused {
            return vec![(hint.to_owned(), WEAK)];
        }
        let i = self.byte_at(self.cursor);
        let color = if self.selected && self.focused {
            ACCENT
        } else {
            TEXT
        };
        vec![
            (self.text[..i].to_owned(), color),
            (if self.focused { "▏" } else { "" }.to_owned(), ACCENT),
            (self.text[i..].to_owned(), color),
        ]
    }
}

impl App {
    pub fn label(&mut self, s: &str, color: Color) {
        self.ui.leaf(
            text(s, self.px, color),
            Layout::row().pad(2),
            Style::default(),
            None,
        );
    }

    pub fn button(&mut self, s: &str, id: Id, selected: bool) -> Interaction {
        let it = self.ui.interaction_of(id);
        let bg = if selected {
            SELECTED
        } else if it.hovered {
            HOVER
        } else {
            PANEL
        };
        self.ui.leaf(
            text(s, self.px, if selected || it.hovered { TEXT } else { WEAK }),
            Layout::row().pad(4),
            Style::bg(bg).border(ui::BORDER_ALL, BORDER),
            Some(id),
        )
    }

    pub fn small_button(&mut self, s: &str, id: Id) -> Interaction {
        self.small_button_w(s, 0, id)
    }

    /// A small button `cols` cells wide whatever its label, so a label that changes on click
    /// does not move the buttons beside it; 0 fits the label.
    pub fn small_button_w(&mut self, s: &str, cols: usize, id: Id) -> Interaction {
        let it = self.ui.interaction_of(id);
        let layout = if cols > 0 {
            Layout::row().pad(2).w(cols as i32 * self.cell.0 + 4)
        } else {
            Layout::row().pad(2)
        };
        self.ui.leaf(
            text(s, self.px, if it.hovered { TEXT } else { WEAK }),
            layout,
            Style::bg(if it.hovered { HOVER } else { FIELD }).border(ui::BORDER_ALL, BORDER),
            Some(id),
        )
    }

    /// A navigation button three cells wide, dimmed when it has nowhere to go.
    pub(super) fn nav_button(&mut self, s: &str, id: Id, enabled: bool) -> Interaction {
        let it = self.ui.interaction_of(id);
        let color = if !enabled {
            dim(WEAK, 90)
        } else if it.hovered {
            TEXT
        } else {
            WEAK
        };
        self.ui.leaf(
            text(s, self.px, color),
            Layout::row().pad(4).w(3 * self.cell.0 + 8),
            Style::bg(if it.hovered && enabled { HOVER } else { PANEL })
                .border(ui::BORDER_ALL, BORDER),
            Some(id),
        )
    }

    /// A selectable row: a full-width line of text with a hover and selection background.
    pub(super) fn row(
        &mut self,
        runs_: Vec<(String, Color)>,
        id: Id,
        selected: bool,
    ) -> Interaction {
        self.row_bg(runs_, id, selected.then_some(SELECTED))
    }

    /// A row with its own background when it is marked; hover shows otherwise.
    pub(super) fn row_bg(
        &mut self,
        runs_: Vec<(String, Color)>,
        id: Id,
        marked: Option<Color>,
    ) -> Interaction {
        let it = self.ui.interaction_of(id);
        let bg = marked.or(if it.hovered { Some(HOVER) } else { None });
        self.ui.leaf(
            runs(runs_, self.px),
            Layout::row().grow_x().pad(2),
            Style {
                bg,
                ..Default::default()
            },
            Some(id),
        )
    }

    /// A text field; clicking it takes the keyboard. The text is clipped to the box and
    /// scrolled so the caret stays in view.
    pub(super) fn field(&mut self, which: Which, hint: &str, width: i32) {
        let id = ui::id_with(ui::id("field"), hint);
        let f = &self.fields[which as usize];
        let focused = f.focused;
        let caret = f.cursor as i32 + 1;
        let r = f.runs(hint);
        let x = (caret * self.cell.0 - (width - 6)).max(0);
        let it = self.ui.open(
            Kind::None,
            Layout::row().w(width).pad(3).scroll(x, 0),
            Style::bg(FIELD).border(ui::BORDER_ALL, if focused { ACCENT } else { BORDER }),
            Some(id),
        );
        self.ui
            .leaf(runs(r, self.px), Layout::row(), Style::default(), None);
        self.ui.close();
        if it.clicked {
            self.take_focus(which);
        }
    }

    pub(super) fn take_focus(&mut self, which: Which) {
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
        self.ui
            .open(Kind::None, layout.scroll(0, off), style, Some(id))
    }

    /// A virtual list of `n` rows `row_h` high in the scrolling column `id`, whose rectangle is
    /// `rect`: the first row in view and how many fit (`guess` before the column has a size).
    /// Only those rows are built. This emits the spacer standing for the rows above; the caller
    /// ends the list with a `spacer` for the rows below.
    pub(super) fn rows_window(
        &mut self,
        id: Id,
        rect: Option<Rect>,
        n: usize,
        row_h: i32,
        guess: usize,
    ) -> (usize, usize) {
        let off = self.scrolls.get(&id).copied().unwrap_or(0);
        let visible = rect.map_or(guess, |r| (r.h / row_h + 2) as usize);
        let first = ((off / row_h).max(0) as usize).min(n);
        self.spacer(first as i32 * row_h);
        (first, visible)
    }

    /// Empty space `h` pixels high.
    pub(super) fn spacer(&mut self, h: i32) {
        self.ui
            .leaf(Kind::None, Layout::row().h(h), Style::default(), None);
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
    pub fn code_block(
        &mut self,
        fi: usize,
        lo: usize,
        hi: usize,
        id: Id,
        wide: bool,
        bg: &dyn Fn(usize) -> Option<Color>,
        bar: &dyn Fn(usize) -> Option<Color>,
    ) -> (Interaction, Option<(usize, Option<usize>)>) {
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
        let rows: Vec<(Option<Color>, Option<Color>)> =
            (lo..=hi).map(|li| (bg(li), bar(li))).collect();
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
        let layout = if wide {
            Layout::row().grow_x().h(h)
        } else {
            Layout::row().w(w).h(h)
        };
        let it = self.ui.leaf(
            Kind::Custom(Box::new(draw)),
            layout,
            Style::default(),
            Some(id),
        );
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
            } else if !self.ui.input.down[0]
                && let Some(t) = self.probe(fi, li, col)
            {
                self.tooltip = Some((t, self.ui.input.mouse));
            }
        }
        (it, at)
    }
}
