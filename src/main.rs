mod cli;
mod index;
mod map;

use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers, Sense, Stroke, TextStyle, pos2, text::LayoutJob, vec2};
use index::{Index, Span, SymRef};
use map::{Author, Kind, Map};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Graph,
    Listing,
    Results,
}

// ---- syntax colours ---------------------------------------------------------------

fn hl_color(class: u8, base: Color32) -> Color32 {
    match class {
        index::HL_KEYWORD => Color32::from_rgb(197, 134, 192),
        index::HL_STRING => Color32::from_rgb(206, 145, 120),
        index::HL_COMMENT => Color32::from_rgb(106, 153, 85),
        index::HL_FUNCTION => Color32::from_rgb(220, 220, 170),
        index::HL_TYPE => Color32::from_rgb(78, 201, 176),
        index::HL_CONSTANT => Color32::from_rgb(181, 206, 168),
        index::HL_PROPERTY => Color32::from_rgb(156, 220, 254),
        _ => base,
    }
}

/// Append one source line to `job` with its syntax colours, cut to `max_chars` with an ellipsis.
fn append_line(job: &mut LayoutJob, line: &str, spans: &[Span], font: &FontId, base: Color32, max_chars: usize) {
    let fmt = |c: Color32| egui::TextFormat { font_id: font.clone(), color: c, ..Default::default() };
    let (text, cut) = match line.char_indices().nth(max_chars) {
        Some((b, _)) if max_chars > 0 => (&line[..line.char_indices().nth(max_chars - 1).map(|(b, _)| b).unwrap_or(b)], true),
        _ => (line, false),
    };
    let mut at = 0usize;
    for &(s, e, class) in spans {
        let (s, e) = (s as usize, e as usize);
        if s >= text.len() {
            break;
        }
        let e = e.min(text.len());
        if s > at {
            job.append(&text[at..s], 0.0, fmt(base));
        }
        job.append(&text[s..e], 0.0, fmt(hl_color(class, base)));
        at = e;
    }
    if at < text.len() {
        job.append(&text[at..], 0.0, fmt(base));
    }
    if cut {
        job.append("…", 0.0, fmt(base));
    }
}

// ---- graph: the main view ---------------------------------------------------------

const PREVIEW_LINES: usize = 12;
const MAX_NODE_LINES: usize = 200;
const HEADER_MIN_W: f32 = 380.0;
const NODE_MAX_W: f32 = 760.0;
const GAP_X: f32 = 110.0;
const GAP_Y: f32 = 28.0;
const HEADER_H: f32 = 34.0;

/// Font metrics the layout needs, refreshed every frame.
#[derive(Clone, Copy)]
struct Metrics {
    line_h: f32,
    char_w: f32,
}

/// Nodes are symbols showing their code.
///
/// What is visible is *derived*: a base (the focus symbol, or a path's tree of steps) plus an
/// ordered list of expansions ("callers of X", "callees of X"). Toggling an expansion off removes
/// it from the list and rebuilds, so nodes that were only reachable through it disappear too.
///
/// Layout is layered. A step's column is its depth in the path tree; any other node's column is
/// its hop distance from what it was expanded from. Columns are placed from the focus outward.
/// A step wants to sit level with its parent step; any other node level with the mean of its
/// neighbours in the inner column; the focus is pinned. Nodes are stacked in that order with
/// measured sizes and a fixed gap. Dragging a node switches to manual until the next structural
/// change.
#[derive(Default)]
struct Graph {
    nodes: Vec<SymRef>,
    col: HashMap<SymRef, i32>,
    pos: HashMap<SymRef, egui::Pos2>,
    size: HashMap<SymRef, egui::Vec2>, // measured last frame
    focus: Option<SymRef>,
    path_id: Option<usize>,
    step: HashMap<SymRef, (usize, usize)>, // path node -> (1-based pre-order number, anchor index)
    step_parent: HashMap<SymRef, SymRef>,  // path node -> its parent step's node
    origin: HashMap<SymRef, (SymRef, bool)>, // expansion node -> (the node that revealed it, via callees?)
    expansions: Vec<(SymRef, bool)>,       // (node, callees?) in the order the user opened them
    expanded: HashSet<SymRef>,             // nodes showing all their lines
    manual: bool,                          // user dragged something: keep positions
    scene_rect: Option<egui::Rect>,
}

impl Graph {
    fn clear(&mut self) {
        *self = Graph::default();
    }

    /// Lines shown for `r`: (shown, total).
    fn lines_shown(&self, idx: &Index, r: SymRef) -> (usize, usize) {
        let s = idx.sym(r);
        let total = s.end - s.start + 1;
        let cap = if self.expanded.contains(&r) { MAX_NODE_LINES } else { PREVIEW_LINES };
        (total.min(cap), total)
    }

    /// Characters of code that fit on one row of `r`'s node (lines are cut to this).
    fn max_chars(&self, idx: &Index, r: SymRef, m: Metrics) -> usize {
        (((self.node_size(idx, r, m).x - 12.0) / m.char_w) as usize).saturating_sub(6)
    }

    /// Width from the longest shown line (clamped); height from the measured frame, else estimated.
    fn node_size(&self, idx: &Index, r: SymRef, m: Metrics) -> egui::Vec2 {
        let s = idx.sym(r);
        let (shown, total) = self.lines_shown(idx, r);
        let longest = idx.files[r.file].lines[s.start..s.start + shown].iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let w = (12.0 + (longest + 6) as f32 * m.char_w).clamp(HEADER_MIN_W, NODE_MAX_W);
        let h = match self.size.get(&r) {
            Some(sz) => sz.y,
            None => HEADER_H + (shown + usize::from(shown < total)) as f32 * m.line_h + 12.0,
        };
        vec2(w, h)
    }

    fn node_rect(&self, idx: &Index, r: SymRef, m: Metrics) -> egui::Rect {
        egui::Rect::from_min_size(self.pos.get(&r).copied().unwrap_or(pos2(0.0, 0.0)), self.node_size(idx, r, m))
    }

    fn add(&mut self, r: SymRef, col: i32) {
        if self.col.contains_key(&r) {
            return;
        }
        self.col.insert(r, col);
        self.nodes.push(r);
    }

    fn is_expanded(&self, r: SymRef, callees: bool) -> bool {
        self.expansions.contains(&(r, callees))
    }

    /// Derive the visible node set from base + expansions.
    fn rebuild(&mut self, idx: &Index, map: &Map) {
        self.nodes.clear();
        self.col.clear();
        self.step.clear();
        self.step_parent.clear();
        self.origin.clear();
        match self.path_id {
            Some(pi) if pi < map.paths.len() => {
                let anchors = &map.paths[pi].anchors;
                let mut node_of: HashMap<usize, SymRef> = HashMap::new(); // anchor index -> node
                for (k, (ai, depth)) in map.tree_order(pi).into_iter().enumerate() {
                    let a = &anchors[ai];
                    let Some(fi) = idx.find_file(&a.file) else { continue };
                    let Some(si) = idx.files[fi].symbols.iter().position(|s| s.name == a.symbol) else { continue };
                    let r = SymRef { file: fi, sym: si };
                    if self.col.contains_key(&r) {
                        node_of.insert(ai, r);
                        continue; // a symbol twice in one path: one node
                    }
                    self.add(r, depth as i32);
                    self.step.insert(r, (k + 1, ai));
                    node_of.insert(ai, r);
                    if a.parent >= 0 {
                        if let Some(&p) = node_of.get(&(a.parent as usize)) {
                            if p != r {
                                self.step_parent.insert(r, p);
                            }
                        }
                    }
                }
                if self.focus.is_none_or(|f| !self.col.contains_key(&f)) {
                    self.focus = self.nodes.first().copied();
                }
            }
            _ => {
                self.path_id = None;
                if let Some(f) = self.focus {
                    self.add(f, 0);
                }
            }
        }
        for (r, callees) in self.expansions.clone() {
            let Some(&col) = self.col.get(&r) else { continue };
            let s = idx.sym(r);
            let (list, dc) = if callees { (&s.callees, 1) } else { (&s.callers, -1) };
            for &n in list {
                if !self.col.contains_key(&n) {
                    self.origin.insert(n, (r, callees));
                }
                self.add(n, col + dc);
            }
        }
        self.manual = false;
    }

    fn build_around(&mut self, idx: &Index, map: &Map, r: SymRef) {
        self.clear();
        self.focus = Some(r);
        self.expansions = vec![(r, false), (r, true)];
        self.rebuild(idx, map);
    }

    fn build_path(&mut self, idx: &Index, map: &Map, pi: usize) {
        self.clear();
        self.path_id = Some(pi);
        self.rebuild(idx, map);
    }

    /// Open or close the callers/callees of `r`.
    fn toggle(&mut self, idx: &Index, map: &Map, r: SymRef, callees: bool) {
        match self.expansions.iter().position(|e| *e == (r, callees)) {
            Some(i) => {
                self.expansions.remove(i);
            }
            None => self.expansions.push((r, callees)),
        }
        self.rebuild(idx, map);
    }

    /// Recompute every position as a forest of compact subtrees.
    ///
    /// x: a column per depth, each as wide as its widest node, anchored at the focus column.
    /// y: every node owns a block = its own height or the stacked heights of its children's
    /// blocks, whichever is taller; children stack beside the parent (steps and callee
    /// expansions to the right, caller expansions to the left) and the parent centres on
    /// them. Sibling blocks never interleave, so a wide subtree only pushes its own siblings.
    /// Roots (the focus, other path roots, anything without a parent) stack top to bottom.
    /// A final pass pushes apart the rare column collisions between different subtrees.
    fn layout(&mut self, idx: &Index, m: Metrics) {
        let Some(focus) = self.focus else { return };
        if self.nodes.is_empty() {
            return;
        }

        // x
        let mut cols: BTreeMap<i32, Vec<SymRef>> = BTreeMap::new();
        for &r in &self.nodes {
            cols.entry(self.col[&r]).or_default().push(r);
        }
        let (first, last) = (*cols.keys().next().unwrap(), *cols.keys().last().unwrap());
        let anchor_col = self.col[&focus];
        let col_w: BTreeMap<i32, f32> = cols.iter().map(|(&c, rs)| (c, rs.iter().map(|&r| self.node_size(idx, r, m).x).fold(0.0, f32::max))).collect();
        let mut col_x: BTreeMap<i32, f32> = BTreeMap::new();
        col_x.insert(anchor_col, 0.0);
        for c in (anchor_col + 1)..=last {
            col_x.insert(c, col_x[&(c - 1)] + col_w[&(c - 1)] + GAP_X);
        }
        for c in (first..anchor_col).rev() {
            col_x.insert(c, col_x[&(c + 1)] - col_w[&c] - GAP_X);
        }

        // forest: steps under their parent step, expansions beside what revealed them
        let mut right: HashMap<SymRef, Vec<SymRef>> = HashMap::new();
        let mut left: HashMap<SymRef, Vec<SymRef>> = HashMap::new();
        let mut has_parent: HashSet<SymRef> = HashSet::new();
        for &n in &self.nodes {
            if let Some(&p) = self.step_parent.get(&n) {
                right.entry(p).or_default().push(n);
                has_parent.insert(n);
            } else if let Some(&(o, callees)) = self.origin.get(&n) {
                if callees { right.entry(o).or_default() } else { left.entry(o).or_default() }.push(n);
                has_parent.insert(n);
            }
        }
        for kids in right.values_mut() {
            kids.sort_by_key(|k| self.step.get(k).map_or(usize::MAX, |s| s.0)); // steps first, in step order
        }
        let mut roots: Vec<SymRef> = vec![focus];
        roots.extend(self.nodes.iter().copied().filter(|n| *n != focus && !has_parent.contains(n)));

        // pass 1: block heights, bottom-up
        let mut height: HashMap<SymRef, f32> = HashMap::new();
        fn measure(g: &Graph, idx: &Index, m: Metrics, r: SymRef, right: &HashMap<SymRef, Vec<SymRef>>, left: &HashMap<SymRef, Vec<SymRef>>, height: &mut HashMap<SymRef, f32>, seen: &mut HashSet<SymRef>) -> f32 {
            if !seen.insert(r) {
                return 0.0;
            }
            let stack = |g: &Graph, kids: Option<&Vec<SymRef>>, height: &mut HashMap<SymRef, f32>, seen: &mut HashSet<SymRef>| -> f32 {
                let mut h = 0.0;
                for &k in kids.into_iter().flatten() {
                    let kh = measure(g, idx, m, k, right, left, height, seen);
                    if kh > 0.0 {
                        h += kh + GAP_Y;
                    }
                }
                (h - GAP_Y).max(0.0)
            };
            let rh = stack(g, right.get(&r), height, seen);
            let lh = stack(g, left.get(&r), height, seen);
            let h = g.node_size(idx, r, m).y.max(rh).max(lh);
            height.insert(r, h);
            h
        }
        let mut seen = HashSet::new();
        for &r in &roots {
            measure(self, idx, m, r, &right, &left, &mut height, &mut seen);
        }

        // pass 2: place, top-down
        fn place(g: &mut Graph, idx: &Index, m: Metrics, r: SymRef, top: f32, col_x: &BTreeMap<i32, f32>, right: &HashMap<SymRef, Vec<SymRef>>, left: &HashMap<SymRef, Vec<SymRef>>, height: &HashMap<SymRef, f32>, done: &mut HashSet<SymRef>) {
            if !done.insert(r) {
                return;
            }
            let block = height[&r];
            let own = g.node_size(idx, r, m).y;
            g.pos.insert(r, pos2(col_x[&g.col[&r]], top + (block - own) / 2.0));
            for kids in [right.get(&r), left.get(&r)] {
                let kids: Vec<SymRef> = kids.into_iter().flatten().copied().filter(|k| height.contains_key(k) && !done.contains(k)).collect();
                let stack_h: f32 = kids.iter().map(|k| height[k] + GAP_Y).sum::<f32>() - GAP_Y;
                let mut cur = top + (block - stack_h.max(0.0)) / 2.0;
                for k in kids {
                    place(g, idx, m, k, cur, col_x, right, left, height, done);
                    cur += height[&k] + GAP_Y;
                }
            }
        }
        let mut done = HashSet::new();
        let mut cur = 0.0;
        for &r in &roots {
            if done.contains(&r) {
                continue;
            }
            place(self, idx, m, r, cur, &col_x, &right, &left, &height, &mut done);
            cur += height[&r] + GAP_Y * 2.0;
        }

        // pass 3: different subtrees can still meet in one column (a caller's callees land in
        // the focus column, say); push the later one down.
        for rs in cols.values_mut() {
            rs.sort_by(|a, b| self.pos[a].y.partial_cmp(&self.pos[b].y).unwrap());
            let mut prev_bottom = f32::NEG_INFINITY;
            for &r in rs.iter() {
                let h = self.node_size(idx, r, m).y;
                let y = self.pos[&r].y.max(prev_bottom + GAP_Y);
                self.pos.get_mut(&r).unwrap().y = y;
                prev_bottom = y + h;
            }
        }
    }

    fn fit(&mut self) {
        self.scene_rect = None; // Scene resets an invalid rect to fit the contents
    }

    /// Readable zoom centred on the focused node (fit-all is unreadable past a few nodes).
    fn look_at_focus(&mut self, idx: &Index, m: Metrics) {
        match self.focus {
            Some(r) => {
                let c = self.node_rect(idx, r, m).center();
                self.scene_rect = Some(egui::Rect::from_center_size(c, vec2(1900.0, 1100.0)));
            }
            None => self.fit(),
        }
    }
}

struct App {
    idx: Index,
    map: Map,
    map_path: PathBuf,
    map_mtime: Option<SystemTime>,
    dirty: bool,
    last_poll: Instant,
    warned_disk: bool,

    sel_path: Option<usize>,
    sel_anchor: Option<usize>,

    tab: Tab,
    graph: Graph,
    metrics: Metrics,
    cur_file: Option<usize>,
    sel: Option<(usize, usize)>, // (anchor line, active line) of the line selection
    scroll_to: Option<usize>,

    sym_filter: String,
    search: String,
    new_path: String,
    results: Vec<(usize, usize)>, // (file, line)

    cmd: String,
    output: String,
    status: String,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok().and_then(|m| m.modified().ok())
}

// ---- actions ----------------------------------------------------------------------

impl App {
    fn new(root: &Path) -> App {
        let idx = index::build(root);
        let map_path = root.join(".codemap");
        let map = Map::load(&map_path);
        let mut status = format!("{} files, {} symbols indexed", idx.files.len(), idx.files.iter().map(|f| f.symbols.len()).sum::<usize>());
        if map.is_none() && map_path.exists() {
            status = ".codemap is unreadable or an old format: starting from an empty map, saving overwrites it".into();
        }
        let mut map = map.unwrap_or_default();
        map.resolve_all(&idx);
        let mut app = App {
            idx,
            map,
            map_mtime: mtime(&map_path),
            map_path,
            dirty: false,
            last_poll: Instant::now(),
            warned_disk: false,
            sel_path: None,
            sel_anchor: None,
            tab: Tab::Graph,
            graph: Graph::default(),
            metrics: Metrics { line_h: 15.0, char_w: 7.5 },
            cur_file: None,
            sel: None,
            scroll_to: None,
            sym_filter: String::new(),
            search: String::new(),
            new_path: String::new(),
            results: Vec::new(),
            cmd: String::new(),
            output: "type 'help' for commands\n".into(),
            status,
        };
        if let Some(r) = app.idx.roots().first().copied() {
            app.focus(r);
        }
        app
    }

    /// Make `r` the current symbol: listing position, xrefs, and graph focus (rebuilding the
    /// graph around it if it is not already on screen). Moves the view.
    fn focus(&mut self, r: SymRef) {
        let s = self.idx.sym(r);
        let (start, end) = (s.start, s.end);
        self.cur_file = Some(r.file);
        self.sel = Some((start, end));
        self.scroll_to = Some(start.saturating_sub(3));
        if !self.graph.col.contains_key(&r) {
            self.graph.build_around(&self.idx, &self.map, r);
        }
        self.graph.focus = Some(r);
        self.relayout();
        self.graph.look_at_focus(&self.idx, self.metrics);
        if self.tab == Tab::Results {
            self.tab = Tab::Graph;
        }
    }

    fn relayout(&mut self) {
        if !self.graph.manual {
            self.graph.layout(&self.idx, self.metrics);
        }
    }

    /// The graph's node set changed (path edited, map reloaded): rebuild in place, view stays.
    fn refresh_graph(&mut self) {
        self.graph.rebuild(&self.idx, &self.map);
        self.relayout();
    }

    fn open_line(&mut self, file: usize, line: usize) {
        self.cur_file = Some(file);
        self.sel = Some((line, line));
        self.scroll_to = Some(line.saturating_sub(8));
        self.tab = Tab::Listing;
    }

    fn show_path(&mut self, pi: usize) {
        self.graph.build_path(&self.idx, &self.map, pi);
        self.relayout();
        self.graph.look_at_focus(&self.idx, self.metrics);
        if let Some(r) = self.graph.focus {
            let s = self.idx.sym(r);
            let (start, end) = (s.start, s.end);
            self.cur_file = Some(r.file);
            self.sel = Some((start, end));
            self.scroll_to = Some(start.saturating_sub(3));
        }
    }

    fn run_search(&mut self) {
        self.results.clear();
        let re = match regex::Regex::new(&self.search) {
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
        self.status = format!("{} hits for /{}/", self.results.len(), self.search);
        self.tab = Tab::Results;
    }

    fn create_path(&mut self) {
        let name = self.new_path.trim().to_owned();
        if name.is_empty() {
            return;
        }
        self.sel_path = Some(self.map.add_path(&name, Kind::Flow, Author::Human));
        self.sel_anchor = None;
        self.new_path.clear();
        self.dirty = true;
    }

    /// Parent for a new step of the selected path. A node revealed by opening a step's callees
    /// goes under that step (that is the call it was reached by); otherwise under the selected
    /// step, else the focused node's step, else root.
    fn step_parent_for_new(&self, pi: usize, node: Option<SymRef>) -> i32 {
        if self.graph.path_id == Some(pi) {
            if let Some((_, ai)) = node.and_then(|n| self.graph.origin.get(&n)).filter(|(_, callees)| *callees).and_then(|(o, _)| self.graph.step.get(o)) {
                return *ai as i32;
            }
        }
        if let Some(ai) = self.sel_anchor.filter(|&ai| ai < self.map.paths[pi].anchors.len()) {
            return ai as i32;
        }
        if self.graph.path_id == Some(pi) {
            if let Some((_, ai)) = self.graph.focus.and_then(|f| self.graph.step.get(&f)) {
                return *ai as i32;
            }
        }
        -1
    }

    /// Add lines as a step of the selected path. The view does not move; the new step becomes
    /// the selected one so repeated adds build a chain, and a click on another step starts a
    /// branch.
    fn add_step(&mut self, fi: usize, ls: usize, le: usize, node: Option<SymRef>) {
        let Some(pi) = self.sel_path else {
            self.status = "select a path first (Paths window)".into();
            return;
        };
        let name = &self.idx.files[fi].path;
        if let Some(s) = self.idx.files[fi].symbols.iter().filter(|s| s.start <= ls && le <= s.end).max_by_key(|s| s.depth) {
            if s.start == ls && s.end == le {
                if let Some(ai) = self.map.step_for(pi, name, &s.name) {
                    self.sel_anchor = Some(ai);
                    self.status = format!("'{}' is already step [{ai}] of '{}'", s.name, self.map.paths[pi].name);
                    return;
                }
            }
        }
        let parent = self.step_parent_for_new(pi, node);
        let ai = self.map.add_anchor(&self.idx, pi, fi, ls, le, Author::Human, parent);
        self.sel_anchor = Some(ai);
        self.dirty = true;
        self.status = format!("step [{ai}] added to '{}' under [{parent}]", self.map.paths[pi].name);
        if self.graph.path_id == Some(pi) {
            self.refresh_graph();
        }
    }

    /// Top-bar button: the listing selection, else the focused node.
    fn add_selection(&mut self) {
        match (self.tab, self.cur_file, self.sel, self.graph.focus) {
            (Tab::Listing, Some(fi), Some((a, b)), _) => self.add_step(fi, a.min(b), a.max(b), None),
            (_, _, _, Some(r)) => {
                let s = self.idx.sym(r);
                let (start, end) = (s.start, s.end);
                self.add_step(r.file, start, end, Some(r));
            }
            _ => self.status = "select lines in the listing or a node in the graph first".into(),
        }
    }

    // ponytail: no undo; deletes the selected step (children move up), else the selected path.
    fn delete_selected(&mut self) {
        let Some(pi) = self.sel_path else { return };
        match self.sel_anchor {
            Some(ai) if ai < self.map.paths[pi].anchors.len() => {
                self.map.remove_anchor(pi, ai);
                self.sel_anchor = None;
            }
            _ => {
                self.map.paths.remove(pi);
                self.sel_path = None;
            }
        }
        self.dirty = true;
        self.refresh_graph();
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

    fn run_cmd(&mut self) {
        let line = std::mem::take(&mut self.cmd);
        self.output.push_str(&format!("> {line}\n"));
        let cmd = match cli::parse(&cli::tokenize(&line)) {
            Ok(cmd) => cmd,
            Err(e) => return self.output.push_str(&e.to_string()),
        };
        match cli::exec(&self.idx, &mut self.map, cmd, Author::Human, &mut self.output) {
            Ok(true) => {
                self.dirty = true;
                self.output.push_str("(map changed, ctrl+s to save)\n");
                self.refresh_graph();
            }
            Ok(false) => {}
            Err(e) => self.output.push_str(&format!("error: {e}\n")),
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
                if self.sel_path.is_some_and(|pi| pi >= self.map.paths.len()) {
                    self.sel_path = None;
                }
                self.refresh_graph();
                self.status = "map reloaded (changed on disk)".into();
            }
        }

        if self.idx.changed() {
            self.reindex();
        }
    }

    /// Rebuild the index and carry the graph over by (file, symbol) identity.
    fn reindex(&mut self) {
        let focus = self.graph.focus.map(|r| self.idx.key(r));
        let expansions: Vec<((String, String), bool)> = self.graph.expansions.iter().map(|(r, c)| (self.idx.key(*r), *c)).collect();
        let expanded: Vec<(String, String)> = self.graph.expanded.iter().map(|r| self.idx.key(*r)).collect();
        let cur_file = self.cur_file.map(|fi| self.idx.files[fi].path.clone());

        self.idx = index::build(&self.idx.root);
        self.map.resolve_all(&self.idx);
        self.graph.focus = focus.and_then(|k| self.idx.by_key(&k));
        self.graph.expansions = expansions.into_iter().filter_map(|(k, c)| self.idx.by_key(&k).map(|r| (r, c))).collect();
        self.graph.expanded = expanded.iter().filter_map(|k| self.idx.by_key(k)).collect();
        self.graph.size.clear();
        self.refresh_graph();
        self.cur_file = cur_file.and_then(|p| self.idx.find_file(&p));
        self.results.clear();
        self.status = format!("re-indexed: {} files, {} symbols (source changed)", self.idx.files.len(), self.idx.files.iter().map(|f| f.symbols.len()).sum::<usize>());
    }
}

// ---- windows ----------------------------------------------------------------------

enum Action {
    Focus(SymRef),
    OpenListing(SymRef),
    SelectPath(usize),
    ShowPath(usize),
    SelectStep(usize, usize),
    MoveAnchor(usize, usize, isize),
    Promote(SymRef),
    ExpandCallers(SymRef),
    ExpandCallees(SymRef),
    ToggleExpand(SymRef),
    AddToPath(SymRef),
    Relayout,
}

impl App {
    fn symbols_window(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.strong("Symbols");
            ui.add(egui::TextEdit::singleline(&mut self.sym_filter).hint_text("filter").desired_width(f32::INFINITY));
        });
        let filter = self.sym_filter.to_lowercase();
        let rows: Vec<SymRef> = self
            .idx
            .files
            .iter()
            .enumerate()
            .flat_map(|(file, f)| (0..f.symbols.len()).map(move |sym| SymRef { file, sym }))
            .filter(|r| filter.is_empty() || self.idx.sym(*r).name.to_lowercase().contains(&filter) || self.idx.files[r.file].path.to_lowercase().contains(&filter))
            .collect();
        let cur = self.graph.focus;
        let row_h = ui.text_style_height(&TextStyle::Monospace);
        egui::ScrollArea::vertical().id_salt("symbols").auto_shrink(false).show_rows(ui, row_h, rows.len(), |ui, range| {
            for r in &rows[range] {
                let s = self.idx.sym(*r);
                let text = format!("{}{:<26} {:<12} {}:{}", if s.depth > 0 { "  " } else { "" }, trunc(&s.name, 26), trunc(s.kind, 12), self.idx.files[r.file].path, s.start + 1);
                let resp = ui.selectable_label(cur == Some(*r), egui::RichText::new(text).monospace());
                if resp.clicked() {
                    action = Some(Action::Focus(*r));
                }
                resp.on_hover_text(format!("{} callers, {} callees", s.callers.len(), s.callees.len()));
            }
        });
        action
    }

    fn paths_window(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.strong(format!("Paths ({})", self.map.paths.len()));
            ui.weak("a path = a tree of code steps");
        });
        egui::ScrollArea::vertical().id_salt("paths").auto_shrink(false).show(ui, |ui| {
            for pi in 0..self.map.paths.len() {
                let selected = self.sel_path == Some(pi);
                let (name, kind, tag, n) = {
                    let p = &self.map.paths[pi];
                    (p.name.clone(), p.kind.name(), p.author.tag(), p.anchors.len())
                };
                ui.horizontal(|ui| {
                    if ui.selectable_label(selected, format!("{name} [{kind}]{tag}  ({n} steps)")).clicked() {
                        action = Some(Action::SelectPath(pi));
                    }
                    if ui.small_button("graph").on_hover_text("show this path as a tree in the graph").clicked() {
                        action = Some(Action::ShowPath(pi));
                    }
                });
                if !selected {
                    continue;
                }
                ui.indent(pi, |ui| {
                    if ui.add(egui::TextEdit::multiline(&mut self.map.paths[pi].note).desired_rows(2).hint_text("what this path is / does").desired_width(f32::INFINITY)).changed() {
                        self.dirty = true;
                    }
                    ui.weak("steps: click to select (new steps go under the selected one), up/down reorder siblings, * = has a note");
                    let n = self.map.paths[pi].anchors.len();
                    for (k, (ai, depth)) in self.map.tree_order(pi).into_iter().enumerate() {
                        let a = &self.map.paths[pi].anchors[ai];
                        let name = if a.symbol.is_empty() { "(lines)" } else { a.symbol.as_str() };
                        let text = format!("{}{}. {}{}  {}:{}-{}{}{}", "    ".repeat(depth), k + 1, if a.stale { "! " } else { "" }, name, a.file, a.line_start + 1, a.line_end + 1, a.author.tag(), if a.note.is_empty() { "" } else { " *" });
                        let color = if a.stale { Color32::LIGHT_RED } else { ui.visuals().text_color() };
                        let step_selected = self.sel_anchor == Some(ai);
                        ui.horizontal(|ui| {
                            if ui.add_enabled(ai > 0, egui::Button::new("up").small()).clicked() {
                                action = Some(Action::MoveAnchor(pi, ai, -1));
                            }
                            if ui.add_enabled(ai + 1 < n, egui::Button::new("down").small()).clicked() {
                                action = Some(Action::MoveAnchor(pi, ai, 1));
                            }
                            let resp = ui.selectable_label(step_selected, egui::RichText::new(text).color(color));
                            if resp.clicked() {
                                action = Some(Action::SelectStep(pi, ai));
                            }
                            if a.stale {
                                resp.on_hover_text("stale: the code changed since this step was pinned. Delete and re-add it.");
                            }
                        });
                        if step_selected {
                            ui.indent(("note", pi, ai), |ui| {
                                if ui.add(egui::TextEdit::multiline(&mut self.map.paths[pi].anchors[ai].note).desired_rows(1).hint_text("note for this step").desired_width(f32::INFINITY)).changed() {
                                    self.dirty = true;
                                }
                            });
                        }
                    }
                });
            }

            // ponytail: every root, one hop deep; the full tree lives in `tree` and the graph.
            let roots = self.idx.roots();
            egui::CollapsingHeader::new(format!("Entry points ({})", roots.len())).show(ui, |ui| {
                ui.weak("symbols nothing calls. promote = new path shaped like its call tree");
                for r in roots {
                    let s = self.idx.sym(r);
                    ui.horizontal(|ui| {
                        if ui.small_button("promote").clicked() {
                            action = Some(Action::Promote(r));
                        }
                        if ui.selectable_label(self.graph.focus == Some(r), format!("{} ({} calls)", s.name, s.callees.len())).clicked() {
                            action = Some(Action::Focus(r));
                        }
                    });
                }
            });
        });
        action
    }

    fn xrefs_window(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let Some(cur) = self.graph.focus else {
            ui.weak("no symbol focused");
            return None;
        };
        let s = self.idx.sym(cur);
        ui.strong(&s.name);
        ui.weak(format!("{} {}:{}-{}", s.kind, self.idx.files[cur.file].path, s.start + 1, s.end + 1));
        ui.separator();
        egui::ScrollArea::vertical().id_salt("xrefs").auto_shrink(false).show(ui, |ui| {
            ui.label(format!("Xrefs to ({})", s.callers.len()));
            for &r in &s.callers {
                if ui.selectable_label(false, cli::describe(&self.idx, r)).clicked() {
                    action = Some(Action::Focus(r));
                }
            }
            ui.separator();
            ui.label(format!("Xrefs from ({})", s.callees.len()));
            for &r in &s.callees {
                if ui.selectable_label(false, cli::describe(&self.idx, r)).clicked() {
                    action = Some(Action::Focus(r));
                }
            }
        });
        action
    }

    fn graph_view(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        if self.graph.nodes.is_empty() {
            ui.weak("click a symbol, an entry point, or a path's 'graph' button");
            return None;
        }
        ui.horizontal(|ui| {
            ui.weak("drag background to pan, ctrl+scroll to zoom, drag a node's title to move it");
            if ui.small_button("fit").clicked() {
                self.graph.fit();
            }
            if ui.add_enabled(self.graph.manual, egui::Button::new("auto layout").small()).clicked() {
                action = Some(Action::Relayout);
            }
            if let Some(pi) = self.graph.path_id {
                ui.separator();
                ui.label(format!("showing path '{}' as a tree; green edges are its steps", self.map.paths[pi].name));
            }
        });

        let m = self.metrics;
        if !self.graph.manual {
            self.graph.layout(&self.idx, m); // measured sizes from last frame settle the layout
        }
        let font = TextStyle::Monospace.resolve(ui.style());
        let in_path: Vec<SymRef> = self
            .sel_path
            .map(|pi| {
                self.map.paths[pi]
                    .anchors
                    .iter()
                    .filter_map(|a| {
                        let fi = self.idx.find_file(&a.file)?;
                        let si = self.idx.files[fi].symbols.iter().position(|s| s.name == a.symbol)?;
                        Some(SymRef { file: fi, sym: si })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut scene_rect = self.graph.scene_rect.unwrap_or(egui::Rect::ZERO);
        egui::Scene::new().zoom_range(0.1..=1.5).show(ui, &mut scene_rect, |ui| {
            let painter = ui.painter().clone();
            let edge = Stroke::new(1.5, ui.visuals().weak_text_color());
            let back = Stroke::new(1.5, Color32::from_rgb(220, 160, 80));
            let step_stroke = Stroke::new(3.0, Color32::from_rgb(90, 200, 120));
            let rect_of = |g: &Graph, r: SymRef| g.node_rect(&self.idx, r, m);
            let curve = |p0: egui::Pos2, p1: egui::Pos2, stroke: Stroke| {
                let dx = ((p1.x - p0.x).abs() * 0.5).max(GAP_X * 0.8);
                egui::epaint::CubicBezierShape::from_points_stroke([p0, p0 + vec2(dx, 0.0), p1 - vec2(dx, 0.0), p1], false, Color32::TRANSPARENT, stroke)
            };
            let hy = vec2(0.0, HEADER_H / 2.0);

            // call edges: caller's right header edge -> callee's left header edge. A callee that
            // sits to the left of its caller gets a short leftward curve in another colour
            // instead of a loop across the canvas. Pairs that are path steps are drawn below.
            for &a in &self.graph.nodes {
                let ra = rect_of(&self.graph, a);
                for &b in &self.idx.sym(a).callees {
                    if !self.graph.col.contains_key(&b) || a == b || self.graph.step_parent.get(&b) == Some(&a) {
                        continue;
                    }
                    let rb = rect_of(&self.graph, b);
                    if rb.left() >= ra.right() {
                        let (p0, p1) = (ra.right_top() + hy, rb.left_top() + hy);
                        painter.add(curve(p0, p1, edge));
                        painter.circle_filled(p1, 3.0, edge.color);
                    } else {
                        let (p0, p1) = (ra.left_top() + hy, rb.right_top() + hy);
                        let dx = ((p0.x - p1.x).abs() * 0.5).max(GAP_X * 0.8);
                        painter.add(egui::epaint::CubicBezierShape::from_points_stroke([p0, p0 - vec2(dx, 0.0), p1 + vec2(dx, 0.0), p1], false, Color32::TRANSPARENT, back));
                        painter.circle_filled(p1, 3.0, back.color);
                    }
                }
            }
            // path edges: parent step -> child step, labelled with the child's number
            for (&child, &parent) in &self.graph.step_parent {
                let (ra, rb) = (rect_of(&self.graph, parent), rect_of(&self.graph, child));
                let (p0, p1) = (ra.right_top() + hy, rb.left_top() + hy);
                painter.add(curve(p0, p1, step_stroke));
                painter.circle_filled(p1, 4.0, step_stroke.color);
                if let Some((k, _)) = self.graph.step.get(&child) {
                    painter.text(p1 - vec2(GAP_X * 0.4, 12.0), Align2::CENTER_CENTER, format!("{k}"), font.clone(), step_stroke.color);
                }
            }

            let base_color = ui.visuals().text_color();
            let dim = ui.visuals().weak_text_color();
            for r in self.graph.nodes.clone() {
                let s = self.idx.sym(r);
                let rect = rect_of(&self.graph, r);
                let focused = self.graph.focus == Some(r);
                let on_path = in_path.contains(&r);
                let border = if focused {
                    Stroke::new(2.0, ui.visuals().selection.stroke.color)
                } else if on_path {
                    Stroke::new(2.0, step_stroke.color)
                } else {
                    Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
                };
                let step = self.graph.step.get(&r).copied();

                let frame = ui.scope_builder(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(rect.min, vec2(rect.width(), f32::INFINITY))), |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    egui::Frame::new().fill(ui.visuals().extreme_bg_color).stroke(border).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                        ui.set_width(rect.width() - 12.0);
                        // title = drag handle; buttons registered after it so they stay clickable
                        ui.horizontal(|ui| {
                            let mut title = ui.strong(&s.name);
                            if let Some((k, _)) = step {
                                title = title.union(ui.colored_label(step_stroke.color, format!("step {k}")));
                            }
                            title = title.union(ui.weak(format!("{}:{}", self.idx.files[r.file].path, s.start + 1)));
                            let drag = ui.interact(title.rect, ui.id().with(("drag", r)), Sense::click_and_drag());
                            if drag.dragged() {
                                *self.graph.pos.get_mut(&r).unwrap() += drag.drag_delta();
                                self.graph.manual = true;
                            }
                            if drag.clicked() {
                                action = Some(Action::Focus(r));
                            }
                            drag.on_hover_text("drag to move, click to focus");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let (shown, total) = self.graph.lines_shown(&self.idx, r);
                                if total > PREVIEW_LINES && ui.small_button(if shown < total { "more" } else { "less" }).on_hover_text("show all / fewer lines").clicked() {
                                    action = Some(Action::ToggleExpand(r));
                                }
                                if ui.small_button("+path").on_hover_text("add as a step of the selected path, under the selected step").clicked() {
                                    action = Some(Action::AddToPath(r));
                                }
                                if ui.small_button("listing").on_hover_text("open in the listing").clicked() {
                                    action = Some(Action::OpenListing(r));
                                }
                                let (out_open, in_open) = (self.graph.is_expanded(r, true), self.graph.is_expanded(r, false));
                                if ui.add_enabled(!s.callees.is_empty(), egui::Button::new(format!("callees > {}", s.callees.len())).small().selected(out_open)).on_hover_text("show / hide what this calls").clicked() {
                                    action = Some(Action::ExpandCallees(r));
                                }
                                if ui.add_enabled(!s.callers.is_empty(), egui::Button::new(format!("{} < callers", s.callers.len())).small().selected(in_open)).on_hover_text("show / hide what calls this").clicked() {
                                    action = Some(Action::ExpandCallers(r));
                                }
                            });
                        });
                        // step note: the human's or the AI's annotation for this step of the path
                        if let (Some((_, ai)), Some(pi)) = (step, self.graph.path_id) {
                            let note = &self.map.paths[pi].anchors[ai].note;
                            if !note.is_empty() {
                                ui.label(egui::RichText::new(note).italics().color(step_stroke.color));
                            }
                        }
                        ui.separator();
                        let f = &self.idx.files[r.file];
                        let (shown, total) = self.graph.lines_shown(&self.idx, r);
                        let max_chars = self.graph.max_chars(&self.idx, r, m);
                        let mut job = LayoutJob::default();
                        let numfmt = egui::TextFormat { font_id: font.clone(), color: dim, ..Default::default() };
                        for li in s.start..s.start + shown {
                            job.append(&format!("{:4} ", li + 1), 0.0, numfmt.clone());
                            append_line(&mut job, &f.lines[li], &f.hl[li], &font, base_color, max_chars);
                            job.append("\n", 0.0, numfmt.clone());
                        }
                        if shown < total {
                            job.append(&format!("     … {} more lines", total - shown), 0.0, numfmt.clone());
                        }
                        ui.set_clip_rect(egui::Rect::from_min_size(rect.min, vec2(rect.width(), f32::INFINITY)).intersect(ui.clip_rect()));
                        ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
                    })
                });
                self.graph.size.insert(r, vec2(rect.width(), frame.inner.response.rect.height()));
            }
        });
        self.graph.scene_rect = Some(scene_rect);
        action
    }

    fn listing(&mut self, ui: &mut egui::Ui) {
        let Some(fi) = self.cur_file else {
            ui.weak("click a symbol to open its file");
            return;
        };
        let font = TextStyle::Monospace.resolve(ui.style());
        let row_h = ui.text_style_height(&TextStyle::Monospace);
        let row_stride = row_h + ui.spacing().item_spacing.y; // show_rows adds spacing between rows
        let text_color = ui.visuals().text_color();
        let dim = ui.visuals().weak_text_color();
        let sel_bg = ui.visuals().selection.bg_fill.linear_multiply(0.4);
        let shift = ui.input(|i| i.modifiers.shift);
        let n = self.idx.files[fi].lines.len();

        ui.weak(format!("{}  —  click a line, shift-click to extend, then 'add selection to path'", self.idx.files[fi].path));
        let mut area = egui::ScrollArea::both().id_salt("listing").auto_shrink(false);
        if let Some(line) = self.scroll_to.take() {
            area = area.vertical_scroll_offset(line as f32 * row_stride);
        }

        let mut clicked = None;
        area.show_rows(ui, row_h, n, |ui, range| {
            let f = &self.idx.files[fi];
            let (lo, hi) = self.sel.map(|(a, b)| (a.min(b), a.max(b))).unwrap_or((usize::MAX, usize::MAX));
            let anchors: Vec<(usize, usize, bool)> = self
                .sel_path
                .map(|pi| self.map.paths[pi].anchors.iter().filter(|a| a.file == f.path).map(|a| (a.line_start, a.line_end, a.stale)).collect())
                .unwrap_or_default();
            let width = ui.available_width().max(2000.0);

            for li in range {
                let (rect, resp) = ui.allocate_exact_size(vec2(width, row_h), Sense::click());
                if li >= lo && li <= hi {
                    ui.painter().rect_filled(rect, 0.0, sel_bg);
                }
                if let Some(&(_, _, stale)) = anchors.iter().find(|&&(s, e, _)| li >= s && li <= e) {
                    let bar = egui::Rect::from_min_size(rect.min, vec2(4.0, row_h));
                    ui.painter().rect_filled(bar, 0.0, if stale { Color32::LIGHT_RED } else { Color32::LIGHT_GREEN });
                }
                let p = ui.painter();
                p.text(rect.left_top() + vec2(8.0, 0.0), Align2::LEFT_TOP, format!("{:5}", li + 1), font.clone(), dim);
                let mut job = LayoutJob::default();
                append_line(&mut job, &f.lines[li], &f.hl[li], &font, text_color, usize::MAX);
                let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
                p.galley(rect.left_top() + vec2(64.0, 0.0), galley, text_color);
                if resp.clicked() {
                    clicked = Some(li);
                }
            }
        });

        if let Some(li) = clicked {
            self.sel = match (shift, self.sel) {
                (true, Some((a, _))) => Some((a, li)),
                _ => Some((li, li)),
            };
        }
    }

    fn results_view(&mut self, ui: &mut egui::Ui) {
        let row_h = ui.text_style_height(&TextStyle::Monospace);
        let mut open = None;
        egui::ScrollArea::vertical().id_salt("results").auto_shrink(false).show_rows(ui, row_h, self.results.len(), |ui, range| {
            for ri in range {
                let (fi, li) = self.results[ri];
                let f = &self.idx.files[fi];
                let text = format!("{}:{}: {}", f.path, li + 1, f.lines[li].trim());
                if ui.selectable_label(false, egui::RichText::new(text).monospace()).clicked() {
                    open = Some((fi, li));
                }
            }
        });
        if let Some((fi, li)) = open {
            self.open_line(fi, li);
        }
    }

    fn output_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Output");
            if ui.small_button("clear").clicked() {
                self.output.clear();
            }
        });
        let cmd_h = ui.text_style_height(&TextStyle::Body) + 8.0;
        egui::ScrollArea::vertical().id_salt("output").auto_shrink(false).stick_to_bottom(true).max_height(ui.available_height() - cmd_h).show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(&self.output).monospace()).selectable(true));
        });
        ui.horizontal(|ui| {
            ui.monospace(">");
            let resp = ui.add(egui::TextEdit::singleline(&mut self.cmd).desired_width(f32::INFINITY).font(TextStyle::Monospace));
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.cmd.trim().is_empty() {
                self.run_cmd();
                resp.request_focus();
            }
        });
    }

    fn apply(&mut self, action: Option<Action>) {
        match action {
            Some(Action::Focus(r)) => self.focus(r),
            Some(Action::OpenListing(r)) => {
                self.focus(r);
                self.tab = Tab::Listing;
            }
            Some(Action::SelectPath(pi)) => {
                self.sel_path = if self.sel_path == Some(pi) { None } else { Some(pi) };
                self.sel_anchor = None;
            }
            Some(Action::ShowPath(pi)) => {
                self.sel_path = Some(pi);
                self.sel_anchor = None;
                self.show_path(pi);
                self.tab = Tab::Graph;
            }
            Some(Action::SelectStep(pi, ai)) => {
                self.sel_anchor = Some(ai);
                let a = &self.map.paths[pi].anchors[ai];
                let (ls, le, file, sym) = (a.line_start, a.line_end, a.file.clone(), a.symbol.clone());
                if let Some(fi) = self.idx.find_file(&file) {
                    if let Some(si) = self.idx.files[fi].symbols.iter().position(|s| s.name == sym) {
                        if self.graph.path_id != Some(pi) {
                            self.show_path(pi);
                        }
                        self.focus(SymRef { file: fi, sym: si });
                    }
                    self.cur_file = Some(fi);
                    self.sel = Some((ls, le));
                    self.scroll_to = Some(ls.saturating_sub(3));
                }
            }
            Some(Action::MoveAnchor(pi, ai, delta)) => {
                let bi = (ai as isize + delta) as usize;
                self.map.swap_anchors(pi, ai, bi);
                self.sel_anchor = Some(bi);
                self.dirty = true;
                self.refresh_graph();
            }
            Some(Action::Promote(r)) => {
                let pi = self.map.promote(&self.idx, r, 1, Author::Human);
                self.sel_path = Some(pi);
                self.sel_anchor = None;
                self.dirty = true;
                self.show_path(pi);
                self.tab = Tab::Graph;
            }
            Some(Action::ExpandCallers(r)) => {
                self.graph.toggle(&self.idx, &self.map, r, false);
                self.relayout();
            }
            Some(Action::ExpandCallees(r)) => {
                self.graph.toggle(&self.idx, &self.map, r, true);
                self.relayout();
            }
            Some(Action::ToggleExpand(r)) => {
                if !self.graph.expanded.remove(&r) {
                    self.graph.expanded.insert(r);
                }
                self.graph.size.remove(&r); // re-measure
                self.graph.manual = false;
            }
            Some(Action::AddToPath(r)) => {
                let s = self.idx.sym(r);
                let (start, end) = (s.start, s.end);
                self.add_step(r.file, start, end, Some(r));
            }
            Some(Action::Relayout) => {
                self.graph.manual = false;
                self.graph.layout(&self.idx, self.metrics);
            }
            None => {}
        }
    }
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_owned() } else { s.chars().take(n.saturating_sub(1)).chain(std::iter::once('…')).collect() }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
            self.save();
        }
        let mono = TextStyle::Monospace.resolve(&ctx.style()); // resolve outside the fonts lock: ctx.style() inside it deadlocks
        self.metrics = ctx.fonts_mut(|f| Metrics { line_h: f.row_height(&mono), char_w: f.glyph_width(&mono, 'M') });
        ctx.request_repaint_after(Duration::from_secs(1)); // keep polling while idle
        self.poll_disk();

        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                ui.label("search");
                if ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(220.0)).lost_focus() && enter {
                    self.run_search();
                }
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Graph, "Graph");
                ui.selectable_value(&mut self.tab, Tab::Listing, "Listing");
                ui.selectable_value(&mut self.tab, Tab::Results, format!("Results ({})", self.results.len()));
                ui.separator();
                ui.label("new path");
                if ui.add(egui::TextEdit::singleline(&mut self.new_path).desired_width(140.0)).lost_focus() && enter {
                    self.create_path();
                }
                if ui.button("add selection to path").on_hover_text("listing: selected lines. graph: focused node. Goes under the selected step.").clicked() {
                    self.add_selection();
                }
                if ui.button("delete selected").on_hover_text("the selected step (its children move up), else the selected path").clicked() {
                    self.delete_selected();
                }
                if ui.button(if self.dirty { "save *" } else { "save" }).clicked() {
                    self.save();
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.monospace(self.map_path.display().to_string());
                ui.separator();
                ui.label(&self.status);
            });
        });

        egui::TopBottomPanel::bottom("output").resizable(true).default_height(160.0).show(ctx, |ui| self.output_panel(ui));

        let mut action = None;
        egui::SidePanel::left("left").default_width(420.0).resizable(true).show(ctx, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            let half = ui.available_height() / 2.0;
            ui.allocate_ui(vec2(ui.available_width(), half), |ui| action = self.symbols_window(ui));
            ui.separator();
            ui.allocate_ui(vec2(ui.available_width(), ui.available_height()), |ui| action = self.paths_window(ui).or(action.take()));
        });
        egui::SidePanel::right("xrefs").default_width(300.0).resizable(true).show(ctx, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            action = self.xrefs_window(ui).or(action.take());
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Graph => action = self.graph_view(ui).or(action.take()),
            Tab::Listing => self.listing(ui),
            Tab::Results => self.results_view(ui),
        });
        self.apply(action);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.dirty {
            self.save();
        }
    }
}

// ---- entry -------------------------------------------------------------------------

fn cli_main(root: &Path, args: &[String]) -> i32 {
    let cmd = match cli::parse(args) {
        Ok(cmd) => cmd,
        Err(e) if e.use_stderr() => {
            eprint!("{e}");
            return 2;
        }
        Err(e) => {
            print!("{e}");
            return 0;
        }
    };
    let idx = index::build(root);
    let map_path = root.join(".codemap");
    let map = Map::load(&map_path);
    if map.is_none() && map_path.exists() {
        eprintln!("{}: unreadable or an old format, starting from an empty map", map_path.display());
    }
    let mut map = map.unwrap_or_default();
    map.resolve_all(&idx);
    let mut out = String::new();
    // A closed pipe (`| head`) is not an error worth a panic.
    let emit = |s: &str| {
        let _ = std::io::Write::write_all(&mut std::io::stdout(), s.as_bytes());
    };
    match cli::exec(&idx, &mut map, cmd, Author::Ai, &mut out) {
        Ok(dirty) => {
            emit(&out);
            if dirty {
                if let Err(e) = map.save(&map_path) {
                    eprintln!("save failed: {e}");
                    return 1;
                }
                println!("saved {}", map_path.display());
            }
            0
        }
        Err(e) => {
            emit(&out);
            eprintln!("{e}");
            2
        }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "help" || a == "--help" || a == "-h") {
        print!("{}", cli::help());
        return Ok(());
    }
    let root = args.first().map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    if args.len() > 1 {
        std::process::exit(cli_main(&root, &args[1..]));
    }

    let title = format!("codemap - {}", root.display());
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([2400.0, 1350.0]).with_position([0.0, 0.0]).with_maximized(true).with_title(&title),
        ..Default::default()
    };
    eframe::run_native("codemap", options, Box::new(move |_cc| Ok(Box::new(App::new(&root)))))
}
