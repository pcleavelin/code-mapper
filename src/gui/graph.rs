//! The Graph tab: the selection as a left-to-right tree of nodes showing their code, derived
//! every frame from the selected path (or the selected symbol alone) plus the expansions the
//! reader opened. The tree is laid out in cells and rows of the node font and drawn at the
//! cell size of the current zoom, so text is drawn at a whole pixel size and never scaled, and
//! zooming scales positions by exactly the factor the text grew by. Everything is drawn by one
//! custom element; clicks are resolved against the rectangles the last frame recorded.

use super::{
    ACCENT, Action, App, BG, BORDER, CONTEXT_LINES, FIELD, GREEN, GUTTER, HOVER, ORANGE, PANEL,
    RED, SELECTED, TEXT, WEAK, dim,
};
use crate::gfx::{Color, Gfx, Glyphs, Rect};
use crate::index::{self, Index, SymRef};
use crate::map::Map;
use crate::ui::{self, Kind, Layout, Measure, Style};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// A graph node: a symbol, and the step it stands for when it is one. Two slice steps of one
/// symbol are two nodes; an expansion that reveals a symbol some step already shows reuses
/// that step's node.
pub type Node = (SymRef, Option<usize>);

/// A node across a re-index: (file path, symbol name) instead of the symbol's index.
type NodeKey = ((String, String), Option<usize>);

/// The graph state that is the reader's, not the index's, held while the index is rebuilt.
pub struct Saved {
    expansions: Vec<(NodeKey, bool)>,
    collapsed: Vec<NodeKey>,
    context: Vec<(NodeKey, (usize, usize))>,
    manual: Vec<(NodeKey, (f32, f32))>,
    auto_open: Option<NodeKey>,
}

const PREVIEW_LINES: usize = 12;
const MIN_COLS: i32 = 44;
const MAX_COLS: i32 = 110;
const GAP_X: i32 = 12; // cells between columns
const GAP_Y: i32 = 2; // rows between stacked subtrees
const WHEEL_NOTCH: f32 = 40.0; // pixels one wheel notch reports
const PAN_ROWS: i32 = 3; // rows the wheel pans per notch
const MARGIN: i32 = 8; // pixels kept between the canvas edge and what a camera move brings in

/// The node font's cell at the current zoom. Layout happens in cells and rows; these turn it
/// into pixels.
#[derive(Clone, Copy)]
struct Metrics {
    cell_w: i32,
    row_h: i32,
    pad: i32, // inside a node, in pixels
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Btn {
    Less,
    Listing,
    Callees,
    Callers,
    Above, // more context above the slice
    Below, // more context below
    NoContext,
}

/// A rectangle of last frame's scene the mouse can land on, in window pixels.
#[derive(Clone, Copy)]
enum Hit {
    Header(Node),
    Button(Node, Btn),
    Line(Node, usize), // a code line: the file line index
    Body(Node),
}

#[derive(Clone, Copy)]
enum Drag {
    Pan,
    Node(Node),
}

#[derive(Default)]
pub struct Graph {
    // The tree: rebuilt every frame from the selection and the reader's state below, and read
    // as last frame's by the next frame's input.
    nodes: Vec<Node>,
    col: HashMap<Node, i32>,
    pos: HashMap<Node, (i32, i32)>,       // in cells and rows
    size: HashMap<Node, (i32, i32)>,      // in cells and rows
    cell: (i32, i32),                     // the cell width and row height, in pixels
    range: HashMap<Node, (usize, usize)>, // a node's own lines: the step's slice, or the whole symbol
    view: HashMap<Node, (usize, usize)>,  // range widened by context, clamped to the file
    by_sym: HashMap<SymRef, Node>,
    path_id: Option<usize>,
    step: HashMap<Node, (usize, usize, String)>, // step node -> (pre-order position, anchor index, hierarchical number)
    step_parent: HashMap<Node, Node>,
    origin: HashMap<Node, (Node, bool)>, // expansion node -> (the node that revealed it, via callees?)
    code_top: HashMap<Node, i32>,        // y of each node's first code row
    has_note: HashSet<Node>,             // step nodes with a note row
    hits: Vec<(Rect, Hit)>,              // what the mouse can land on, in window pixels

    // The reader's: what they opened, cut, widened and dragged, and the camera.
    expansions: Vec<(Node, bool)>,
    auto_open: Option<Node>, // the off-path root whose callers and callees were opened for it
    collapsed: HashSet<Node>, // nodes cut to a preview
    context: HashMap<Node, (usize, usize)>, // extra lines shown above and below, asked for with the node's buttons
    manual: HashMap<Node, (f32, f32)>,      // dragged positions, in cells and rows
    drag: Option<Drag>,
    pub pan: (i32, i32),
    pub zoom: f32,
    pub want_look: bool, // centre on the focus once it has a position
    pub hold_look: bool, // the next focus change came from the graph itself: do not move the camera
    want_fit: bool,      // zoom out and pan so the whole tree is on the canvas
    keep: Option<(Node, (i32, i32))>, // a node and its pixel position before the relayout its button caused
}

impl Graph {
    pub fn new() -> Graph {
        Graph {
            zoom: 1.0,
            ..Default::default()
        }
    }

    /// Everything the reader built on top of the selection, keyed by symbol name instead of
    /// symbol index, so it survives a re-index that renumbers the symbols of a file.
    pub fn save(&self, idx: &Index) -> Saved {
        let key = |n: &Node| (idx.key(n.0), n.1);
        Saved {
            expansions: self.expansions.iter().map(|(n, c)| (key(n), *c)).collect(),
            collapsed: self.collapsed.iter().map(key).collect(),
            context: self.context.iter().map(|(n, v)| (key(n), *v)).collect(),
            manual: self.manual.iter().map(|(n, v)| (key(n), *v)).collect(),
            auto_open: self.auto_open.as_ref().map(key),
        }
    }

    /// Put it back on the new index, dropping whatever no longer has a symbol. The camera is
    /// not touched: a re-index is not a navigation.
    pub fn restore(&mut self, idx: &Index, s: Saved) {
        let node = |k: &NodeKey| idx.by_key(&k.0).map(|r| (r, k.1));
        self.expansions = s
            .expansions
            .iter()
            .filter_map(|(k, c)| Some((node(k)?, *c)))
            .collect();
        self.collapsed = s.collapsed.iter().filter_map(node).collect();
        self.context = s
            .context
            .iter()
            .filter_map(|(k, v)| Some((node(k)?, *v)))
            .collect();
        self.manual = s
            .manual
            .iter()
            .filter_map(|(k, v)| Some((node(k)?, *v)))
            .collect();
        self.auto_open = s.auto_open.as_ref().and_then(node);
    }

    /// What the camera still owes the reader, for dumps: a pending fit, look or keep.
    pub fn camera_state(&self) -> String {
        format!(
            "fit={} look={} keep={}",
            self.want_fit,
            self.want_look,
            self.keep.is_some()
        )
    }

    pub fn drag_state(&self) -> String {
        match self.drag {
            None => "none".into(),
            Some(Drag::Pan) => "pan".into(),
            Some(Drag::Node(_)) => "node".into(),
        }
    }

    /// Every node button's node name, drawn label and last frame's window rectangle.
    pub fn button_rects(&self, idx: &Index) -> Vec<(String, String, Rect)> {
        self.hits
            .iter()
            .filter_map(|(r, h)| match h {
                Hit::Button(n, b) => {
                    let label = self
                        .header(idx, *n)
                        .1
                        .into_iter()
                        .find(|(k, _)| k == b)
                        .map(|(_, l)| l)?;
                    Some((idx.sym(n.0).name.clone(), label, *r))
                }
                _ => None,
            })
            .collect()
    }

    /// Every node's name (with its step number) and last frame's window rectangle.
    pub fn node_rects(&self, idx: &Index) -> Vec<(String, Rect)> {
        self.hits
            .iter()
            .filter_map(|(r, h)| match h {
                Hit::Body(n) => Some((
                    format!(
                        "{}{}",
                        self.step
                            .get(n)
                            .map(|s| format!("{} ", s.2))
                            .unwrap_or_default(),
                        idx.sym(n.0).name
                    ),
                    *r,
                )),
                _ => None,
            })
            .collect()
    }
}

impl Graph {
    /// `n`'s rectangle in canvas pixels, offset by `o`; None when `n` is not in this frame's tree.
    fn rect_px(&self, n: Node, o: (i32, i32)) -> Option<Rect> {
        if !self.col.contains_key(&n) {
            return None;
        }
        let ((x, y), (w, h), (cw, rh)) = (self.pos[&n], self.size[&n], self.cell);
        Some(Rect::new(o.0 + x * cw, o.1 + y * rh, w * cw, h * rh))
    }

    fn focus_node(&self, focus: Option<SymRef>, focus_step: Option<usize>) -> Option<Node> {
        let f = focus?;
        let stepped = (f, focus_step);
        if self.col.contains_key(&stepped) {
            Some(stepped)
        } else {
            self.by_sym.get(&f).copied()
        }
    }

    fn lines_shown(&self, n: Node) -> (usize, usize) {
        let (lo, hi) = self.view[&n];
        let total = hi - lo + 1;
        (
            if self.collapsed.contains(&n) {
                total.min(PREVIEW_LINES)
            } else {
                total
            },
            total,
        )
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
        (lo..=hi.min(f.lines.len().saturating_sub(1))).find(|&li| index::call_site(f, li, word))
    }

    /// What `n` calls: the symbol's callees, narrowed to the ones its lines name when the node
    /// is a slice of the symbol.
    fn callees_of(&self, idx: &Index, n: Node) -> Vec<SymRef> {
        let s = idx.sym(n.0);
        if self.range[&n] == (s.start, s.end) {
            return s.callees.clone();
        }
        s.callees
            .iter()
            .copied()
            .filter(|c| self.call_line(idx, n, &idx.sym(*c).name).is_some())
            .collect()
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
            // a step that gets no node passes its parent on, so its children stay in the tree
            let mut node_of: HashMap<usize, Node> = HashMap::new();
            for (k, (ai, depth, number)) in map.numbered(idx, pi).into_iter().enumerate() {
                let a = &anchors[ai];
                let parent = a.parent.and_then(|p| node_of.get(&p).copied());
                let mut skip = || {
                    if let Some(p) = parent {
                        node_of.insert(ai, p);
                    }
                };
                let Some(fi) = idx.find_file(&a.file) else {
                    skip();
                    continue;
                };
                let si = match a.sym {
                    Some(si) => Some(si),
                    // a lines-only anchor takes the symbol its first line is inside, else the
                    // file's first symbol; a file with no symbols at all has no node
                    None if a.symbol.is_empty() => idx
                        .by_line(&a.file, a.line_start)
                        .map(|r| r.sym)
                        .or(Some(0))
                        .filter(|&si| si < idx.files[fi].symbols.len()),
                    None => None,
                };
                let Some(si) = si else {
                    skip();
                    continue;
                };
                let n = (SymRef { file: fi, sym: si }, Some(ai));
                self.add(idx, n, depth as i32, (a.line_start, a.line_end));
                self.step.insert(n, (k, ai, number));
                if !a.note.is_empty() {
                    self.has_note.insert(n);
                }
                node_of.insert(ai, n);
                if let Some(p) = parent {
                    self.step_parent.insert(n, p);
                }
            }
        }
        if let Some(f) = focus
            && !self.by_sym.contains_key(&f)
        {
            let s = idx.sym(f);
            self.add(idx, (f, None), 0, (s.start, s.end)); // an off-path selection is its own root
            // it arrives with its neighbourhood open, once: closing them again sticks
            if self.auto_open != Some((f, None)) {
                self.auto_open = Some((f, None));
                for callees in [true, false] {
                    if !self.expansions.contains(&((f, None), callees)) {
                        self.expansions.push(((f, None), callees));
                    }
                }
            }
        }
        for (n, callees) in self.expansions.clone() {
            let Some(&col) = self.col.get(&n) else {
                continue;
            };
            let s = idx.sym(n.0);
            let list = if callees {
                self.callees_of(idx, n)
            } else {
                s.callers.clone()
            };
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
        self.context.retain(|n, _| self.col.contains_key(n));
        self.view.clear();
        for &n in &self.nodes {
            let (lo, hi) = self.range[&n];
            // a preview is of the node's own lines: context waits until the node is opened
            let (a, b) = if self.collapsed.contains(&n) {
                (0, 0)
            } else {
                self.context.get(&n).copied().unwrap_or((0, 0))
            };
            let last = idx.files[n.0.file].lines.len().saturating_sub(1);
            self.view
                .insert(n, (lo.saturating_sub(a), (hi + b).min(last)));
        }
    }

    fn toggle(&mut self, n: Node, callees: bool) {
        self.keep(n);
        match self.expansions.iter().position(|e| *e == (n, callees)) {
            Some(i) => {
                self.expansions.remove(i);
            }
            None => self.expansions.push((n, callees)),
        }
        self.manual.remove(&n);
    }

    /// The column of the file line under the pointer, or None over the line-number gutter.
    /// The grid a node draws is the one `code_block` draws, so the gutter is the same width.
    fn code_col(&self, cell_w: i32, n: Node, li: usize, mouse: (i32, i32)) -> Option<usize> {
        let rect = self
            .hits
            .iter()
            .find(|(_, h)| matches!(h, Hit::Line(m, l) if *m == n && *l == li))
            .map(|(r, _)| *r)?;
        let col = ((mouse.0 - rect.x) / cell_w.max(1)).max(0) as usize;
        col.checked_sub(GUTTER)
    }

    /// Keep `n` at its screen position through the relayout its button causes, so the tree
    /// growing or shrinking around it does not slide it out from under the pointer.
    fn keep(&mut self, n: Node) {
        self.keep = self.rect_px(n, (0, 0)).map(|r| (n, (r.x, r.y)));
    }

    /// Keep `n` where it is when its own size is about to change, so the button under the
    /// pointer does not move.
    fn pin(&mut self, n: Node) {
        if let Some(&(x, y)) = self.pos.get(&n) {
            self.manual.insert(n, (x as f32, y as f32));
        }
    }

    /// A node's header runs and the buttons the header carries, left to right.
    fn header(&self, idx: &Index, n: Node) -> (Vec<(String, Color)>, Vec<(Btn, String)>) {
        let s = idx.sym(n.0);
        let f = &idx.files[n.0.file];
        let (lo, hi) = self.view[&n];
        let (shown, total) = self.lines_shown(n);
        let off_path = self.path_id.is_some() && !self.step.contains_key(&n);
        let mut header = Vec::new();
        if let Some((_, _, number)) = self.step.get(&n) {
            header.push((format!("{number} "), GREEN));
        }
        header.push((s.name.clone(), TEXT));
        if off_path {
            header.push(("  off path".into(), WEAK));
        }
        let (rlo, rhi) = self.range[&n];
        header.push((format!("  {}:{}-{}", f.path, rlo + 1, rhi + 1), WEAK));
        let mut labels: Vec<(Btn, String)> = Vec::new();
        if total > PREVIEW_LINES {
            labels.push((
                Btn::Less,
                if shown < total {
                    "more".into()
                } else {
                    "less".into()
                },
            ));
        }
        if !self.collapsed.contains(&n) {
            if lo > 0 {
                labels.push((Btn::Above, "▲".into()));
            }
            if hi + 1 < f.lines.len() {
                labels.push((Btn::Below, "▼".into()));
            }
            if self.context.contains_key(&n) {
                labels.push((Btn::NoContext, "no context".into()));
            }
        }
        labels.push((Btn::Listing, "listing".into()));
        // an expansion that is open says so and closes on a click; a closed one that would
        // reveal nothing new is not offered at all
        let hidden = |list: &[SymRef]| list.iter().filter(|r| !self.by_sym.contains_key(r)).count();
        let callees = self.callees_of(idx, n);
        if self.expansions.contains(&(n, true)) && !callees.is_empty() {
            labels.push((Btn::Callees, format!("hide {} callees", callees.len())));
        } else if hidden(&callees) > 0 {
            labels.push((Btn::Callees, format!("callees > {}", hidden(&callees))));
        }
        if self.expansions.contains(&(n, false)) && !s.callers.is_empty() {
            labels.push((Btn::Callers, format!("hide {} callers", s.callers.len())));
        } else if hidden(&s.callers) > 0 {
            labels.push((Btn::Callers, format!("{} < callers", hidden(&s.callers))));
        }
        (header, labels)
    }

    /// Cells the header needs: its text, then every button with its padding and gap.
    fn header_cols(&self, idx: &Index, n: Node) -> i32 {
        let (header, labels) = self.header(idx, n);
        let text: usize = header.iter().map(|(s, _)| s.chars().count()).sum();
        let buttons: usize = labels.iter().map(|(_, l)| l.chars().count() + 3).sum();
        (text + buttons + 2) as i32
    }

    /// A node is as wide as its longest shown line within limits, and never narrower than
    /// its header, so the buttons always sit inside it.
    fn node_size(&self, idx: &Index, n: Node) -> (i32, i32) {
        let (lo, _) = self.view[&n];
        let (shown, total) = self.lines_shown(n);
        let f = &idx.files[n.0.file];
        let longest = f.lines[lo..lo + shown]
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0) as i32
            + 6;
        let cols = longest
            .clamp(MIN_COLS, MAX_COLS)
            .max(self.header_cols(idx, n));
        let note_rows = i32::from(self.has_note.contains(&n));
        let rows = 1 + note_rows + shown as i32 + usize::from(shown < total) as i32;
        (cols + 1, rows + 1) // one cell and one row of padding
    }

    /// Recompute every position as a left-to-right forest: a column per depth anchored at
    /// column 0, each subtree stacked beside its parent, children in the order the parent's
    /// code calls them.
    fn layout(&mut self, idx: &Index) {
        if self.nodes.is_empty() {
            return;
        }
        let mut cols: BTreeMap<i32, Vec<Node>> = BTreeMap::new();
        for &n in &self.nodes {
            cols.entry(self.col[&n]).or_default().push(n);
        }
        let (first, last) = (*cols.keys().next().unwrap(), *cols.keys().last().unwrap());
        let anchor = 0.clamp(first, last);
        let col_w: BTreeMap<i32, i32> = cols
            .iter()
            .map(|(&c, ns)| (c, ns.iter().map(|&n| self.size[&n].0).max().unwrap_or(0)))
            .collect();
        let mut col_x: BTreeMap<i32, i32> = BTreeMap::new();
        col_x.insert(anchor, 0);
        for c in (anchor + 1)..=last {
            col_x.insert(c, col_x[&(c - 1)] + col_w[&(c - 1)] + GAP_X);
        }
        for c in (first..anchor).rev() {
            col_x.insert(c, col_x[&(c + 1)] - col_w[&c] - GAP_X);
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
                if callees {
                    right.entry(o).or_default()
                } else {
                    left.entry(o).or_default()
                }
                .push(n);
                has_parent.insert(n);
            }
        }
        for (&p, kids) in right.iter_mut() {
            kids.sort_by_key(|k| {
                (
                    self.call_line(idx, p, &idx.sym(k.0).name)
                        .unwrap_or(usize::MAX),
                    self.step.get(k).map_or(usize::MAX, |s| s.0),
                )
            });
        }
        let mut roots: Vec<Node> = self
            .nodes
            .iter()
            .copied()
            .filter(|n| !has_parent.contains(n))
            .collect();
        roots.sort_by_key(|r| self.step.get(r).map_or(usize::MAX, |s| s.0));

        // block heights, bottom-up
        let mut height: HashMap<Node, i32> = HashMap::new();
        fn measure(
            g: &Graph,
            r: Node,
            right: &HashMap<Node, Vec<Node>>,
            left: &HashMap<Node, Vec<Node>>,
            height: &mut HashMap<Node, i32>,
            seen: &mut HashSet<Node>,
        ) -> i32 {
            if !seen.insert(r) {
                return 0;
            }
            let stack = |kids: Option<&Vec<Node>>,
                         height: &mut HashMap<Node, i32>,
                         seen: &mut HashSet<Node>|
             -> i32 {
                let mut h = 0;
                for &k in kids.into_iter().flatten() {
                    let kh = measure(g, k, right, left, height, seen);
                    if kh > 0 {
                        h += kh + GAP_Y;
                    }
                }
                (h - GAP_Y).max(0)
            };
            let rh = stack(right.get(&r), height, seen);
            let lh = stack(left.get(&r), height, seen);
            let h = g.size[&r].1.max(rh).max(lh);
            height.insert(r, h);
            h
        }
        let mut seen = HashSet::new();
        for &r in &roots {
            measure(self, r, &right, &left, &mut height, &mut seen);
        }
        // place, top-down
        fn place(
            g: &mut Graph,
            r: Node,
            top: i32,
            col_x: &BTreeMap<i32, i32>,
            right: &HashMap<Node, Vec<Node>>,
            left: &HashMap<Node, Vec<Node>>,
            height: &HashMap<Node, i32>,
            done: &mut HashSet<Node>,
        ) {
            if !done.insert(r) {
                return;
            }
            let block = height[&r];
            let own = g.size[&r].1;
            g.pos
                .insert(r, (col_x[&g.col[&r]], top + (block - own) / 2));
            for kids in [right.get(&r), left.get(&r)] {
                let kids: Vec<Node> = kids
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|k| height.contains_key(k) && !done.contains(k))
                    .collect();
                let stack_h: i32 = kids.iter().map(|k| height[k] + GAP_Y).sum::<i32>() - GAP_Y;
                let mut cur = top + (block - stack_h.max(0)) / 2;
                for k in kids {
                    place(g, k, cur, col_x, right, left, height, done);
                    cur += height[&k] + GAP_Y;
                }
            }
        }
        let mut done = HashSet::new();
        let mut cur = 0;
        for &r in &roots {
            if done.contains(&r) {
                continue;
            }
            place(self, r, cur, &col_x, &right, &left, &height, &mut done);
            cur += height[&r] + GAP_Y * 2;
        }
        // different subtrees can still meet in one column; push the later one down
        for ns in cols.values_mut() {
            ns.sort_by_key(|n| self.pos[n].1);
            let mut prev_bottom = i32::MIN / 2;
            for &n in ns.iter() {
                let h = self.size[&n].1;
                let y = self.pos[&n].1.max(prev_bottom + GAP_Y);
                self.pos.get_mut(&n).unwrap().1 = y;
                prev_bottom = y + h;
            }
        }
    }
}

impl Graph {
    /// The bounding box of the whole tree in cells and rows, which no zoom changes.
    fn bbox_cells(&self) -> Option<(i32, i32, i32, i32)> {
        let mut b: Option<(i32, i32, i32, i32)> = None;
        for n in &self.nodes {
            let (Some(&(x, y)), Some(&(w, h))) = (self.pos.get(n), self.size.get(n)) else {
                continue;
            };
            b = Some(match b {
                None => (x, y, x + w, y + h),
                Some((l, t, r, bo)) => (l.min(x), t.min(y), r.max(x + w), bo.max(y + h)),
            });
        }
        b.map(|(l, t, r, bo)| (l, t, r - l, bo - t))
    }

    /// The camera moves only on an explicit request: a new focus or the fit button. A node
    /// whose button changed the tree around it is held at its screen position, so the pan
    /// follows the relayout and the pointer stays over the button it pressed.
    fn move_camera(&mut self, canvas: Rect, focus_node: Option<Node>) {
        if self.want_look {
            if let Some(Rect { x, y, w, h }) = focus_node.and_then(|f| self.rect_px(f, (0, 0))) {
                // a node taller than the canvas is aligned to its top: its header and buttons
                // are what the reader came for
                let y = if h + MARGIN * 2 > canvas.h {
                    MARGIN - y
                } else {
                    canvas.h / 2 - y - h / 2
                };
                self.pan = (canvas.w / 2 - x - w / 2, y);
                self.want_look = false;
            }
            return;
        }
        if let Some((n, (ox, oy))) = self.keep.take()
            && let Some(Rect { x, y, .. }) = self.rect_px(n, (0, 0))
        {
            self.pan = (self.pan.0 + ox - x, self.pan.1 + oy - y);
        }
    }
}

/// Everything the custom element draws, in window pixels.
struct Scene {
    px: u32,
    m: Metrics,
    edges: Vec<([(f32, f32); 4], Color, f32)>, // curve, colour, width; each ends in an arrow head
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
    code: Rc<Glyphs>, // the shown lines, the first being file line `lo`
    lo: usize,
    tinted: HashSet<usize>,
    slice: Option<(usize, usize)>, // the node's own lines, highlighted when context shows around them
    more: Option<usize>,
}

impl App {
    /// The graph tab: input from last frame's rectangles, then the tree rebuilt, laid out and
    /// turned into a scene that a custom element draws.
    pub fn graph_tab(&mut self, gfx: &mut Gfx) {
        let canvas_id = ui::id("graph-canvas");
        let it = self.ui.interaction_of(canvas_id);
        let canvas = it.rect.unwrap_or(Rect::new(0, 0, 100, 100));
        let canvas_known = it.rect.is_some();
        let mouse = self.ui.input.mouse;
        let mods = self.ui.input.mods;

        // zoom around the mouse with ctrl+wheel, pan with the wheel
        if it.wheel.1 != 0.0 && (it.hovered || self.graph.drag.is_some()) {
            if mods.ctrl {
                let old = self.graph.zoom;
                let new = (old * if it.wheel.1 > 0.0 { 1.1 } else { 1.0 / 1.1 }).clamp(0.3, 2.0);
                // keep the cell under the mouse still: positions scale by the cell, not the zoom
                let (oc, or) = gfx.cell(self.graph_px());
                self.graph.zoom = new;
                let (nc, nr) = gfx.cell(self.graph_px());
                let (mx, my) = ((mouse.0 - canvas.x) as f32, (mouse.1 - canvas.y) as f32);
                let (ux, uy) = (
                    (mx - self.graph.pan.0 as f32) / oc as f32,
                    (my - self.graph.pan.1 as f32) / or as f32,
                );
                self.graph.pan = (
                    (mx - ux * nc as f32).round() as i32,
                    (my - uy * nr as f32).round() as i32,
                );
            } else {
                let per_notch = (gfx.cell(self.graph_px()).1 * PAN_ROWS) as f32;
                let step = |d: f32| (d / WHEEL_NOTCH * per_notch).round() as i32;
                if mods.shift {
                    self.graph.pan.0 += step(it.wheel.1);
                } else {
                    self.graph.pan.1 += step(it.wheel.1);
                    self.graph.pan.0 += step(it.wheel.0);
                }
            }
        }
        // clicks and drags against last frame's hit rectangles
        let hit = self
            .graph
            .hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(mouse.0, mouse.1))
            .map(|(_, h)| *h);
        if it.clicked {
            match hit {
                Some(Hit::Button(n, b)) => match b {
                    Btn::Less => {
                        self.graph.keep(n);
                        if !self.graph.collapsed.remove(&n) {
                            self.graph.collapsed.insert(n);
                        }
                        self.graph.manual.remove(&n);
                    }
                    Btn::Listing => self
                        .actions
                        .push(Action::GoTo(n.0.file, self.graph.range[&n].0)),
                    Btn::Callees => self.graph.toggle(n, true),
                    Btn::Callers => self.graph.toggle(n, false),
                    Btn::Above => {
                        self.graph.context.entry(n).or_default().0 += CONTEXT_LINES;
                        self.graph.pin(n);
                    }
                    Btn::Below => {
                        self.graph.context.entry(n).or_default().1 += CONTEXT_LINES;
                        self.graph.pin(n);
                    }
                    Btn::NoContext => {
                        self.graph.context.remove(&n);
                        self.graph.pin(n);
                    }
                },
                Some(Hit::Header(n)) => {
                    self.graph.drag = Some(Drag::Node(n));
                    self.actions.push(match (n.1, self.graph.path_id) {
                        (Some(ai), Some(pi)) => Action::SelectStep(pi, ai, false),
                        _ => Action::Focus(n.0),
                    });
                    self.graph.hold_look = true; // selecting from the graph must not move the camera
                }
                Some(Hit::Line(n, li)) => {
                    self.graph.drag = Some(Drag::Node(n)); // dragging code moves the node without selecting it
                    if let Some(col) =
                        self.graph
                            .code_col(gfx.cell(self.graph_px()).0, n, li, mouse)
                    {
                        if mods.alt {
                            self.actions.push(Action::PeekAt(n.0.file, li, col));
                        } else if mods.ctrl || it.double_clicked {
                            self.actions.push(Action::Jump(n.0.file, li, col));
                        }
                    }
                }
                Some(Hit::Body(n)) => self.graph.drag = Some(Drag::Node(n)),
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
                    let (cw, rh) = gfx.cell(self.graph_px());
                    let base = self.graph.pos.get(&n).copied().unwrap_or((0, 0));
                    let cur = self
                        .graph
                        .manual
                        .get(&n)
                        .copied()
                        .unwrap_or((base.0 as f32, base.1 as f32));
                    self.graph.manual.insert(
                        n,
                        (
                            cur.0 + d.0 as f32 / cw.max(1) as f32,
                            cur.1 + d.1 as f32 / rh.max(1) as f32,
                        ),
                    );
                }
                None => {}
            }
        }
        if !it.down {
            self.graph.drag = None;
        }
        if let Some(Hit::Line(n, li)) = hit.filter(|_| it.hovered && !self.ui.input.down[0])
            && let Some(col) = self
                .graph
                .code_col(gfx.cell(self.graph_px()).0, n, li, mouse)
            && let Some(t) = self.probe(n.0.file, li, col)
        {
            self.tooltip = Some((t, mouse));
        }

        // the tree for this selection: the whole selected path, whatever is selected inside it
        self.graph
            .rebuild(&self.idx, &self.map, self.sel_path, self.focus);
        let px = self.graph_px();
        for &n in &self.graph.nodes.clone() {
            let s = self.graph.node_size(&self.idx, n);
            self.graph.size.insert(n, s);
        }
        self.graph.layout(&self.idx);
        for (n, p) in self.graph.manual.clone() {
            self.graph
                .pos
                .insert(n, (p.0.round() as i32, p.1.round() as i32));
        }
        // fit: the layout is in cells, so the largest font at which the tree fits the canvas
        // settles both the zoom and the pan in one frame
        if self.graph.want_fit && canvas_known {
            self.graph.want_fit = false;
            if let Some((x, y, w, h)) = self.graph.bbox_cells() {
                let smallest = ((self.px as f32) * 0.3).round().max(6.0) as u32;
                let mut size = px;
                let mut cell = gfx.cell(size);
                while size > smallest
                    && (w * cell.0 > canvas.w - MARGIN * 2 || h * cell.1 > canvas.h - MARGIN * 2)
                {
                    size -= 1;
                    cell = gfx.cell(size);
                }
                self.graph.zoom = size as f32 / self.px as f32;
                self.graph.pan = (MARGIN - x * cell.0, MARGIN - y * cell.1);
            }
        }
        let px = self.graph_px();
        let (cell_w, row_h) = gfx.cell(px);
        let z = self.graph.zoom;
        let m = Metrics {
            cell_w,
            row_h,
            pad: cell_w / 2,
        };
        self.graph.cell = (cell_w, row_h);
        let focus_node = self.graph.focus_node(self.focus, self.sel_anchor);
        if canvas_known {
            self.graph.move_camera(canvas, focus_node);
        }

        // toolbar
        self.ui.open(
            Kind::None,
            Layout::row()
                .grow_x()
                .pad(4)
                .gap(8)
                .cross(ui::Align::Center),
            Style::default(),
            None,
        );
        self.label(
            "drag the background to pan, ctrl+wheel to zoom, drag a node's title to move it",
            WEAK,
        );
        if self.small_button("1:1", ui::id("graph-1to1")).clicked {
            self.graph.zoom = 1.0;
            self.graph.want_look = true;
        }
        if self
            .small_button("auto layout", ui::id("graph-auto"))
            .clicked
        {
            self.graph.manual.clear();
        }
        if self.small_button("fit", ui::id("graph-fit")).clicked {
            self.graph.want_fit = true;
        }
        if let Some(pi) = self.graph.path_id {
            let s = format!(
                "path '{}': green edges are its steps, grey ones expansions, orange a call back up the tree",
                self.map.paths[pi].name
            );
            self.label(&s, WEAK);
        }
        self.label(&format!("{:.0}%", z * 100.0), WEAK);
        self.ui.close();

        // the scene
        let scene = self.build_scene(canvas, focus_node, px, m);
        self.ui.leaf(
            Kind::Custom(Box::new(move |gfx: &mut Gfx, _: Rect| {
                draw_scene(gfx, &scene)
            })),
            Layout::col().grow(),
            Style::bg(BG),
            Some(canvas_id),
        );
    }

    /// The node font: the UI size scaled by the zoom, whole pixels.
    fn graph_px(&self) -> u32 {
        ((self.px as f32) * self.graph.zoom).round().max(6.0) as u32
    }

    fn build_scene(
        &mut self,
        canvas: Rect,
        focus_node: Option<Node>,
        px: u32,
        m: Metrics,
    ) -> Scene {
        let o = (canvas.x + self.graph.pan.0, canvas.y + self.graph.pan.1);
        let mouse = self.ui.input.mouse;
        let mut hits = Vec::new();
        let mut nodes = Vec::new();
        let step_color = GREEN;
        // nodes
        for i in 0..self.graph.nodes.len() {
            let n = self.graph.nodes[i];
            let rect = self.graph.rect_px(n, o).unwrap();
            let (lo, hi) = self.graph.view[&n];
            let (shown, total) = self.graph.lines_shown(n);
            let code = self.grid(n.0.file, lo, lo + shown - 1);
            let on_path = self.graph.step.contains_key(&n);
            let off_path = self.graph.path_id.is_some() && !on_path;
            let focused = focus_node == Some(n);
            let stale =
                n.1.zip(self.graph.path_id)
                    .is_some_and(|(ai, pi)| self.map.paths[pi].anchors[ai].stale);
            let (border, border_w) = if stale {
                (RED, 2)
            } else if focused {
                (ACCENT, 2)
            } else if on_path {
                (step_color, 2)
            } else {
                (BORDER, 1)
            };
            let node_hits_from = hits.len();
            let fill = if off_path { PANEL } else { FIELD };
            let (header, labels) = self.graph.header(&self.idx, n);
            // buttons, right-aligned in the header
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
            let note =
                n.1.and_then(|ai| {
                    self.graph
                        .path_id
                        .map(|pi| self.map.paths[pi].anchors[ai].note.clone())
                })
                .filter(|s| !s.is_empty());
            let code_top = rect.y + m.pad + m.row_h + if note.is_some() { m.row_h } else { 0 } + 4;
            self.graph.code_top.insert(n, code_top);
            for (k, li) in (lo..lo + shown).enumerate() {
                let lr = Rect::new(
                    rect.x + m.pad,
                    code_top + k as i32 * m.row_h,
                    rect.w - m.pad * 2,
                    m.row_h,
                );
                hits.push((lr, Hit::Line(n, li)));
            }
            hits.extend(button_hits);
            // the lines an edge leaves from: every node on the canvas this one calls, plus its
            // child steps, whichever way either was revealed
            let mut targets: Vec<SymRef> = self
                .graph
                .callees_of(&self.idx, n)
                .into_iter()
                .filter(|c| *c != n.0 && self.graph.by_sym.contains_key(c))
                .collect();
            targets.extend(
                self.graph
                    .step_parent
                    .iter()
                    .filter(|(_, p)| **p == n)
                    .map(|(c, _)| c.0),
            );
            let tinted: HashSet<usize> = targets
                .iter()
                .filter_map(|c| self.graph.call_line(&self.idx, n, &self.idx.sym(*c).name))
                .filter(|&li| li < lo + shown)
                .collect();
            // later hits win: body, then the header, then lines, then the buttons
            let mut ordered = vec![(rect, Hit::Body(n)), (header_rect, Hit::Header(n))];
            ordered.extend(hits.drain(node_hits_from..));
            hits.extend(ordered);
            let (rlo, rhi) = self.graph.range[&n];
            let slice = ((lo, hi) != (rlo, rhi)).then_some((rlo, rhi));
            nodes.push(SceneNode {
                rect,
                fill,
                border,
                border_w,
                header,
                buttons,
                note,
                code_top,
                code,
                lo,
                tinted,
                slice,
                more: (shown < total).then_some(total - shown),
            });
        }
        // edges: where an edge leaves a node is level with the call line when it is shown
        let g = &self.graph;
        let edge_out = |n: Node, word: &str| -> (f32, f32) {
            let r = g.rect_px(n, o).unwrap();
            let (lo, _) = g.view[&n];
            let (shown, _) = g.lines_shown(n);
            match g.call_line(&self.idx, n, word) {
                Some(li) if li < lo + shown => (
                    r.right() as f32,
                    (g.code_top[&n] + (li - lo) as i32 * m.row_h + m.row_h / 2) as f32,
                ),
                _ => (r.right() as f32, (r.y + m.pad + m.row_h / 2) as f32),
            }
        };
        // a curve leaving p0 and entering p1 horizontally, bowing out rightwards for `dir` 1.0
        // and leftwards for -1.0
        let curve = |p0: (f32, f32), p1: (f32, f32), dir: f32| {
            let dx = ((p1.0 - p0.0).abs() * 0.5).max((GAP_X * m.cell_w) as f32 * 0.8) * dir;
            [p0, (p0.0 + dx, p0.1), (p1.0 - dx, p1.1), p1]
        };
        let mut edges = Vec::new();
        let hy = (m.pad + m.row_h / 2) as f32;
        for &a in &g.nodes {
            for bs in g.callees_of(&self.idx, a) {
                let Some(&b) = g.by_sym.get(&bs) else {
                    continue;
                };
                if a.0 == bs || g.step_parent.get(&b) == Some(&a) {
                    continue;
                }
                let (ra, rb) = (g.rect_px(a, o).unwrap(), g.rect_px(b, o).unwrap());
                if rb.x >= ra.right() {
                    let p0 = edge_out(a, &self.idx.sym(bs).name);
                    edges.push((curve(p0, (rb.x as f32, rb.y as f32 + hy), 1.0), WEAK, 1.5));
                } else {
                    let p0 = (ra.x as f32, ra.y as f32 + hy);
                    edges.push((
                        curve(p0, (rb.right() as f32, rb.y as f32 + hy), -1.0),
                        ORANGE,
                        1.5,
                    ));
                }
            }
        }
        for (&child, &parent) in &g.step_parent {
            let rb = g.rect_px(child, o).unwrap();
            let p0 = edge_out(parent, &self.idx.sym(child.0).name);
            edges.push((
                curve(p0, (rb.x as f32, rb.y as f32 + hy), 1.0),
                step_color,
                3.0,
            ));
        }
        self.graph.hits = hits
            .into_iter()
            .map(|(r, h)| (r.intersect(&canvas), h))
            .collect();
        Scene {
            px,
            m,
            edges,
            nodes,
        }
    }
}

fn draw_scene(gfx: &mut Gfx, scene: &Scene) {
    let m = scene.m;
    for (p, color, w) in &scene.edges {
        gfx.curve(*p, *w, *color);
        gfx.circle(p[3].0, p[3].1, 3.5, *color);
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
            gfx.text(
                br.x + m.cell_w,
                br.y,
                scene.px,
                label,
                if *hovered { TEXT } else { WEAK },
            );
        }
        if let Some(note) = &n.note {
            gfx.text(n.rect.x + m.pad, y + m.row_h, scene.px, note, GREEN);
        }
        gfx.rect(
            Rect::new(n.rect.x + m.pad, n.code_top - 2, n.rect.w - m.pad * 2, 1),
            BORDER,
        );
        for k in 0..n.code.h {
            let li = n.lo + k;
            let ly = n.code_top + k as i32 * m.row_h;
            if n.slice.is_some_and(|(a, b)| li >= a && li <= b) {
                gfx.rect(
                    Rect::new(n.rect.x + m.pad, ly, n.rect.w - m.pad * 2, m.row_h),
                    dim(SELECTED, 120),
                );
            }
            if n.tinted.contains(&li) {
                gfx.rect(
                    Rect::new(n.rect.x + m.pad, ly, n.rect.w - m.pad * 2, m.row_h),
                    dim(GREEN, 46),
                );
            }
        }
        gfx.glyphs(n.rect.x + m.pad, n.code_top, scene.px, &n.code);
        if let Some(more) = n.more {
            gfx.text(
                n.rect.x + m.pad,
                n.code_top + n.code.h as i32 * m.row_h,
                scene.px,
                &format!("      … {more} more lines"),
                WEAK,
            );
        }
        gfx.pop_clip();
    }
}
