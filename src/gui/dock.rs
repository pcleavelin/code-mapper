//! The panels around the centre: which edge each one is docked to, in what order, and how
//! big. Dragging a panel's header onto another edge moves it; dragging the splitter on its
//! centre side resizes it. Nothing is saved: every launch starts from the default layout.

use super::*;
use winit::window::CursorIcon;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Panel {
    Nav, // Paths, Symbols, Files
    Xrefs,
    Output,
}

impl Panel {
    fn name(self) -> &'static str {
        match self {
            Panel::Nav => "nav",
            Panel::Xrefs => "xrefs",
            Panel::Output => "output",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Edge {
    Left,
    Right,
    Bottom,
}

struct Place {
    panel: Panel,
    edge: Edge,
    size: Option<i32>, // across the edge (a width at the sides, a height at the bottom); None is the default
}

/// A header held down; it becomes a move once the pointer has left the spot it was pressed at.
struct Grab {
    panel: Panel,
    from: (i32, i32),
    moving: bool,
}

pub(super) struct Dock {
    places: Vec<Place>, // each edge's panels in screen order, left to right and top to bottom
    grab: Option<Grab>,
    pub cursor: CursorIcon,
}

impl Default for Dock {
    fn default() -> Dock {
        let place = |panel, edge| Place { panel, edge, size: None };
        Dock { places: vec![place(Panel::Nav, Edge::Left), place(Panel::Xrefs, Edge::Right), place(Panel::Output, Edge::Bottom)], grab: None, cursor: CursorIcon::Default }
    }
}

fn panel_id(p: Panel) -> Id {
    ui::id_with(ui::id("panel"), p.name())
}

impl Dock {
    /// Every panel's size this frame, in `places` order, held between a floor and what leaves
    /// the centre 20 columns and 12 rows (with the top and status bars). The stored size is
    /// left alone, so a panel squeezed by a small window grows back with it.
    pub(super) fn sizes(&self, win: (i32, i32), cell: (i32, i32)) -> Vec<i32> {
        let mut out = vec![0; self.places.len()];
        for bottom in [false, true] {
            let (min, mut budget) = if bottom { (3 * cell.1, win.1 - 12 * cell.1) } else { (12 * cell.0, win.0 - 20 * cell.0) };
            let mine: Vec<usize> = (0..self.places.len()).filter(|&i| (self.places[i].edge == Edge::Bottom) == bottom).collect();
            for (k, &i) in mine.iter().enumerate() {
                let p = &self.places[i];
                let want = p.size.unwrap_or_else(|| default_size(p.panel, p.edge, win, cell));
                let s = want.clamp(min, (budget - min * (mine.len() - k - 1) as i32).max(min));
                out[i] = s;
                budget -= s;
            }
        }
        out
    }

    /// Takes `panel` out and puts it at position `at` among `edge`'s panels. A panel changing
    /// between a side and the bottom goes back to the default size, since its size was across
    /// the other axis.
    fn move_to(&mut self, panel: Panel, edge: Edge, at: usize) {
        let Some(from) = self.places.iter().position(|p| p.panel == panel) else { return };
        let mut p = self.places.remove(from);
        if (p.edge == Edge::Bottom) != (edge == Edge::Bottom) {
            p.size = None;
        }
        p.edge = edge;
        let i = self.places.iter().enumerate().filter(|(_, q)| q.edge == edge).nth(at).map_or(self.places.len(), |(i, _)| i);
        self.places.insert(i, p);
    }

    /// For dumps: each panel's edge in order, then the sizes.
    pub(super) fn describe(&self) -> String {
        let places: Vec<String> = self.places.iter().map(|p| format!("{}={:?}", p.panel.name(), p.edge)).collect();
        let sizes: Vec<String> = self.places.iter().map(|p| format!("{:?}", p.size)).collect();
        format!("{} sizes={}", places.join(" "), sizes.join(","))
    }
}

fn default_size(panel: Panel, edge: Edge, win: (i32, i32), cell: (i32, i32)) -> i32 {
    match edge {
        Edge::Bottom => (win.1 / 6).max(6 * cell.1),
        _ => (if panel == Panel::Nav { 44 } else { 40 } * cell.0).min(win.0 / 4),
    }
}

impl App {
    fn split_w(&self) -> i32 {
        (self.px as i32 / 2).max(5)
    }

    /// The panels docked to `edge`, each with its splitter on the centre side.
    pub(super) fn docked(&mut self, edge: Edge, sizes: &[i32]) {
        let split = self.split_w();
        for i in 0..self.dock.places.len() {
            let Place { panel, edge: e, .. } = self.dock.places[i];
            if e != edge {
                continue;
            }
            let size = sizes[i];
            let mut layout = if edge == Edge::Bottom { Layout::col().grow_x().h(size + split) } else { Layout::row().w(size + split).grow_y() };
            layout.clip = true;
            self.ui.open(Kind::None, layout, Style::default(), Some(panel_id(panel)));
            if edge != Edge::Left {
                self.splitter(i);
            }
            match panel {
                Panel::Nav => self.left_panel(),
                Panel::Xrefs => self.xrefs_panel(if edge == Edge::Bottom { (self.ui.size.0, size) } else { (size, self.ui.size.1) }),
                Panel::Output => self.output_panel(),
            }
            if edge == Edge::Left {
                self.splitter(i);
            }
            self.ui.close();
        }
    }

    /// The bar between a panel and the centre, wider than the line it shows so it is easy to
    /// catch; the rest of it is the centre's colour.
    fn splitter(&mut self, i: usize) {
        let Place { panel, edge, .. } = self.dock.places[i];
        let split = self.split_w();
        let id = ui::id_with(ui::id("split"), panel.name());
        let it = self.ui.interaction_of(id);
        if it.down || (it.hovered && !self.ui.input.down[0]) {
            self.dock.cursor = if edge == Edge::Bottom { CursorIcon::RowResize } else { CursorIcon::ColResize };
        }
        let (layout, side) = match edge {
            Edge::Left => (Layout::col().w(split).grow_y(), BORDER_LEFT),
            Edge::Right => (Layout::col().w(split).grow_y(), BORDER_RIGHT),
            Edge::Bottom => (Layout::row().grow_x().h(split), BORDER_BOTTOM),
        };
        let style = if it.hovered || it.down { Style::bg(ACCENT) } else { Style::bg(BG).border(side, BORDER) };
        self.ui.leaf(Kind::None, layout, style, Some(id));
    }

    /// Opens a panel's header row: pressing on it anywhere a button is not starts a move.
    /// The caller fills it and closes it.
    pub(super) fn grip(&mut self, panel: Panel) {
        let id = ui::id_with(ui::id("grip"), panel.name());
        let held = self.dock.grab.as_ref().is_some_and(|g| g.panel == panel);
        let it = self.ui.interaction_of(id);
        if it.clicked {
            self.dock.grab = Some(Grab { panel, from: self.ui.input.mouse, moving: false });
        }
        if held {
            self.dock.cursor = CursorIcon::Grabbing;
        } else if it.hovered && !self.ui.input.down[0] {
            self.dock.cursor = CursorIcon::Grab;
        }
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(4).cross(Align::Center), Style::bg(if it.hovered || held { HOVER } else { PANEL }).border(BORDER_BOTTOM, BORDER), Some(id));
    }

    /// The dock's share of the frame's input, taken before any size is read so a drag shows
    /// in the frame it happens: a held splitter puts its panel's inner edge under the
    /// pointer, and a held header arms or, released, drops.
    pub(super) fn dock_input(&mut self) {
        let split = self.split_w();
        let (mx, my) = self.ui.input.mouse;
        for i in 0..self.dock.places.len() {
            let Place { panel, edge, .. } = self.dock.places[i];
            if !self.ui.interaction_of(ui::id_with(ui::id("split"), panel.name())).down {
                continue;
            }
            if let Some(r) = self.ui.interaction_of(panel_id(panel)).rect {
                self.dock.places[i].size = Some(match edge {
                    Edge::Left => mx - r.x - split / 2,
                    Edge::Right => r.right() - mx - split / 2,
                    Edge::Bottom => r.bottom() - my - split / 2,
                });
            }
        }
        let Some(g) = self.dock.grab.as_mut() else { return };
        if (mx - g.from.0).abs() + (my - g.from.1).abs() > 8 {
            g.moving = true;
        }
        if !self.ui.input.down[0] {
            let (panel, moving) = (g.panel, g.moving);
            self.dock.grab = None;
            if let Some((edge, at, _)) = self.drop_target(panel, (mx, my)).filter(|_| moving) {
                self.dock.move_to(panel, edge, at);
            }
        }
    }

    /// Where a dragged header would land, as a band over the window. Runs after the tree is
    /// built, so the band floats over it.
    pub(super) fn drag_band(&mut self) {
        let Some(g) = self.dock.grab.as_ref().filter(|g| g.moving) else { return };
        if let Some((_, _, band)) = self.drop_target(g.panel, self.ui.input.mouse) {
            self.ui.leaf(Kind::None, Layout::col().floating(band.x, band.y).w(band.w).h(band.h), Style::bg(dim(ACCENT, 70)).border(ui::BORDER_ALL, ACCENT), None);
        }
    }

    /// The edge under the pointer (the bottom panels or the bottom quarter of the row above
    /// them, else the row's nearer half), the position among that edge's other panels, and
    /// the band the panel would take there. Outside the body there is none: letting go there
    /// cancels the move.
    fn drop_target(&self, panel: Panel, (mx, my): (i32, i32)) -> Option<(Edge, usize, Rect)> {
        let body = self.ui.interaction_of(ui::id("body")).rect?;
        let row = self.ui.interaction_of(ui::id("dock-row")).rect?;
        if !body.contains(mx, my) {
            return None;
        }
        let edge = if my > row.bottom() - row.h / 4 {
            Edge::Bottom
        } else if mx < body.x + body.w / 2 {
            Edge::Left
        } else {
            Edge::Right
        };
        let others: Vec<Rect> = self.dock.places.iter().filter(|p| p.edge == edge && p.panel != panel).filter_map(|p| self.ui.interaction_of(panel_id(p.panel)).rect).collect();
        let at = others.iter().filter(|r| if edge == Edge::Bottom { r.y + r.h / 2 < my } else { r.x + r.w / 2 < mx }).count();
        let place = self.dock.places.iter().find(|p| p.panel == panel)?;
        let kept = place.size.filter(|_| (place.edge == Edge::Bottom) == (edge == Edge::Bottom));
        let d = kept.unwrap_or_else(|| default_size(panel, edge, self.ui.size, self.cell));
        let band = match edge {
            Edge::Left => Rect::new(others.get(at).map_or(others.last().map_or(row.x, |r| r.right()), |r| r.x), row.y, d, row.h),
            Edge::Right => Rect::new(others.get(at).map_or(row.right(), |r| r.x) - d, row.y, d, row.h),
            Edge::Bottom => Rect::new(body.x, others.get(at).map_or(body.bottom(), |r| r.y) - d, body.w, d),
        };
        Some((edge, at, band))
    }
}
