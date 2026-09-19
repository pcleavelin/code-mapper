//! The Graph tab: the selection as a left-to-right tree of nodes showing their code, derived
//! every frame from the selected path (or the selected symbol alone) plus the expansions the
//! reader opened. The tree is laid out in pixels at the current zoom, so text is drawn at a
//! whole pixel size and never scaled. Everything is drawn by one custom element; clicks are
//! resolved against the rectangles the last frame recorded.

use crate::gfx::{Color, Gfx, Rect};
use crate::gui::{self, Action, App, ACCENT, BORDER, FIELD, GREEN, HOVER, ORANGE, PANEL, TEXT, WEAK};
use crate::index::{self, Index, SymRef};
use crate::map::Map;
use crate::ui::{self, Kind, Layout, Measure, Style};
use std::collections::{BTreeMap, HashMap, HashSet};

/// A graph node: a symbol, and the step it stands for when it is one. Two slice steps of one
/// symbol are two nodes; an expansion that reveals a symbol some step already shows reuses
/// that step's node.
pub type Node = (SymRef, Option<usize>);

const PREVIEW_LINES: usize = 12;
const MIN_COLS: i32 = 44;
const MAX_COLS: i32 = 110;
const GAP_X: i32 = 110;
const GAP_Y: i32 = 28;

#[derive(Clone, Copy)]
struct Metrics {
    cell_w: i32,
    row_h: i32,
    pad: i32,
    gap_x: i32, // the gaps scale with the zoom so the tree keeps its shape
    gap_y: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Btn {
    Less,
    Listing,
    Callees,
    Callers,
}

/// A rectangle of last frame's scene the mouse can land on, in window pixels.
#[derive(Clone, Copy)]
enum Hit {
    Header(Node),
    Button(Node, Btn),
    Line(Node, usize), // a code line: the file line index
    Body(Node),
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Pan,
    Node(Node),
}

#[derive(Default)]
pub struct Graph {
    nodes: Vec<Node>,
    col: HashMap<Node, i32>,
    pos: HashMap<Node, (i32, i32)>,        // this frame, canvas pixels
    manual: HashMap<Node, (f32, f32)>,     // dragged positions, in zoom-1 pixels; layout stops while any exist
    size: HashMap<Node, (i32, i32)>,
    range: HashMap<Node, (usize, usize)>, // lines a node shows: the step's slice, or the whole symbol
    by_sym: HashMap<SymRef, Node>,
    path_id: Option<usize>,
    step: HashMap<Node, (usize, usize, String)>, // step node -> (pre-order position, anchor index, hierarchical number)
    step_parent: HashMap<Node, Node>,
    origin: HashMap<Node, (Node, bool)>, // expansion node -> (the node that revealed it, via callees?)
    expansions: Vec<(Node, bool)>,
    collapsed: HashSet<Node>, // nodes cut to a preview
    code_top: HashMap<Node, i32>, // y of each node's first code row, this frame
    has_note: HashSet<Node>,      // step nodes with a note row
    pub pan: (i32, i32),
    pub zoom: f32,
    pub want_look: bool, // centre on the focus once it has a position
    hits: Vec<(Rect, Hit)>,
    drag: Option<Drag>,
}

impl Graph {
    pub fn new() -> Graph {
        Graph { zoom: 1.0, ..Default::default() }
    }
}

fn has_word(f: &index::File, li: usize, word: &str) -> bool {
    index::call_site(f, li, word)
}

impl Graph {
    fn focus_node(&self, focus: Option<SymRef>, focus_step: Option<usize>) -> Option<Node> {
        let f = focus?;
        let stepped = (f, focus_step);
        if self.col.contains_key(&stepped) { Some(stepped) } else { self.by_sym.get(&f).copied() }
    }

    fn lines_shown(&self, n: Node) -> (usize, usize) {
        let (lo, hi) = self.range[&n];
        let total = hi - lo + 1;
        (if self.collapsed.contains(&n) { total.min(PREVIEW_LINES) } else { total }, total)
    }

    fn add(&mut self, idx: &Index, n: Node, col: i32, range: (usize, usize)) {
        if self.col.contains_key(&n) {
            return;
        }
        let f = &idx.files[n.0.file];
        let hi = range.1.min(f.lines.len().saturating_sub(1));
        self.col.insert(n, col);
        self.range.insert(n, (range.0.min(hi), hi));
        self.by_sym.entry(n.0).or_insert(n);
        self.nodes.push(n);
    }

    /// The first line of `n` that names `word` as an identifier outside comments and strings.
    fn call_line(&self, idx: &Index, n: Node, word: &str) -> Option<usize> {
        let (lo, hi) = self.range[&n];
        let f = &idx.files[n.0.file];
        (lo..=hi.min(f.lines.len().saturating_sub(1))).find(|&li| has_word(f, li, word))
    }

    /// What `n` calls: the symbol's callees, narrowed to the ones its lines name when the node
    /// is a slice of the symbol.
    fn callees_of(&self, idx: &Index, n: Node) -> Vec<SymRef> {
        let s = idx.sym(n.0);
        if self.range[&n] == (s.start, s.end) {
            return s.callees.clone();
        }
        s.callees.iter().copied().filter(|c| self.call_line(idx, n, &idx.sym(*c).name).is_some()).collect()
    }

    /// Derive the node set from the selection + expansions.
    fn rebuild(&mut self, idx: &Index, map: &Map, path_id: Option<usize>, focus: Option<SymRef>) {
        self.nodes.clear();
        self.col.clear();
        self.range.clear();
        self.by_sym.clear();
        self.step.clear();
        self.step_parent.clear();
        self.origin.clear();
        self.has_note.clear();
        self.path_id = path_id.filter(|&pi| pi < map.paths.len());
        if let Some(pi) = self.path_id {
            let anchors = &map.paths[pi].anchors;
            let mut node_of: HashMap<usize, Node> = HashMap::new();
            for (k, (ai, depth, number)) in map.numbered(idx, pi).into_iter().enumerate() {
                let a = &anchors[ai];
                let Some(fi) = idx.find_file(&a.file) else { continue };
                let Some(si) = a.sym else { continue };
                let n = (SymRef { file: fi, sym: si }, Some(ai));
                self.add(idx, n, depth as i32, (a.line_start, a.line_end));
                self.step.insert(n, (k, ai, number));
                if !a.note.is_empty() {
                    self.has_note.insert(n);
                }
                node_of.insert(ai, n);
                if a.parent >= 0 {
                    if let Some(&p) = node_of.get(&(a.parent as usize)) {
                        self.step_parent.insert(n, p);
                    }
                }
            }
        }
        if let Some(f) = focus {
            if !self.by_sym.contains_key(&f) {
                let s = idx.sym(f);
                self.add(idx, (f, None), 0, (s.start, s.end)); // an off-path selection is its own root
            }
        }
        for (n, callees) in self.expansions.clone() {
            let Some(&col) = self.col.get(&n) else { continue };
            let s = idx.sym(n.0);
            let list = if callees { self.callees_of(idx, n) } else { s.callers.clone() };
            let dc = if callees { 1 } else { -1 };
            for r in list {
                if self.by_sym.contains_key(&r) {
                    continue;
                }
                let t = idx.sym(r);
                let m = (r, None);
                self.origin.insert(m, (n, callees));
                self.add(idx, m, col + dc, (t.start, t.end));
            }
        }
        self.manual.retain(|n, _| self.col.contains_key(n));
    }

    fn toggle(&mut self, n: Node, callees: bool) {
        match self.expansions.iter().position(|e| *e == (n, callees)) {
            Some(i) => {
                self.expansions.remove(i);
            }
            None => self.expansions.push((n, callees)),
        }
        self.manual.clear();
    }

    fn node_size(&self, idx: &Index, n: Node, m: Metrics) -> (i32, i32) {
        let (lo, _) = self.range[&n];
        let (shown, total) = self.lines_shown(n);
        let f = &idx.files[n.0.file];
        let longest = f.lines[lo..lo + shown].iter().map(|l| l.chars().count()).max().unwrap_or(0) as i32 + 6;
        let cols = longest.clamp(MIN_COLS, MAX_COLS);
        let note_rows = i32::from(self.has_note.contains(&n));
        let rows = 1 + note_rows + shown as i32 + usize::from(shown < total) as i32;
        (cols * m.cell_w + m.pad * 2, rows * m.row_h + m.pad * 2 + 4)
    }

    /// Recompute every position as a left-to-right forest: a column per depth anchored at
    /// column 0, each subtree stacked beside its parent, children in the order the parent's
    /// code calls them.
    fn layout(&mut self, idx: &Index, m: Metrics) {
        if self.nodes.is_empty() {
            return;
        }
        let mut cols: BTreeMap<i32, Vec<Node>> = BTreeMap::new();
        for &n in &self.nodes {
            cols.entry(self.col[&n]).or_default().push(n);
        }
        let (first, last) = (*cols.keys().next().unwrap(), *cols.keys().last().unwrap());
        let anchor = 0.clamp(first, last);
        let col_w: BTreeMap<i32, i32> = cols.iter().map(|(&c, ns)| (c, ns.iter().map(|&n| self.size[&n].0).max().unwrap_or(0))).collect();
        let mut col_x: BTreeMap<i32, i32> = BTreeMap::new();
        col_x.insert(anchor, 0);
        for c in (anchor + 1)..=last {
            col_x.insert(c, col_x[&(c - 1)] + col_w[&(c - 1)] + m.gap_x);
        }
        for c in (first..anchor).rev() {
            col_x.insert(c, col_x[&(c + 1)] - col_w[&c] - m.gap_x);
        }
        // the forest
        let mut right: HashMap<Node, Vec<Node>> = HashMap::new();
        let mut left: HashMap<Node, Vec<Node>> = HashMap::new();
        let mut has_parent: HashSet<Node> = HashSet::new();
        for &n in &self.nodes {
            if let Some(&p) = self.step_parent.get(&n) {
                right.entry(p).or_default().push(n);
                has_parent.insert(n);
            } else if let Some(&(o, callees)) = self.origin.get(&n) {
                if callees { right.entry(o).or_default() } else { left.entry(o).or_default() }.push(n);
                has_parent.insert(n);
            }
        }
        for (&p, kids) in right.iter_mut() {
            kids.sort_by_key(|k| (self.call_line(idx, p, &idx.sym(k.0).name).unwrap_or(usize::MAX), self.step.get(k).map_or(usize::MAX, |s| s.0)));
        }
        let mut roots: Vec<Node> = self.nodes.iter().copied().filter(|n| !has_parent.contains(n)).collect();
        roots.sort_by_key(|r| self.step.get(r).map_or(usize::MAX, |s| s.0));

        // block heights, bottom-up
        let mut height: HashMap<Node, i32> = HashMap::new();
        let gap_y = m.gap_y;
        fn measure(g: &Graph, gap_y: i32, r: Node, right: &HashMap<Node, Vec<Node>>, left: &HashMap<Node, Vec<Node>>, height: &mut HashMap<Node, i32>, seen: &mut HashSet<Node>) -> i32 {
            if !seen.insert(r) {
                return 0;
            }
            let mut stack = |kids: Option<&Vec<Node>>, height: &mut HashMap<Node, i32>, seen: &mut HashSet<Node>| -> i32 {
                let mut h = 0;
                for &k in kids.into_iter().flatten() {
                    let kh = measure(g, gap_y, k, right, left, height, seen);
                    if kh > 0 {
                        h += kh + gap_y;
                    }
                }
                (h - gap_y).max(0)
            };
            let rh = stack(right.get(&r), height, seen);
            let lh = stack(left.get(&r), height, seen);
            let h = g.size[&r].1.max(rh).max(lh);
            height.insert(r, h);
            h
        }
        let mut seen = HashSet::new();
        for &r in &roots {
            measure(self, gap_y, r, &right, &left, &mut height, &mut seen);
        }
        // place, top-down
        fn place(g: &mut Graph, gap_y: i32, r: Node, top: i32, col_x: &BTreeMap<i32, i32>, right: &HashMap<Node, Vec<Node>>, left: &HashMap<Node, Vec<Node>>, height: &HashMap<Node, i32>, done: &mut HashSet<Node>) {
            if !done.insert(r) {
                return;
            }
            let block = height[&r];
            let own = g.size[&r].1;
            g.pos.insert(r, (col_x[&g.col[&r]], top + (block - own) / 2));
            for kids in [right.get(&r), left.get(&r)] {
                let kids: Vec<Node> = kids.into_iter().flatten().copied().filter(|k| height.contains_key(k) && !done.contains(k)).collect();
                let stack_h: i32 = kids.iter().map(|k| height[k] + gap_y).sum::<i32>() - gap_y;
                let mut cur = top + (block - stack_h.max(0)) / 2;
                for k in kids {
                    place(g, gap_y, k, cur, col_x, right, left, height, done);
                    cur += height[&k] + gap_y;
                }
            }
        }
        let mut done = HashSet::new();
        let mut cur = 0;
        for &r in &roots {
            if done.contains(&r) {
                continue;
            }
            place(self, gap_y, r, cur, &col_x, &right, &left, &height, &mut done);
            cur += height[&r] + gap_y * 2;
        }
        // different subtrees can still meet in one column; push the later one down
        for ns in cols.values_mut() {
            ns.sort_by_key(|n| self.pos[n].1);
            let mut prev_bottom = i32::MIN / 2;
            for &n in ns.iter() {
                let h = self.size[&n].1;
                let y = self.pos[&n].1.max(prev_bottom + gap_y);
                self.pos.get_mut(&n).unwrap().1 = y;
                prev_bottom = y + h;
            }
        }
    }
}

/// Everything the custom element draws, in window pixels.
struct Scene {
    px: u32,
    m: Metrics,
    edges: Vec<([(f32, f32); 4], Color, f32, bool)>, // curve, colour, width, arrow head
    nodes: Vec<SceneNode>,
}

struct SceneNode {
    rect: Rect,
    fill: Color,
    border: Color,
    border_w: i32,
    header: Vec<(String, Color)>,
    buttons: Vec<(Rect, String, bool)>, // rect, label, hovered
    note: Option<String>,
    code_top: i32,
    lines: Vec<(usize, Vec<(String, Color)>)>, // (file line, runs)
    tinted: HashSet<usize>,
    more: Option<usize>,
}

impl App {
    /// The graph tab: input from last frame's rectangles, then the tree rebuilt, laid out and
    /// turned into a scene that a custom element draws.
    pub fn graph_tab(&mut self, gfx: &mut Gfx) {
        let canvas_id = ui::id("graph-canvas");
        let it = self.ui.interaction_of(canvas_id);
        let canvas = it.rect.unwrap_or(Rect::new(0, 0, 100, 100));
        let mouse = self.ui.input.mouse;
        let mods = self.ui.input.mods;

        // zoom around the mouse with ctrl+wheel, pan with the wheel
        if it.wheel.1 != 0.0 && (it.hovered || self.graph.drag.is_some()) {
            if mods.ctrl {
                let old = self.graph.zoom;
                let new = (old * if it.wheel.1 > 0.0 { 1.1 } else { 1.0 / 1.1 }).clamp(0.3, 2.0);
                // keep the scene point under the mouse still
                let (mx, my) = ((mouse.0 - canvas.x) as f32, (mouse.1 - canvas.y) as f32);
                let (px, py) = ((mx - self.graph.pan.0 as f32) / old, (my - self.graph.pan.1 as f32) / old);
                self.graph.pan = ((mx - px * new).round() as i32, (my - py * new).round() as i32);
                self.graph.zoom = new;
            } else if mods.shift {
                self.graph.pan.0 += it.wheel.1 as i32;
            } else {
                self.graph.pan.1 += it.wheel.1 as i32;
                self.graph.pan.0 += it.wheel.0 as i32;
            }
        }
        // clicks and drags against last frame's hit rectangles
        let hit = self.graph.hits.iter().rev().find(|(r, _)| r.contains(mouse.0, mouse.1)).map(|(_, h)| *h);
        if it.clicked {
            match hit {
                Some(Hit::Button(n, b)) => match b {
                    Btn::Less => {
                        if !self.graph.collapsed.remove(&n) {
                            self.graph.collapsed.insert(n);
                        }
                        self.graph.manual.clear();
                    }
                    Btn::Listing => self.actions.push(Action::GoTo(n.0.file, self.graph.range[&n].0)),
                    Btn::Callees => self.graph.toggle(n, true),
                    Btn::Callers => self.graph.toggle(n, false),
                },
                Some(Hit::Header(n)) => {
                    self.graph.drag = Some(Drag::Node(n));
                    self.actions.push(match (n.1, self.graph.path_id) {
                        (Some(ai), Some(pi)) => Action::SelectStep(pi, ai, false),
                        _ => Action::Focus(n.0),
                    });
                    self.graph.want_look = false;
                }
                Some(Hit::Line(n, li)) => {
                    let (lo, _) = self.graph.range[&n];
                    let _ = lo;
                    if let Some(rect) = self.graph.hits.iter().find(|(_, h)| matches!(h, Hit::Line(m, l) if *m == n && *l == li)).map(|(r, _)| *r) {
                        let cell = gfx.cell(self.graph_px()).0.max(1);
                        let col = (((mouse.0 - rect.x) / cell) as usize).saturating_sub(5);
                        if mods.alt {
                            if let Some(r) = self.symbol_at(n.0.file, li, col) {
                                self.actions.push(Action::Peek(r));
                            }
                        } else if mods.ctrl || it.double_clicked {
                            self.actions.push(Action::Jump(n.0.file, li, col));
                        }
                    }
                }
                Some(Hit::Body(_)) => {}
                None => self.graph.drag = Some(Drag::Pan),
            }
        }
        if let Some(d) = it.drag {
            match self.graph.drag {
                Some(Drag::Pan) => {
                    self.graph.pan.0 += d.0;
                    self.graph.pan.1 += d.1;
                }
                Some(Drag::Node(n)) => {
                    let z = self.graph.zoom;
                    let base = self.graph.pos.get(&n).copied().unwrap_or((0, 0));
                    let cur = self.graph.manual.get(&n).copied().unwrap_or((base.0 as f32 / z, base.1 as f32 / z));
                    self.graph.manual.insert(n, (cur.0 + d.0 as f32 / z, cur.1 + d.1 as f32 / z));
                }
                None => {}
            }
        }
        if !it.down {
            self.graph.drag = None;
        }
        if let Some(Hit::Line(n, li)) = hit.filter(|_| it.hovered && !self.ui.input.down[0]) {
            if let Some(rect) = self.graph.hits.iter().find(|(_, h)| matches!(h, Hit::Line(m, l) if *m == n && *l == li)).map(|(r, _)| *r) {
                let cell = gfx.cell(self.graph_px()).0.max(1);
                let col = (((mouse.0 - rect.x) / cell) as usize).saturating_sub(5);
                if let Some(r) = self.symbol_at(n.0.file, li, col) {
                    self.tooltip = Some((r, mouse));
                }
            }
        }

        // the tree for this selection
        let path_id = if self.sel_anchor.is_some() { self.sel_path } else { None };
        self.graph.rebuild(&self.idx, &self.map, path_id, self.focus);
        let px = self.graph_px();
        let (cell_w, row_h) = gfx.cell(px);
        let z = self.graph.zoom;
        let m = Metrics { cell_w, row_h, pad: (6.0 * z).round() as i32, gap_x: (GAP_X as f32 * z).round() as i32, gap_y: (GAP_Y as f32 * z).round() as i32 };
        for &n in &self.graph.nodes.clone() {
            let s = self.graph.node_size(&self.idx, n, m);
            self.graph.size.insert(n, s);
        }
        self.graph.layout(&self.idx, m);
        let z = self.graph.zoom;
        for (n, p) in self.graph.manual.clone() {
            self.graph.pos.insert(n, ((p.0 * z).round() as i32, (p.1 * z).round() as i32));
        }
        let focus_node = self.graph.focus_node(self.focus, self.sel_anchor);
        if self.graph.want_look {
            if let Some(f) = focus_node.and_then(|f| self.graph.pos.get(&f).map(|p| (*p, self.graph.size[&f]))) {
                let ((x, y), (w, h)) = f;
                self.graph.pan = (canvas.w / 2 - x - w / 2, canvas.h / 2 - y - h / 2);
                self.graph.want_look = false;
            }
        }

        // toolbar
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(8).cross(ui::Align::Center), Style::default(), None);
        self.label("drag the background to pan, ctrl+wheel to zoom, drag a node's title to move it", WEAK);
        if self.small_button("1:1", ui::id("graph-1to1")).clicked {
            self.graph.zoom = 1.0;
            self.graph.want_look = true;
        }
        if self.small_button("auto layout", ui::id("graph-auto")).clicked {
            self.graph.manual.clear();
        }
        if let Some(pi) = self.graph.path_id {
            let s = format!("path '{}': green edges are its steps, grey ones expansions, orange a call back up the tree", self.map.paths[pi].name);
            self.label(&s, WEAK);
        }
        self.label(&format!("{:.0}%", z * 100.0), WEAK);
        self.ui.close();

        // the scene
        let scene = self.build_scene(canvas, focus_node, px, m);
        let hovered_canvas = it.hovered;
        let _ = hovered_canvas;
        self.ui.leaf(
            Kind::Custom(Box::new(move |gfx: &mut Gfx, r: Rect| draw_scene(gfx, r, &scene))),
            Layout::col().grow(),
            Style::bg(gui::BG),
            Some(canvas_id),
        );
    }

    /// The node font: the UI size scaled by the zoom, whole pixels.
    fn graph_px(&self) -> u32 {
        ((self.px as f32) * self.graph.zoom).round().max(6.0) as u32
    }

    fn build_scene(&mut self, canvas: Rect, focus_node: Option<Node>, px: u32, m: Metrics) -> Scene {
        let (ox, oy) = (canvas.x + self.graph.pan.0, canvas.y + self.graph.pan.1);
        let mouse = self.ui.input.mouse;
        let mut hits = Vec::new();
        let mut nodes = Vec::new();
        let step_color = GREEN;
        // nodes
        for &n in &self.graph.nodes {
            let (x, y) = self.graph.pos[&n];
            let (w, h) = self.graph.size[&n];
            let rect = Rect::new(ox + x, oy + y, w, h);
            let s = self.idx.sym(n.0);
            let f = &self.idx.files[n.0.file];
            let (lo, _) = self.graph.range[&n];
            let (shown, total) = self.graph.lines_shown(n);
            let on_path = self.graph.step.contains_key(&n);
            let off_path = self.graph.path_id.is_some() && !on_path;
            let focused = focus_node == Some(n);
            let (border, border_w) = if focused { (ACCENT, 2) } else if on_path { (step_color, 2) } else { (BORDER, 1) };
            let node_hits_from = hits.len();
            let fill = if off_path { PANEL } else { FIELD };
            let mut header = Vec::new();
            if let Some((_, _, number)) = self.graph.step.get(&n) {
                header.push((format!("{number} "), step_color));
            }
            header.push((s.name.clone(), TEXT));
            if off_path {
                header.push(("  off path".into(), WEAK));
            }
            let (rlo, rhi) = self.graph.range[&n];
            header.push((format!("  {}:{}-{}", f.path, rlo + 1, rhi + 1), WEAK));
            // buttons, right-aligned in the header
            let n_callees = self.graph.callees_of(&self.idx, n).len();
            let mut labels: Vec<(Btn, String)> = Vec::new();
            if total > PREVIEW_LINES {
                labels.push((Btn::Less, if shown < total { "more".into() } else { "less".into() }));
            }
            labels.push((Btn::Listing, "listing".into()));
            if n_callees > 0 {
                labels.push((Btn::Callees, format!("callees > {n_callees}")));
            }
            if !s.callers.is_empty() {
                labels.push((Btn::Callers, format!("{} < callers", s.callers.len())));
            }
            let mut buttons = Vec::new();
            let mut button_hits = Vec::new();
            let mut bx = rect.right() - m.pad;
            for (b, label) in labels.into_iter().rev() {
                let bw = (label.chars().count() as i32 + 2) * m.cell_w;
                bx -= bw + 4;
                let br = Rect::new(bx, rect.y + m.pad, bw, m.row_h);
                let hovered = br.contains(mouse.0, mouse.1);
                button_hits.push((br, Hit::Button(n, b)));
                buttons.push((br, label, hovered));
            }
            let header_rect = Rect::new(rect.x, rect.y, (bx - rect.x).max(0), m.row_h + m.pad);
            let note = n.1.and_then(|ai| self.graph.path_id.map(|pi| self.map.paths[pi].anchors[ai].note.clone())).filter(|s| !s.is_empty());
            let code_top = rect.y + m.pad + m.row_h + if note.is_some() { m.row_h } else { 0 } + 4;
            self.graph.code_top.insert(n, code_top);
            let mut lines = Vec::new();
            for (k, li) in (lo..lo + shown).enumerate() {
                let lr = Rect::new(rect.x + m.pad, code_top + k as i32 * m.row_h, rect.w - m.pad * 2, m.row_h);
                hits.push((lr, Hit::Line(n, li)));
                lines.push((li, gui::code_runs(f, li, true)));
            }
            hits.extend(button_hits);
            // the lines that call the nodes hanging off this one
            let children: Vec<Node> = self.graph.nodes.iter().copied().filter(|c| self.graph.step_parent.get(c) == Some(&n) || self.graph.origin.get(c).is_some_and(|(o, callees)| *o == n && *callees)).collect();
            let tinted: HashSet<usize> = children.iter().filter_map(|c| self.graph.call_line(&self.idx, n, &self.idx.sym(c.0).name)).filter(|&li| li < lo + shown).collect();
            // later hits win: body, then the header, then lines, then the buttons
            let mut ordered = vec![(rect, Hit::Body(n)), (header_rect, Hit::Header(n))];
            ordered.extend(hits.drain(node_hits_from..));
            hits.extend(ordered);
            nodes.push(SceneNode { rect, fill, border, border_w, header, buttons, note, code_top, lines, tinted, more: (shown < total).then_some(total - shown) });
        }
        // edges: where an edge leaves a node is level with the call line when it is shown
        let rect_of = |g: &Graph, n: Node| {
            let (x, y) = g.pos[&n];
            let (w, h) = g.size[&n];
            Rect::new(ox + x, oy + y, w, h)
        };
        let edge_out = |g: &Graph, n: Node, word: &str| -> (f32, f32) {
            let r = rect_of(g, n);
            let (lo, _) = g.range[&n];
            let (shown, _) = g.lines_shown(n);
            match g.call_line(&self.idx, n, word) {
                Some(li) if li < lo + shown => (r.right() as f32, (g.code_top[&n] + (li - lo) as i32 * m.row_h + m.row_h / 2) as f32),
                _ => (r.right() as f32, (r.y + m.pad + m.row_h / 2) as f32),
            }
        };
        let mut edges = Vec::new();
        let hy = (m.pad + m.row_h / 2) as f32;
        for &a in &self.graph.nodes {
            for bs in self.graph.callees_of(&self.idx, a) {
                let Some(&b) = self.graph.by_sym.get(&bs) else { continue };
                if a.0 == bs || self.graph.step_parent.get(&b) == Some(&a) {
                    continue;
                }
                let (ra, rb) = (rect_of(&self.graph, a), rect_of(&self.graph, b));
                if rb.x >= ra.right() {
                    let p0 = edge_out(&self.graph, a, &self.idx.sym(bs).name);
                    let p1 = (rb.x as f32, rb.y as f32 + hy);
                    let dx = ((p1.0 - p0.0).abs() * 0.5).max(m.gap_x as f32 * 0.8);
                    edges.push(([p0, (p0.0 + dx, p0.1), (p1.0 - dx, p1.1), p1], WEAK, 1.5, true));
                } else {
                    let p0 = (ra.x as f32, ra.y as f32 + hy);
                    let p1 = (rb.right() as f32, rb.y as f32 + hy);
                    let dx = ((p0.0 - p1.0).abs() * 0.5).max(m.gap_x as f32 * 0.8);
                    edges.push(([p0, (p0.0 - dx, p0.1), (p1.0 + dx, p1.1), p1], ORANGE, 1.5, true));
                }
            }
        }
        for (&child, &parent) in &self.graph.step_parent {
            let rb = rect_of(&self.graph, child);
            let p0 = edge_out(&self.graph, parent, &self.idx.sym(child.0).name);
            let p1 = (rb.x as f32, rb.y as f32 + hy);
            let dx = ((p1.0 - p0.0).abs() * 0.5).max(m.gap_x as f32 * 0.8);
            edges.push(([p0, (p0.0 + dx, p0.1), (p1.0 - dx, p1.1), p1], step_color, 3.0, true));
        }
        self.graph.hits = hits.into_iter().map(|(r, h)| (r.intersect(&canvas), h)).collect();
        Scene { px, m, edges, nodes }
    }
}

fn draw_scene(gfx: &mut Gfx, _r: Rect, scene: &Scene) {
    let m = scene.m;
    for (p, color, w, arrow) in &scene.edges {
        gfx.curve(*p, *w, *color);
        if *arrow {
            gfx.circle(p[3].0, p[3].1, 3.5, *color);
        }
    }
    for n in &scene.nodes {
        gfx.rect(n.rect, n.fill);
        gfx.rect_outline(n.rect, n.border_w, n.border);
        gfx.push_clip(n.rect.shrink(1));
        let mut pen = n.rect.x + m.pad;
        let y = n.rect.y + m.pad;
        for (s, c) in &n.header {
            pen = gfx.text(pen, y, scene.px, s, *c);
        }
        for (br, label, hovered) in &n.buttons {
            gfx.rect(*br, if *hovered { HOVER } else { PANEL });
            gfx.rect_outline(*br, 1, BORDER);
            gfx.text(br.x + m.cell_w, br.y, scene.px, label, if *hovered { TEXT } else { WEAK });
        }
        if let Some(note) = &n.note {
            gfx.text(n.rect.x + m.pad, y + m.row_h, scene.px, note, GREEN);
        }
        gfx.rect(Rect::new(n.rect.x + m.pad, n.code_top - 2, n.rect.w - m.pad * 2, 1), BORDER);
        for (k, (li, runs)) in n.lines.iter().enumerate() {
            let ly = n.code_top + k as i32 * m.row_h;
            if n.tinted.contains(li) {
                gfx.rect(Rect::new(n.rect.x + m.pad, ly, n.rect.w - m.pad * 2, m.row_h), gui::dim(GREEN, 46));
            }
            let mut pen = n.rect.x + m.pad;
            for (s, c) in runs {
                pen = gfx.text(pen, ly, scene.px, s, *c);
            }
        }
        if let Some(more) = n.more {
            gfx.text(n.rect.x + m.pad, n.code_top + n.lines.len() as i32 * m.row_h, scene.px, &format!("      … {more} more lines"), WEAK);
        }
        gfx.pop_clip();
    }
}

impl App {
    /// The floating definition under the pointer, drawn over everything.
    pub fn tooltip_element(&mut self) {
        let Some((r, (mx, my))) = self.tooltip.take() else { return };
        if !self.ui.input.down[0] && r.file < self.idx.files.len() {
            let s = self.idx.sym(r);
            let (name, place, start, end, s_end) = (s.name.clone(), format!("{} {}:{}-{}", s.kind, self.idx.files[r.file].path, s.start + 1, s.end + 1), s.start, s.end, s.end);
            let px = self.px;
            let (w, h) = (self.ui.size.0, self.ui.size.1);
            let x = (mx + 16).min(w - 90 * self.cell.0).max(0);
            let y = (my + 16).min(h - 30 * self.cell.1).max(0);
            self.ui.open(Kind::None, Layout::col().floating(x, y).pad(6).gap(2), Style::bg(PANEL).border(ui::BORDER_ALL, BORDER), None);
            self.label(&name, TEXT);
            self.label(&place, WEAK);
            let end = end.min(self.idx.files[r.file].lines.len().saturating_sub(1)).min(start + 23);
            for li in start..=end {
                let runs = gui::code_runs(&self.idx.files[r.file], li, true);
                self.ui.leaf(Kind::Text(ui::Text { runs, px, wrap: false }), Layout::row(), Style::default(), None);
            }
            if end < s_end {
                self.label(&format!("      … {} more lines", s_end - end), WEAK);
            }
            self.label("alt-click: pin in the peek panel   ctrl-click or double-click: go there", WEAK);
            self.ui.close();
        }
    }
}
