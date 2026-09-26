use crate::gfx::{Color, Gfx, Rect};
use std::collections::HashMap;

pub type Id = u64;

pub fn id(s: &str) -> Id {
    id_with(0xcbf29ce484222325, s)
}

pub fn id_with(base: Id, s: &str) -> Id {
    let mut h = base;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn id_n(base: Id, n: usize) -> Id {
    (base ^ (n as u64).wrapping_mul(0x9e3779b97f4a7c15)).wrapping_mul(0x100000001b3)
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Enter,
    Escape,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Char(char),
}

#[derive(Default)]
pub struct Input {
    pub mouse: (i32, i32),
    pub down: [bool; 3],
    pub pressed: [bool; 3],
    pub clicks: [u8; 3],
    pub wheel: (f32, f32),
    pub keys: Vec<(Key, Mods)>,
    pub text: String,
    pub mods: Mods,
    pub size: (i32, i32),
    pub time: f64,
    pub back: bool,
    pub forward: bool,
}

impl Input {
    pub fn end_frame(&mut self) {
        self.pressed = [false; 3];
        self.clicks = [0; 3];
        self.wheel = (0.0, 0.0);
        self.keys.clear();
        self.text.clear();
        self.back = false;
        self.forward = false;
    }

    pub fn key_with(&self, k: Key, ctrl: bool, alt: bool) -> bool {
        self.keys
            .iter()
            .any(|(kk, m)| *kk == k && m.ctrl == ctrl && m.alt == alt)
    }
}

pub const SCROLLBAR_W: i32 = 8;

pub fn scrollbar(r: Rect, content_h: i32, scroll: i32) -> Option<(Rect, Rect)> {
    if content_h <= r.h || r.h <= 0 {
        return None;
    }
    let track = Rect::new(r.right() - SCROLLBAR_W, r.y, SCROLLBAR_W, r.h);
    let th = ((r.h as i64 * r.h as i64) / content_h as i64).max(20) as i32;
    let max = (content_h - r.h).max(1);
    let ty = r.y + ((scroll.clamp(0, max) as i64 * (r.h - th) as i64) / max as i64) as i32;
    Some((track, Rect::new(track.x, ty, SCROLLBAR_W, th)))
}

pub trait Measure {
    fn cell(&mut self, px: u32) -> (i32, i32);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Exact(i32),
    Fit,
    Grow,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Row,
    Col,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Start,
    Center,
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub dir: Dir,
    pub size: [Size; 2],
    pub floating: Option<(i32, i32)>,
    pub pad: i32,
    pub gap: i32,
    pub cross: Align,
    pub clip: bool,
    pub scroll: (i32, i32),
}

impl Layout {
    pub fn row() -> Layout {
        Layout {
            dir: Dir::Row,
            size: [Size::Fit, Size::Fit],
            floating: None,
            pad: 0,
            gap: 0,
            cross: Align::Start,
            clip: false,
            scroll: (0, 0),
        }
    }
    pub fn col() -> Layout {
        Layout {
            dir: Dir::Col,
            ..Layout::row()
        }
    }
    pub fn size(mut self, w: Size, h: Size) -> Layout {
        self.size = [w, h];
        self
    }
    pub fn grow(self) -> Layout {
        self.size(Size::Grow, Size::Grow)
    }
    pub fn grow_x(mut self) -> Layout {
        self.size[0] = Size::Grow;
        self
    }
    pub fn grow_y(mut self) -> Layout {
        self.size[1] = Size::Grow;
        self
    }
    pub fn w(mut self, w: i32) -> Layout {
        self.size[0] = Size::Exact(w);
        self
    }
    pub fn h(mut self, h: i32) -> Layout {
        self.size[1] = Size::Exact(h);
        self
    }
    pub fn pad(mut self, p: i32) -> Layout {
        self.pad = p;
        self
    }
    pub fn gap(mut self, g: i32) -> Layout {
        self.gap = g;
        self
    }
    pub fn cross(mut self, a: Align) -> Layout {
        self.cross = a;
        self
    }
    pub fn scroll(mut self, x: i32, y: i32) -> Layout {
        self.scroll = (x, y);
        self.clip = true;
        self
    }
    pub fn floating(mut self, x: i32, y: i32) -> Layout {
        self.floating = Some((x, y));
        self
    }
}

pub const BORDER_LEFT: u8 = 1;
pub const BORDER_RIGHT: u8 = 2;
pub const BORDER_TOP: u8 = 4;
pub const BORDER_BOTTOM: u8 = 8;
pub const BORDER_ALL: u8 = 15;

#[derive(Clone, Copy, Debug, Default)]
pub struct Style {
    pub bg: Option<Color>,
    pub border: u8,
    pub border_color: Color,
}

impl Style {
    pub fn bg(c: Color) -> Style {
        Style {
            bg: Some(c),
            ..Default::default()
        }
    }
    pub fn border(mut self, sides: u8, c: Color) -> Style {
        self.border = sides;
        self.border_color = c;
        self
    }
}

pub struct Text {
    pub runs: Vec<(String, Color)>,
    pub px: u32,
    pub wrap: bool,
}

pub enum Kind {
    None,
    Text(Text),
    Custom(Box<dyn FnOnce(&mut Gfx, Rect)>),
}

pub struct Element {
    parent: Option<usize>,
    first: Option<usize>,
    last: Option<usize>,
    next: Option<usize>,
    prev: Option<usize>,
    pub kind: Kind,
    pub layout: Layout,
    pub style: Style,
    pub id: Option<Id>,
    size: [i32; 2],
    pos: [i32; 2],
    content: [i32; 2],
    lines: Vec<String>,
    pub rect: Rect,
    pub clip: Rect,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Interaction {
    pub hovered: bool,
    pub clicked: bool,
    pub double_clicked: bool,
    pub down: bool,
    pub drag: Option<(i32, i32)>,
    pub wheel: (f32, f32),
    pub rect: Option<Rect>,
}

#[derive(Default)]
pub struct Ui {
    els: Vec<Element>,
    open: Option<usize>,
    prev: HashMap<Id, (Rect, Rect, [i32; 2])>,
    hot: Option<Id>,
    active: Option<Id>,
    last_mouse: (i32, i32),
    scroll_drag: Option<(Id, i32)>,
    pub input: InputView,
    pub size: (i32, i32),
}

#[derive(Clone, Copy, Default)]
pub struct InputView {
    pub mouse: (i32, i32),
    pub pressed: [bool; 3],
    pub down: [bool; 3],
    pub clicks: [u8; 3],
    pub wheel: (f32, f32),
    pub mods: Mods,
}

impl Ui {
    pub fn begin(&mut self, input: &Input) {
        self.els.clear();
        self.open = None;
        self.size = input.size;
        self.input = InputView {
            mouse: input.mouse,
            pressed: input.pressed,
            down: input.down,
            clicks: input.clicks,
            wheel: input.wheel,
            mods: input.mods,
        };
        let (mx, my) = input.mouse;
        self.hot = self
            .prev
            .iter()
            .filter(|(_, (r, c, _))| r.contains(mx, my) && c.contains(mx, my))
            .max_by_key(|(id, (r, c, _))| {
                let v = r.intersect(c);
                (-(v.w as i64 * v.h as i64), **id)
            })
            .map(|(id, _)| *id);
        if input.pressed[0] {
            self.active = self.hot;
        }
        if !input.down[0] && !input.pressed[0] {
            self.active = None;
        }
    }

    pub fn hot(&self) -> Option<Id> {
        self.hot
    }

    pub fn active(&self) -> Option<Id> {
        self.active
    }

    pub fn interaction_of(&self, id: Id) -> Interaction {
        let hovered = self.hot == Some(id);
        let i = &self.input;
        Interaction {
            hovered,
            clicked: hovered && i.pressed[0],
            double_clicked: hovered && i.clicks[0] == 2,
            down: self.active == Some(id) && i.down[0],
            drag: if self.active == Some(id) && i.down[0] && !i.pressed[0] {
                Some((i.mouse.0 - self.last_mouse.0, i.mouse.1 - self.last_mouse.1))
            } else {
                None
            },
            wheel: if hovered { i.wheel } else { (0.0, 0.0) },
            rect: self.prev.get(&id).map(|(r, _, _)| *r),
        }
    }

    pub fn open(
        &mut self,
        kind: Kind,
        layout: Layout,
        style: Style,
        id: Option<Id>,
    ) -> Interaction {
        let i = self.els.len();
        let mut e = Element {
            parent: self.open,
            first: None,
            last: None,
            next: None,
            prev: None,
            kind,
            layout,
            style,
            id,
            size: [0, 0],
            pos: [0, 0],
            content: [0, 0],
            lines: Vec::new(),
            rect: Rect::default(),
            clip: Rect::default(),
        };
        if let Some(p) = self.open {
            if let Some(last) = self.els[p].last {
                e.prev = Some(last);
                self.els[last].next = Some(i);
            } else {
                self.els[p].first = Some(i);
            }
            self.els[p].last = Some(i);
        }
        self.els.push(e);
        self.open = Some(i);
        id.map(|id| self.interaction_of(id)).unwrap_or_default()
    }

    pub fn close(&mut self) {
        if let Some(i) = self.open {
            self.open = self.els[i].parent;
        }
    }

    pub fn leaf(
        &mut self,
        kind: Kind,
        layout: Layout,
        style: Style,
        id: Option<Id>,
    ) -> Interaction {
        let r = self.open(kind, layout, style, id);
        self.close();
        r
    }

    pub fn dragging(&self) -> bool {
        self.scroll_drag.is_some()
    }

    pub fn content_of(&self, id: Id) -> Option<([i32; 2], Rect)> {
        self.prev.get(&id).map(|(r, _, c)| (*c, *r))
    }

    pub fn scroll_by_wheel(&mut self, id: Id, offset: &mut i32) -> i32 {
        let (mx, my) = self.input.mouse;
        let inside = self
            .prev
            .get(&id)
            .is_some_and(|(r, c, _)| r.contains(mx, my) && c.contains(mx, my));
        if self.hot == Some(id) || inside {
            *offset -= self.input.wheel.1 as i32;
        }
        if let Some((content, r)) = self.content_of(id) {
            if let Some((track, thumb)) = scrollbar(r, content[1], *offset) {
                let max = (content[1] - r.h).max(0);
                if self.input.pressed[0] && inside && track.contains(mx, my) {
                    if thumb.contains(mx, my) {
                        self.scroll_drag = Some((id, my - thumb.y));
                    } else {
                        *offset = ((my - r.y) as i64 * content[1] as i64 / r.h.max(1) as i64)
                            as i32
                            - r.h / 2;
                    }
                }
                if let Some((did, grab)) = self.scroll_drag
                    && did == id
                    && self.input.down[0]
                {
                    let span = (r.h - thumb.h).max(1);
                    *offset = ((my - grab - r.y) as i64 * max as i64 / span as i64) as i32;
                }
            }
            *offset = (*offset).min((content[1] - r.h).max(0)).max(0);
        } else {
            *offset = (*offset).max(0);
        }
        if !self.input.down[0] {
            self.scroll_drag = None;
        }
        *offset
    }

    fn children(&self, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut c = self.els[i].first;
        while let Some(j) = c {
            if self.els[j].layout.floating.is_none() {
                out.push(j);
            }
            c = self.els[j].next;
        }
        out
    }

    pub fn wrap(text: &str, cols: usize) -> Vec<String> {
        let cols = cols.max(1);
        let mut out = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            let mut len = 0;
            for word in para.split(' ') {
                let wl = word.chars().count();
                if len > 0 && len + 1 + wl > cols {
                    out.push(std::mem::take(&mut line));
                    len = 0;
                }
                if len > 0 {
                    line.push(' ');
                    len += 1;
                }
                if wl > cols {
                    for c in word.chars() {
                        if len >= cols {
                            out.push(std::mem::take(&mut line));
                            len = 0;
                        }
                        line.push(c);
                        len += 1;
                    }
                } else {
                    line.push_str(word);
                    len += wl;
                }
            }
            out.push(line);
        }
        out
    }

    fn text_cols(t: &Text) -> i32 {
        t.runs.iter().map(|(s, _)| s.chars().count() as i32).sum()
    }

    pub fn end(&mut self, m: &mut dyn Measure) {
        let n = self.els.len();
        let win = [self.size.0, self.size.1];
        for axis in 0..2 {
            for i in (0..n).rev() {
                let kids = self.children(i);
                let e = &self.els[i];
                let along = (e.layout.dir == Dir::Row) == (axis == 0);
                let pad2 = e.layout.pad * 2;
                let v = match e.layout.size[axis] {
                    Size::Exact(v) => v,
                    Size::Grow => 0,
                    Size::Fit => {
                        if !kids.is_empty() {
                            let sizes: Vec<i32> =
                                kids.iter().map(|&k| self.els[k].size[axis]).collect();
                            if along {
                                sizes.iter().sum::<i32>()
                                    + e.layout.gap * (kids.len() as i32 - 1)
                                    + pad2
                            } else {
                                sizes.into_iter().max().unwrap_or(0) + pad2
                            }
                        } else {
                            match &e.kind {
                                Kind::Text(t) => {
                                    let (cw, rh) = m.cell(t.px);
                                    if axis == 0 {
                                        if t.wrap {
                                            0
                                        } else {
                                            Self::text_cols(t) * cw + pad2
                                        }
                                    } else {
                                        (if t.wrap {
                                            e.lines.len().max(1) as i32
                                        } else {
                                            1
                                        }) * rh
                                            + pad2
                                    }
                                }
                                _ => pad2,
                            }
                        }
                    }
                };
                self.els[i].size[axis] = v;
            }
            for i in 0..n {
                if (self.els[i].parent.is_none() || self.els[i].layout.floating.is_some())
                    && self.els[i].layout.size[axis] == Size::Grow
                {
                    self.els[i].size[axis] = win[axis];
                }
                let kids = self.children(i);
                if kids.is_empty() {
                    continue;
                }
                let e = &self.els[i];
                let along = (e.layout.dir == Dir::Row) == (axis == 0);
                let inner = e.size[axis] - e.layout.pad * 2;
                if along {
                    let growing: Vec<usize> = kids
                        .iter()
                        .copied()
                        .filter(|&k| self.els[k].layout.size[axis] == Size::Grow)
                        .collect();
                    let used: i32 = kids.iter().map(|&k| self.els[k].size[axis]).sum::<i32>()
                        + e.layout.gap * (kids.len() as i32 - 1);
                    if !growing.is_empty() {
                        let each = ((inner - used).max(0)) / growing.len() as i32;
                        for &k in &growing {
                            self.els[k].size[axis] = each;
                        }
                    }
                } else {
                    for &k in &kids {
                        if self.els[k].layout.size[axis] == Size::Grow {
                            self.els[k].size[axis] = inner.max(0);
                        }
                    }
                }
            }
            if axis == 0 {
                for i in 0..n {
                    let e = &self.els[i];
                    if let Kind::Text(t) = &e.kind
                        && t.wrap
                    {
                        let (cw, _) = m.cell(t.px);
                        let cols = ((e.size[0] - e.layout.pad * 2) / cw.max(1)).max(1) as usize;
                        let lines =
                            Self::wrap(t.runs.first().map(|(s, _)| s.as_str()).unwrap_or(""), cols);
                        self.els[i].lines = lines;
                    }
                }
            }
        }
        let screen = Rect::new(0, 0, win[0], win[1]);
        for i in 0..n {
            let (pos, clip) = match self.els[i].parent {
                None => (
                    self.els[i].layout.floating.map_or([0, 0], |(x, y)| [x, y]),
                    screen,
                ),
                Some(p) => {
                    let pe = &self.els[p];
                    let clip = if pe.layout.clip {
                        pe.clip.intersect(&pe.rect)
                    } else {
                        pe.clip
                    };
                    match self.els[i].layout.floating {
                        Some((x, y)) => ([x, y], screen),
                        None => (self.els[i].pos, clip),
                    }
                }
            };
            let e = &mut self.els[i];
            e.pos = pos;
            e.rect = Rect::new(pos[0], pos[1], e.size[0], e.size[1]);
            e.clip = clip;
            let kids = self.children(i);
            let e = &self.els[i];
            let (pad, gap, cross, size) = (e.layout.pad, e.layout.gap, e.layout.cross, e.size);
            let a = (e.layout.dir == Dir::Col) as usize;
            let b = 1 - a;
            let origin = [
                e.rect.x + pad - e.layout.scroll.0,
                e.rect.y + pad - e.layout.scroll.1,
            ];
            let mut cursor = 0;
            for &k in &kids {
                let ks = self.els[k].size;
                let mut p = origin;
                p[a] += cursor;
                p[b] += match cross {
                    Align::Start => 0,
                    Align::Center => (size[b] - pad * 2 - ks[b]) / 2,
                };
                cursor += ks[a] + gap;
                self.els[k].pos = p;
            }
            let mut content = size;
            content[a] = (cursor - gap).max(0) + pad * 2;
            self.els[i].content = content;
        }
        self.prev.clear();
        for e in &self.els {
            if let Some(id) = e.id {
                self.prev.insert(id, (e.rect, e.clip, e.content));
            }
        }
        self.last_mouse = self.input.mouse;
    }

    pub fn draw(&mut self, gfx: &mut Gfx, text_color: Color) {
        let els = std::mem::take(&mut self.els);
        let mut layers: Vec<(u8, Element)> = els
            .into_iter()
            .map(|e| (e.layout.floating.is_some() as u8, e))
            .collect();
        for i in 0..layers.len() {
            if let Some(p) = layers[i].1.parent {
                layers[i].0 = layers[i].0.max(layers[p].0);
            }
        }
        for layer in 0..2u8 {
            for (l, e) in layers.iter_mut() {
                if *l != layer || e.rect.intersect(&e.clip).is_empty() {
                    continue;
                }
                gfx.push_clip(e.clip);
                if let Some(bg) = e.style.bg {
                    gfx.rect(e.rect, bg);
                }
                let r = e.rect;
                match std::mem::replace(&mut e.kind, Kind::None) {
                    Kind::None => {}
                    Kind::Text(t) => {
                        let (_, rh) = gfx.cell(t.px);
                        let (x, y) = (r.x + e.layout.pad, r.y + e.layout.pad);
                        if t.wrap {
                            let color = t.runs.first().map(|(_, c)| *c).unwrap_or(text_color);
                            for (li, line) in e.lines.iter().enumerate() {
                                gfx.text(x, y + li as i32 * rh, t.px, line, color);
                            }
                        } else {
                            let mut pen = x;
                            for (s, c) in &t.runs {
                                pen = gfx.text(pen, y, t.px, s, *c);
                            }
                        }
                    }
                    Kind::Custom(f) => {
                        gfx.push_clip(r);
                        f(gfx, r);
                        gfx.pop_clip();
                    }
                }
                let (b, c) = (e.style.border, e.style.border_color);
                for (side, edge) in [
                    (BORDER_LEFT, Rect::new(r.x, r.y, 1, r.h)),
                    (BORDER_RIGHT, Rect::new(r.right() - 1, r.y, 1, r.h)),
                    (BORDER_TOP, Rect::new(r.x, r.y, r.w, 1)),
                    (BORDER_BOTTOM, Rect::new(r.x, r.bottom() - 1, r.w, 1)),
                ] {
                    if b & side != 0 {
                        gfx.rect(edge, c);
                    }
                }
                gfx.pop_clip();
            }
            for (l, e) in layers.iter() {
                if *l != layer || !e.layout.clip {
                    continue;
                }
                if let Some((track, thumb)) = scrollbar(e.rect, e.content[1], e.layout.scroll.1) {
                    gfx.push_clip(e.clip);
                    gfx.rect(track, [0, 0, 0, 60]);
                    gfx.rect(thumb.shrink(1), [140, 140, 148, 150]);
                    gfx.pop_clip();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Cells;
    impl Measure for Cells {
        fn cell(&mut self, _px: u32) -> (i32, i32) {
            (8, 16)
        }
    }

    fn text(s: &str) -> Kind {
        Kind::Text(Text {
            runs: vec![(s.into(), [255; 4])],
            px: 14,
            wrap: false,
        })
    }

    #[test]
    fn row_fits_and_grows() {
        let mut ui = Ui::default();
        let mut input = Input::default();
        input.size = (200, 100);
        ui.begin(&input);
        ui.open(
            Kind::None,
            Layout::row().grow().pad(2).gap(4),
            Style::default(),
            None,
        );
        ui.leaf(text("abc"), Layout::row(), Style::default(), Some(1));
        ui.leaf(Kind::None, Layout::row().grow(), Style::default(), Some(2));
        ui.leaf(text("de"), Layout::row(), Style::default(), Some(3));
        ui.close();
        ui.end(&mut Cells);
        let r = |id| ui.prev[&id].0;
        assert_eq!(r(1), Rect::new(2, 2, 24, 16));
        assert_eq!(r(2), Rect::new(30, 2, 200 - 4 - 24 - 16 - 8, 96));
        assert_eq!(r(3).x, 200 - 2 - 16);
    }

    #[test]
    fn wrapped_text_takes_lines() {
        let mut ui = Ui::default();
        let mut input = Input::default();
        input.size = (100, 100);
        ui.begin(&input);
        ui.open(Kind::None, Layout::col().grow(), Style::default(), None);
        ui.leaf(
            Kind::Text(Text {
                runs: vec![("one two three four".into(), [255; 4])],
                px: 14,
                wrap: true,
            }),
            Layout::row().grow_x(),
            Style::default(),
            Some(1),
        );
        ui.close();
        ui.end(&mut Cells);
        assert_eq!(
            Ui::wrap("one two three four", 12),
            ["one two", "three four"]
        );
        assert_eq!(ui.prev[&1].0.h, 32);
    }

    #[test]
    fn hit_test_is_one_frame_late() {
        let mut ui = Ui::default();
        let mut input = Input::default();
        input.size = (100, 100);
        input.mouse = (10, 10);
        ui.begin(&input);
        assert!(
            !ui.leaf(
                Kind::None,
                Layout::row().w(50).h(50),
                Style::default(),
                Some(7)
            )
            .hovered
        );
        ui.end(&mut Cells);
        input.pressed[0] = true;
        ui.begin(&input);
        let it = ui.leaf(
            Kind::None,
            Layout::row().w(50).h(50),
            Style::default(),
            Some(7),
        );
        assert!(it.hovered && it.clicked);
    }
}
