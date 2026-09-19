mod cli;
mod index;
mod lsp;
mod map;

use eframe::egui::{self, Align2, Color32, FontId, Key, Modifiers, Sense, Stroke, TextStyle, pos2, text::LayoutJob, vec2};
use index::{Index, ServerFile, Span, SymRef};
use map::{Author, Change, Kind, Map, PathDiff, StepChange};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Path,
    Diff,
    Graph,
    Listing,
    Results,
}

#[derive(PartialEq, Clone, Copy)]
enum LeftTab {
    Paths,
    Symbols,
    Files,
}

/// Width of the line-number gutter in a code row.
const GUTTER: f32 = 64.0;

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

/// A graph node: a symbol, and the step it stands for when it is one. Two slice steps of one
/// symbol are two nodes; an expansion that reveals a symbol some step already shows reuses
/// that step's node.
type Node = (SymRef, Option<usize>);

/// Nodes are symbols showing their code, or a step's slice of it.
///
/// What is visible is *derived* from the selection: the selected path's tree of steps (or the
/// selected symbol alone), plus an ordered list of expansions ("callers of X", "callees of X").
/// Toggling an expansion off removes it from the list and rebuilds, so nodes that were only
/// reachable through it disappear too. A selected symbol off the path joins as its own root.
///
/// Layout is a left-to-right tree: a step's column is its depth, an expansion node sits one
/// column beside what revealed it. Siblings stack in call order beside their parent. Dragging a
/// node switches to manual until the next structural change.
#[derive(Default)]
struct Graph {
    nodes: Vec<Node>,
    col: HashMap<Node, i32>,
    pos: HashMap<Node, egui::Pos2>,
    size: HashMap<Node, egui::Vec2>,      // measured last frame
    range: HashMap<Node, (usize, usize)>, // lines a node shows: the step's slice, or the whole symbol
    by_sym: HashMap<SymRef, Node>,        // the node edges and expansions land on for a symbol
    focus: Option<SymRef>,                // the selected symbol
    focus_step: Option<usize>,            // the selected step, when the symbol was reached through one
    path_id: Option<usize>,
    step: HashMap<Node, (usize, usize, String)>, // step node -> (pre-order position, anchor index, hierarchical number)
    step_parent: HashMap<Node, Node>,            // step node -> its parent step's node
    origin: HashMap<Node, (Node, bool)>,         // expansion node -> (the node that revealed it, via callees?)
    expansions: Vec<(Node, bool)>,               // (node, callees?) in the order the user opened them
    collapsed: HashSet<Node>,                    // nodes cut to a preview
    call_rows: HashMap<Node, f32>,               // y of each node's first code row, measured last frame
    viewport: egui::Vec2,                        // the canvas size, for a 1:1 look at the focus
    manual: bool,                                // user dragged something: keep positions
    scene_rect: Option<egui::Rect>,
}

impl Graph {
    fn clear(&mut self) {
        *self = Graph::default();
    }

    /// The node the selection sits on.
    fn focus_node(&self) -> Option<Node> {
        let f = self.focus?;
        let stepped = (f, self.focus_step);
        if self.col.contains_key(&stepped) { Some(stepped) } else { self.by_sym.get(&f).copied() }
    }

    /// Lines shown for `n`: (shown, total).
    fn lines_shown(&self, n: Node) -> (usize, usize) {
        let (lo, hi) = self.range[&n];
        let total = hi - lo + 1;
        (if self.collapsed.contains(&n) { total.min(PREVIEW_LINES) } else { total }, total)
    }

    /// Characters of code that fit on one row of `n`'s node (lines are cut to this).
    fn max_chars(&self, idx: &Index, n: Node, m: Metrics) -> usize {
        (((self.node_size(idx, n, m).x - 12.0) / m.char_w) as usize).saturating_sub(6)
    }

    /// Width from the longest shown line (clamped); height from the measured frame, else estimated.
    fn node_size(&self, idx: &Index, n: Node, m: Metrics) -> egui::Vec2 {
        let (lo, _) = self.range[&n];
        let (shown, total) = self.lines_shown(n);
        let longest = idx.files[n.0.file].lines[lo..lo + shown].iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let w = (12.0 + (longest + 6) as f32 * m.char_w).clamp(HEADER_MIN_W, NODE_MAX_W);
        let h = match self.size.get(&n) {
            Some(sz) => sz.y,
            None => HEADER_H + (shown + usize::from(shown < total)) as f32 * m.line_h + 12.0,
        };
        vec2(w, h)
    }

    fn node_rect(&self, idx: &Index, n: Node, m: Metrics) -> egui::Rect {
        egui::Rect::from_min_size(self.pos.get(&n).copied().unwrap_or(pos2(0.0, 0.0)), self.node_size(idx, n, m))
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

    fn is_expanded(&self, n: Node, callees: bool) -> bool {
        self.expansions.contains(&(n, callees))
    }

    /// The first line of `n` that names `word` as an identifier: where a call to it sits.
    fn call_line(&self, idx: &Index, n: Node, word: &str) -> Option<usize> {
        let (lo, hi) = self.range[&n];
        let lines = &idx.files[n.0.file].lines;
        (lo..=hi.min(lines.len().saturating_sub(1))).find(|&li| has_word(&lines[li], word))
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

    /// Where an edge out of `n` towards `word` starts: level with the call line when it is on
    /// screen, else the header.
    fn edge_out(&self, idx: &Index, n: Node, word: &str, m: Metrics) -> egui::Pos2 {
        let r = self.node_rect(idx, n, m);
        let (lo, _) = self.range[&n];
        let (shown, _) = self.lines_shown(n);
        match (self.call_line(idx, n, word), self.call_rows.get(&n)) {
            (Some(li), Some(&top)) if li < lo + shown => pos2(r.right(), top + (li - lo) as f32 * m.line_h + m.line_h / 2.0),
            _ => r.right_top() + vec2(0.0, HEADER_H / 2.0),
        }
    }

    /// Derive the visible node set from the selection + expansions.
    fn rebuild(&mut self, idx: &Index, map: &Map) {
        self.nodes.clear();
        self.col.clear();
        self.range.clear();
        self.by_sym.clear();
        self.step.clear();
        self.step_parent.clear();
        self.origin.clear();
        if let Some(pi) = self.path_id.filter(|&pi| pi < map.paths.len()) {
            let anchors = &map.paths[pi].anchors;
            let mut node_of: HashMap<usize, Node> = HashMap::new(); // anchor index -> node
            for (k, (ai, depth, number)) in map.numbered(pi).into_iter().enumerate() {
                let a = &anchors[ai];
                let Some(fi) = idx.find_file(&a.file) else { continue };
                let Some(si) = a.sym else { continue };
                let n = (SymRef { file: fi, sym: si }, Some(ai));
                self.add(idx, n, depth as i32, (a.line_start, a.line_end));
                self.step.insert(n, (k, ai, number));
                node_of.insert(ai, n);
                if a.parent >= 0 {
                    if let Some(&p) = node_of.get(&(a.parent as usize)) {
                        self.step_parent.insert(n, p);
                    }
                }
            }
        } else {
            self.path_id = None;
        }
        if let Some(f) = self.focus {
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
        self.manual = false;
    }

    /// A lone symbol with its callers and callees open.
    fn build_around(&mut self, idx: &Index, map: &Map, r: SymRef) {
        self.clear();
        self.focus = Some(r);
        self.expansions = vec![((r, None), false), ((r, None), true)];
        self.rebuild(idx, map);
    }

    fn build_path(&mut self, idx: &Index, map: &Map, pi: usize) {
        self.clear();
        self.path_id = Some(pi);
        self.rebuild(idx, map);
    }

    /// Open or close the callers/callees of `n`.
    fn toggle(&mut self, idx: &Index, map: &Map, n: Node, callees: bool) {
        match self.expansions.iter().position(|e| *e == (n, callees)) {
            Some(i) => {
                self.expansions.remove(i);
            }
            None => self.expansions.push((n, callees)),
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
    /// Roots (path roots in step order, then anything else without a parent) stack top to
    /// bottom. A final pass pushes apart the rare column collisions between different subtrees.
    fn layout(&mut self, idx: &Index, m: Metrics) {
        if self.nodes.is_empty() {
            return;
        }

        // x
        let mut cols: BTreeMap<i32, Vec<Node>> = BTreeMap::new();
        for &r in &self.nodes {
            cols.entry(self.col[&r]).or_default().push(r);
        }
        let (first, last) = (*cols.keys().next().unwrap(), *cols.keys().last().unwrap());
        let anchor_col = 0.clamp(first, last);
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
        for kids in right.values_mut() {
            kids.sort_by_key(|k| self.step.get(k).map_or(usize::MAX, |s| s.0)); // steps first, in step order
        }
        let mut roots: Vec<Node> = self.nodes.iter().copied().filter(|n| !has_parent.contains(n)).collect();
        roots.sort_by_key(|r| self.step.get(r).map_or(usize::MAX, |s| s.0));

        // pass 1: block heights, bottom-up
        let mut height: HashMap<Node, f32> = HashMap::new();
        fn measure(g: &Graph, idx: &Index, m: Metrics, r: Node, right: &HashMap<Node, Vec<Node>>, left: &HashMap<Node, Vec<Node>>, height: &mut HashMap<Node, f32>, seen: &mut HashSet<Node>) -> f32 {
            if !seen.insert(r) {
                return 0.0;
            }
            let stack = |g: &Graph, kids: Option<&Vec<Node>>, height: &mut HashMap<Node, f32>, seen: &mut HashSet<Node>| -> f32 {
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
        fn place(g: &mut Graph, idx: &Index, m: Metrics, r: Node, top: f32, col_x: &BTreeMap<i32, f32>, right: &HashMap<Node, Vec<Node>>, left: &HashMap<Node, Vec<Node>>, height: &HashMap<Node, f32>, done: &mut HashSet<Node>) {
            if !done.insert(r) {
                return;
            }
            let block = height[&r];
            let own = g.node_size(idx, r, m).y;
            g.pos.insert(r, pos2(col_x[&g.col[&r]], top + (block - own) / 2.0));
            for kids in [right.get(&r), left.get(&r)] {
                let kids: Vec<Node> = kids.into_iter().flatten().copied().filter(|k| height.contains_key(k) && !done.contains(k)).collect();
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
        match self.focus_node() {
            Some(r) => {
                let c = self.node_rect(idx, r, m).center();
                let size = if self.viewport.x > 0.0 { self.viewport } else { vec2(1900.0, 1100.0) };
                self.scene_rect = Some(egui::Rect::from_center_size(c, size)); // 1:1, so the text is crisp
            }
            None => self.fit(),
        }
    }
}

/// What the language-server thread sends back.
enum Msg {
    File(ServerFile),
    Progress(String),
    Failed(&'static index::Lang, String),
    Done,
}

struct App {
    idx: Index,
    backend: Option<Receiver<Msg>>, // a server thread is running
    backend_progress: String,
    restart_backend: bool, // the index changed while a server thread ran: run again when it ends
    map: Map,
    base: Option<Map>,                       // the map at the parent revision, for the diff
    base_rx: Option<Receiver<Option<Map>>>, // jj is being asked for it
    map_path: PathBuf,
    map_mtime: Option<SystemTime>,
    dirty: bool,
    last_poll: Instant,
    warned_disk: bool,

    sel_path: Option<usize>,                 // the path being read: outline expanded, document shown
    sel_anchor: Option<usize>,               // the selected step of it, when the selection came through the path
    expanded_steps: HashSet<(usize, usize)>, // (path, step) showing the whole enclosing symbol
    collapsed: HashSet<(usize, usize)>,      // (path, step) with its code hidden
    folded: HashSet<(usize, usize)>,         // (path, step) with its subtree hidden
    editing_note: Option<(usize, Option<usize>)>, // (path, step) whose note is in a text box; None step = the path note
    top_step: Option<usize>,                 // the step whose header is topmost in the document viewport
    scroll_to_step: Option<usize>,           // the document scrolls this step's header to the top next frame
    outline_tracked: Option<usize>,          // the step the outline last scrolled to
    peek: Option<SymRef>,                    // a definition pinned in the right panel
    history: Vec<Loc>,     // places before each navigation, newest last
    forward: Vec<Loc>,     // places left by going back, newest last
    last_loc: Option<Loc>, // the place at the end of the last frame

    tab: Tab,
    left: LeftTab,
    graph: Graph,
    metrics: Metrics,
    cur_file: Option<usize>,
    sel: Option<(usize, usize)>, // (anchor line, active line) of the line selection
    scroll_to: Option<usize>,

    sym_filter: String,
    goto_line: String,
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
            sel_path: first_path,
            sel_anchor: None,
            expanded_steps: HashSet::new(),
            collapsed: HashSet::new(),
            folded: HashSet::new(),
            editing_note: None,
            top_step: None,
            scroll_to_step: None,
            outline_tracked: None,
            peek: None,
            history: Vec::new(),
            forward: Vec::new(),
            last_loc: None,
            tab: Tab::Path,
            left: LeftTab::Paths,
            graph: Graph::default(),
            metrics: Metrics { line_h: 15.0, char_w: 7.5 },
            cur_file: None,
            sel: None,
            scroll_to: None,
            sym_filter: String::new(),
            goto_line: String::new(),
            search: String::new(),
            new_path: String::new(),
            results: Vec::new(),
            cmd: String::new(),
            output: "type 'help' for commands; roots and promote live here\n".into(),
            status,
        };
        if let Some(pi) = first_path {
            app.select_path(pi);
        }
        app.start_backend();
        app.load_base();
        app
    }

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

    fn here(&self) -> Loc {
        Loc {
            tab: self.tab,
            file: self.cur_file.map(|fi| self.idx.files[fi].path.clone()),
            sel: self.sel,
            focus: self.graph.focus.map(|r| self.idx.key(r)),
            path: self.sel_path,
            step: self.sel_anchor,
        }
    }

    /// Once a frame: whatever moved the reader (a click anywhere, a tab, a command), the place
    /// they left goes into the history and the forward stack is dropped.
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
            self.scroll_to = Some(a.saturating_sub(3));
        }
        self.last_loc = Some(self.here());
    }

    /// Put `r` in the graph and centre on it. Off the current tree, it becomes its own root with
    /// callers and callees open.
    fn focus_graph(&mut self, r: SymRef) {
        self.graph.focus = Some(r);
        if !self.graph.by_sym.contains_key(&r) {
            if self.graph.path_id.is_none() {
                self.graph.build_around(&self.idx, &self.map, r);
            } else {
                let n = (r, None);
                self.graph.expansions.retain(|(m, _)| *m != n);
                self.graph.expansions.push((n, false));
                self.graph.expansions.push((n, true));
                self.graph.rebuild(&self.idx, &self.map);
            }
        }
        self.relayout();
        self.graph.look_at_focus(&self.idx, self.metrics);
    }

    /// Make `r` the selected symbol: listing position, xrefs, and graph focus. Moves the view,
    /// not the tab. Callers decide whether a step stands behind it.
    fn focus(&mut self, r: SymRef) {
        let s = self.idx.sym(r);
        let (start, end) = (s.start, s.end);
        self.cur_file = Some(r.file);
        self.sel = Some((start, end));
        self.scroll_to = Some(start.saturating_sub(3));
        self.focus_graph(r);
    }

    /// Select a symbol reached outside any path. When it is a whole-symbol step of the path
    /// being read, that step is selected instead so every view agrees.
    fn select_symbol(&mut self, r: SymRef) {
        if let Some(pi) = self.sel_path {
            let file = &self.idx.files[r.file].path;
            if let Some(ai) = self.map.paths[pi].anchors.iter().position(|a| a.sym == Some(r.sym) && a.file == *file && a.off_start == 0) {
                self.select_step(pi, ai, false);
                return;
            }
        }
        self.sel_anchor = None;
        self.graph.focus_step = None;
        self.focus(r);
    }

    /// Select a step. The graph shows its path; the document scrolls its header to the top
    /// unless the click came from inside the document.
    fn select_step(&mut self, pi: usize, ai: usize, in_document: bool) {
        self.sel_path = Some(pi);
        self.sel_anchor = Some(ai);
        if self.graph.path_id != Some(pi) {
            self.graph.build_path(&self.idx, &self.map, pi);
        }
        self.graph.focus_step = Some(ai);
        let a = &self.map.paths[pi].anchors[ai];
        let (file, sym, ls, le) = (a.file.clone(), a.sym, a.line_start, a.line_end);
        match (self.idx.find_file(&file), sym) {
            (Some(fi), Some(si)) => {
                self.focus(SymRef { file: fi, sym: si });
                self.sel = Some((ls, le));
                self.scroll_to = Some(ls.saturating_sub(3));
            }
            (Some(fi), None) => {
                self.cur_file = Some(fi);
                self.sel = Some((ls, le));
                self.scroll_to = Some(ls.saturating_sub(3));
            }
            _ => {}
        }
        if !in_document {
            self.scroll_to_step = Some(ai);
        }
        if matches!(self.tab, Tab::Listing | Tab::Results | Tab::Diff) {
            self.tab = Tab::Path;
        }
    }

    /// Open a path for reading: its first step is selected.
    fn select_path(&mut self, pi: usize) {
        self.sel_path = Some(pi);
        self.editing_note = None;
        match self.map.tree_order(pi).first() {
            Some(&(ai, _)) => self.select_step(pi, ai, false),
            None => {
                self.sel_anchor = None;
                self.graph.build_path(&self.idx, &self.map, pi);
            }
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

    /// Pin lines as a step of the selected path, under the selected step (else a root). The
    /// new step becomes the selected one so repeated pins build a chain.
    fn add_step(&mut self, fi: usize, ls: usize, le: usize) {
        let Some(pi) = self.sel_path else {
            self.status = "select a path first".into();
            return;
        };
        let parent = self.sel_anchor.filter(|&ai| ai < self.map.paths[pi].anchors.len()).map_or(-1, |ai| ai as i32);
        let ai = self.map.add_anchor(&self.idx, pi, fi, ls, le, Author::Human, parent);
        self.sel_anchor = Some(ai);
        self.dirty = true;
        self.status = format!("step [{ai}] added to '{}' under [{parent}]", self.map.paths[pi].name);
        self.refresh_graph();
    }

    /// Top-bar button: the listing's selected lines.
    fn add_selection(&mut self) {
        match (self.tab, self.cur_file, self.sel) {
            (Tab::Listing, Some(fi), Some((a, b))) => self.add_step(fi, a.min(b), a.max(b)),
            _ => self.status = "select lines in the listing first".into(),
        }
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
    fn symbol_at(&self, fi: usize, li: usize, col: usize) -> Option<SymRef> {
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

    /// A hovered code row: the definition under the pointer as a tooltip, alt-click pins it in
    /// the peek panel, ctrl-click or double-click goes there.
    fn code_hover(&self, resp: &egui::Response, fi: usize, li: usize, col: usize) -> Option<Action> {
        let r = self.symbol_at(fi, li, col)?;
        let (alt, ctrl) = resp.ctx.input(|i| (i.modifiers.alt, i.modifiers.command));
        if resp.clicked() && alt {
            return Some(Action::Peek(r));
        }
        if resp.double_clicked() || (resp.clicked() && ctrl) {
            return Some(Action::Jump(fi, li, col));
        }
        resp.clone().on_hover_ui_at_pointer(|ui| self.peek_tooltip(ui, r));
        None
    }

    /// A symbol's definition, as a tooltip: header and the first lines of its code.
    fn peek_tooltip(&self, ui: &mut egui::Ui, r: SymRef) {
        let s = self.idx.sym(r);
        ui.strong(&s.name);
        ui.weak(format!("{} {}:{}-{}", s.kind, self.idx.files[r.file].path, s.start + 1, s.end + 1));
        self.code_block(ui, r, 24);
        ui.weak("alt-click: pin in the peek panel   ctrl-click or double-click: go there");
    }

    /// The first `max` lines of a symbol, syntax coloured, one label.
    fn code_block(&self, ui: &mut egui::Ui, r: SymRef, max: usize) {
        let s = self.idx.sym(r);
        let f = &self.idx.files[r.file];
        let font = TextStyle::Monospace.resolve(ui.style());
        let (base, dim) = (ui.visuals().text_color(), ui.visuals().weak_text_color());
        let numfmt = egui::TextFormat { font_id: font.clone(), color: dim, ..Default::default() };
        let mut job = LayoutJob::default();
        let end = s.end.min(f.lines.len().saturating_sub(1)).min(s.start + max - 1);
        for li in s.start..=end {
            job.append(&format!("{:4} ", li + 1), 0.0, numfmt.clone());
            append_line(&mut job, &f.lines[li], &f.hl[li], &font, base, 120);
            job.append("\n", 0.0, numfmt.clone());
        }
        if end < s.end {
            job.append(&format!("     … {} more lines", s.end - end), 0.0, numfmt.clone());
        }
        ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
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
                self.expanded_steps.clear();
                self.collapsed.clear();
                self.folded.clear();
                self.editing_note = None;
                if self.sel_path.is_some_and(|pi| pi >= self.map.paths.len()) {
                    self.sel_path = None;
                }
                self.refresh_graph();
                self.load_base();
                self.status = "map reloaded (changed on disk)".into();
            }
        }

        if self.idx.changed() {
            self.reindex();
        }
    }

    /// Run `change` on the index and carry the graph, the listing and the map's anchors over by
    /// (file, symbol) identity, since symbol indices do not survive it.
    fn with_index_change(&mut self, change: impl FnOnce(&mut App)) {
        let focus = self.graph.focus.map(|r| self.idx.key(r));
        let expansions: Vec<((String, String), Option<usize>, bool)> = self.graph.expansions.iter().map(|(n, c)| (self.idx.key(n.0), n.1, *c)).collect();
        let collapsed: Vec<((String, String), Option<usize>)> = self.graph.collapsed.iter().map(|n| (self.idx.key(n.0), n.1)).collect();
        let cur_file = self.cur_file.map(|fi| self.idx.files[fi].path.clone());
        let peek = self.peek.map(|r| self.idx.key(r));

        change(self);
        self.peek = peek.and_then(|k| self.idx.by_key(&k));
        self.map.resolve_all(&self.idx);
        self.graph.focus = focus.and_then(|k| self.idx.by_key(&k));
        self.graph.expansions = expansions.into_iter().filter_map(|(k, st, c)| self.idx.by_key(&k).map(|r| ((r, st), c))).collect();
        self.graph.collapsed = collapsed.iter().filter_map(|(k, st)| self.idx.by_key(k).map(|r| (r, *st))).collect();
        self.graph.size.clear();
        self.refresh_graph();
        self.cur_file = cur_file.and_then(|p| self.idx.find_file(&p));
    }

    /// Rebuild the index from disk and the cache, then ask the servers about what changed.
    fn reindex(&mut self) {
        self.with_index_change(|app| app.idx = index::build(&app.idx.root));
        self.results.clear();
        self.status = format!("re-indexed: {} files, {} symbols (source changed)", self.idx.files.len(), self.idx.files.iter().map(|f| f.symbols.len()).sum::<usize>());
        self.start_backend();
    }
}

// ---- windows ----------------------------------------------------------------------

enum Action {
    Focus(SymRef),
    OpenListing(SymRef),
    SelectPath(usize),
    ShowPath(usize),
    SelectStep(usize, usize, bool), // (path, step, clicked inside the document)
    ToggleStep(usize, usize),       // show the whole symbol / just the slice in the document
    ToggleCode(usize, usize),       // hide / show a step's code
    ToggleFold(usize, usize),       // hide / show a step's subtree
    CollapseAll(usize, bool),
    EditNote(usize, Option<usize>), // open a note for editing; None = the path note
    DeleteStep(usize, usize),
    DeletePath(usize),
    ExpandCallers(Node),
    ExpandCallees(Node),
    ToggleExpand(Node),
    Peek(SymRef),              // pin a definition in the right panel
    ClosePeek,
    GoTo(usize, usize),        // (file, line) in the listing
    Jump(usize, usize, usize), // (file, line, column): to the definition of the identifier there
    Relayout,
}

impl App {
    fn symbols_window(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.strong("Symbols");
            ui.weak("+ = in a path");
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
                let covered = if self.map.covers(&self.idx.files[r.file].path, s.start, s.end) { "+" } else { " " };
                let text = format!("{covered} {}{:<26} {:<12} {}:{}", if s.depth > 0 { "  " } else { "" }, trunc(&s.name, 26), trunc(&s.kind, 12), self.idx.files[r.file].path, s.start + 1);
                let text = egui::RichText::new(text).monospace();
                let text = if self.idx.files[r.file].pending { text.weak() } else { text };
                let resp = ui.selectable_label(cur == Some(*r), text);
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
            ui.weak("click to read");
        });
        let diffs = self.diffs();
        let track = self.top_step.filter(|_| self.outline_tracked != self.top_step);
        egui::ScrollArea::vertical().id_salt("paths").auto_shrink(false).show(ui, |ui| {
            for (pi, p) in self.map.paths.iter().enumerate() {
                let stale = p.anchors.iter().filter(|a| a.stale).count();
                let mark = match diffs.iter().find(|d| d.name == p.name).map(|d| d.change) {
                    Some(Change::Added) => "+ ",
                    Some(Change::Changed) => "~ ",
                    _ => "",
                };
                let mut text = egui::RichText::new(format!("{mark}{} [{}]{}  {} steps", p.name, p.kind.name(), p.author.tag(), p.anchors.len()));
                if stale > 0 {
                    text = text.color(Color32::LIGHT_RED);
                } else if !mark.is_empty() {
                    text = text.color(Color32::LIGHT_GREEN);
                }
                let selected = self.sel_path == Some(pi);
                let resp = ui.selectable_label(selected, text);
                if resp.clicked() {
                    action = Some(Action::SelectPath(pi));
                }
                if stale > 0 {
                    resp.on_hover_text(format!("{stale} stale steps: the code changed since they were pinned"));
                }
                if !selected {
                    continue;
                }
                // the outline: every step of the path being read, folded subtrees hidden
                let mut hide_below: Option<usize> = None;
                for (ai, depth, number) in self.map.numbered(pi) {
                    if hide_below.is_some_and(|d| depth > d) {
                        continue;
                    }
                    hide_below = None;
                    let a = &p.anchors[ai];
                    let hidden = if self.folded.contains(&(pi, ai)) {
                        hide_below = Some(depth);
                        self.map.descendants(pi, ai)
                    } else {
                        0
                    };
                    let name = if a.symbol.is_empty() { "(lines)" } else { a.symbol.as_str() };
                    let mut text = egui::RichText::new(format!("{}{number}  {name}{}", "    ".repeat(depth), if hidden > 0 { format!("  +{hidden}") } else { String::new() }));
                    if a.stale {
                        text = text.color(Color32::LIGHT_RED);
                    }
                    let at_top = self.top_step == Some(ai);
                    let resp = ui.selectable_label(at_top || self.sel_anchor == Some(ai), text).on_hover_text(&a.file);
                    if resp.clicked() {
                        action = Some(Action::SelectStep(pi, ai, false));
                    }
                    if track == Some(ai) {
                        resp.scroll_to_me(Some(egui::Align::Center));
                    }
                }
            }
            for d in diffs.iter().filter(|d| d.change == Change::Removed) {
                ui.weak(format!("- {}  (removed, {} steps)", d.name, d.removed.len()));
            }
        });
        if track.is_some() {
            self.outline_tracked = track;
        }
        action
    }

    /// The map against the parent revision's: what an agent session changed.
    fn diff_view(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.strong("Changes against the parent revision");
            ui.weak("jj file show -r @- .codemap");
            if ui.small_button("refresh").clicked() {
                self.load_base();
            }
        });
        ui.separator();
        if self.base.is_none() {
            ui.weak(if self.base_rx.is_some() { "asking jj..." } else { "no map in the parent revision: this needs a jj repo with a committed .codemap" });
            return None;
        }
        let diffs = self.diffs();
        if diffs.iter().all(|d| d.change == Change::Same) {
            ui.weak("no changes");
            return None;
        }
        egui::ScrollArea::vertical().id_salt("diff").auto_shrink(false).show(ui, |ui| {
            for d in &diffs {
                let (mark, color) = match d.change {
                    Change::Same => continue,
                    Change::Added => ("+", Color32::LIGHT_GREEN),
                    Change::Removed => ("-", Color32::LIGHT_RED),
                    Change::Changed => ("~", Color32::LIGHT_GREEN),
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
                let text = egui::RichText::new(format!("{mark} {}   {summary}", d.name)).color(color).strong();
                match self.map.find(&d.name) {
                    Some(pi) => {
                        if ui.selectable_label(false, text).on_hover_text("read it").clicked() {
                            action = Some(Action::SelectPath(pi));
                        }
                    }
                    None => {
                        ui.label(text);
                    }
                }
                ui.indent(&d.name, |ui| {
                    if let Some(pi) = self.map.find(&d.name).filter(|_| d.change == Change::Changed) {
                        for (i, c) in d.steps.iter().enumerate() {
                            if let Some(c) = c {
                                let a = &self.map.paths[pi].anchors[i];
                                ui.weak(format!("{} [{i}] {} {}:{}-{}  {}", if *c == StepChange::Added { "+" } else { "~" }, a.symbol, a.file, a.line_start + 1, a.line_end + 1, c.tag()));
                            }
                        }
                    }
                    for a in &d.removed {
                        ui.weak(format!("- {} {} (removed)", a.file, a.symbol));
                    }
                });
            }
        });
        action
    }

    fn files_window(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.strong("Files");
            ui.weak("covered/total symbols");
        });
        let cov: Vec<(usize, usize)> = self.idx.files.iter().map(|f| (f.symbols.iter().filter(|s| self.map.covers(&f.path, s.start, s.end)).count(), f.symbols.len())).collect();
        egui::ScrollArea::vertical().id_salt("files").auto_shrink(false).show(ui, |ui| {
            let all: Vec<usize> = (0..self.idx.files.len()).collect();
            self.files_tree(ui, &all, 0, &cov, &mut action);
        });
        action
    }

    /// `files` are sorted by path and share their first `depth` components; a run with the
    /// same next component is a directory.
    fn files_tree(&self, ui: &mut egui::Ui, files: &[usize], depth: usize, cov: &[(usize, usize)], action: &mut Option<Action>) {
        let comp = |fi: usize| self.idx.files[fi].path.split('/').nth(depth).unwrap_or("");
        let is_file = |fi: usize| self.idx.files[fi].path.split('/').count() == depth + 1;
        let mut i = 0;
        while i < files.len() {
            let fi = files[i];
            if is_file(fi) {
                let (c, t) = cov[fi];
                let label = if t > 0 { format!("{}  {c}/{t}", comp(fi)) } else { comp(fi).to_owned() };
                let mut text = egui::RichText::new(label);
                if t > 0 && c == 0 {
                    text = text.weak();
                }
                if ui.selectable_label(self.cur_file == Some(fi), text).clicked() {
                    *action = Some(Action::GoTo(fi, 0));
                }
                i += 1;
                continue;
            }
            let dir = comp(fi);
            let j = i + files[i..].iter().take_while(|&&g| !is_file(g) && comp(g) == dir).count();
            let (c, t) = files[i..j].iter().fold((0, 0), |acc, &g| (acc.0 + cov[g].0, acc.1 + cov[g].1));
            let prefix: String = self.idx.files[fi].path.split('/').take(depth + 1).collect::<Vec<_>>().join("/");
            egui::CollapsingHeader::new(format!("{dir}/  {c}/{t}")).id_salt(&prefix).default_open(depth == 0).show(ui, |ui| {
                self.files_tree(ui, &files[i..j], depth + 1, cov, action);
            });
            i = j;
        }
    }

    /// One numbered, syntax-coloured source line in `rect`.
    fn draw_code_line(&self, ui: &egui::Ui, rect: egui::Rect, fi: usize, li: usize, font: &FontId) {
        let f = &self.idx.files[fi];
        let text_color = ui.visuals().text_color();
        let p = ui.painter();
        p.text(rect.left_top() + vec2(8.0, 0.0), Align2::LEFT_TOP, format!("{:5}", li + 1), font.clone(), ui.visuals().weak_text_color());
        let mut job = LayoutJob::default();
        append_line(&mut job, &f.lines[li], &f.hl[li], font, text_color, usize::MAX);
        let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));
        p.galley(rect.left_top() + vec2(GUTTER, 0.0), galley, text_color);
    }

    /// Character column of a pointer position in a code row.
    fn code_col(&self, rect: egui::Rect, pos: egui::Pos2) -> usize {
        ((pos.x - rect.min.x - GUTTER) / self.metrics.char_w).max(0.0) as usize
    }

    /// A note as text; double-click turns it into a text box until it loses focus.
    fn note_view(&mut self, ui: &mut egui::Ui, pi: usize, ai: Option<usize>, hint: &str) -> Option<Action> {
        let editing = self.editing_note == Some((pi, ai));
        let note = match ai {
            Some(ai) => &mut self.map.paths[pi].anchors[ai].note,
            None => &mut self.map.paths[pi].note,
        };
        if editing {
            let resp = ui.add(egui::TextEdit::multiline(note).desired_rows(2).hint_text(hint).desired_width(f32::INFINITY));
            if resp.changed() {
                self.dirty = true;
            }
            if resp.lost_focus() {
                self.editing_note = None;
            } else {
                resp.request_focus();
            }
            return None;
        }
        let text = if note.is_empty() { egui::RichText::new(hint).weak().italics() } else { egui::RichText::new(note.as_str()) };
        let resp = ui.add(egui::Label::new(text).wrap().sense(Sense::click())).on_hover_text("double-click to edit");
        if resp.double_clicked() { Some(Action::EditNote(pi, ai)) } else { None }
    }

    /// The reader's landing view: the selected path as one document. A sticky breadcrumb of
    /// the topmost visible step's ancestors, then the steps in tree order, numbered
    /// hierarchically and indented by depth, each with its note and its lines.
    fn path_document(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let Some(pi) = self.sel_path.filter(|&pi| pi < self.map.paths.len()) else {
            ui.weak(if self.map.paths.is_empty() { "no paths yet: the agent writes them (path-new, path-add in the output panel)" } else { "pick a path on the left" });
            return None;
        };
        let font = TextStyle::Monospace.resolve(ui.style());
        let row_h = ui.text_style_height(&TextStyle::Monospace);
        let diff = self.diffs().into_iter().find(|d| d.name == self.map.paths[pi].name);
        let accent = ui.visuals().selection.stroke.color;
        let slice_bg = ui.visuals().selection.bg_fill.linear_multiply(0.3);
        ui.horizontal(|ui| {
            let p = &self.map.paths[pi];
            ui.heading(&p.name);
            ui.weak(format!("[{}]{}  {} steps", p.kind.name(), p.author.tag(), p.anchors.len()));
            match diff.as_ref().map(|d| d.change) {
                Some(Change::Added) => ui.colored_label(Color32::LIGHT_GREEN, "new since the parent revision"),
                Some(Change::Changed) => ui.colored_label(Color32::LIGHT_GREEN, "changed since the parent revision"),
                _ => ui.label(""),
            };
            if ui.small_button("graph").on_hover_text("the same path as a tree").clicked() {
                action = Some(Action::ShowPath(pi));
            }
            if ui.small_button("collapse all").on_hover_text("hide every step's code").clicked() {
                action = Some(Action::CollapseAll(pi, true));
            }
            if ui.small_button("expand all").clicked() {
                action = Some(Action::CollapseAll(pi, false));
            }
            if ui.small_button("delete path").clicked() {
                action = Some(Action::DeletePath(pi));
            }
        });
        action = self.note_view(ui, pi, None, "what this path is / does (double-click to write)").or(action);
        ui.separator();

        // breadcrumb: the ancestors of the step under the top of the viewport
        let numbered = self.map.numbered(pi);
        let number_of: HashMap<usize, &str> = numbered.iter().map(|(ai, _, n)| (*ai, n.as_str())).collect();
        ui.horizontal(|ui| {
            let mut chain = Vec::new();
            let mut cur = self.top_step;
            while let Some(ai) = cur {
                chain.push(ai);
                cur = usize::try_from(self.map.paths[pi].anchors[ai].parent).ok().filter(|&p| p < self.map.paths[pi].anchors.len());
            }
            if chain.is_empty() {
                ui.weak(" ");
            }
            for (i, &ai) in chain.iter().rev().enumerate() {
                if i > 0 {
                    ui.weak("›");
                }
                let a = &self.map.paths[pi].anchors[ai];
                let name = if a.symbol.is_empty() { "(lines)" } else { a.symbol.as_str() };
                let text = format!("{} {name}", number_of.get(&ai).copied().unwrap_or(""));
                if ui.link(text).on_hover_text(&a.file).clicked() {
                    action = Some(Action::SelectStep(pi, ai, false));
                }
                ui.weak(egui::RichText::new(&a.file).small());
            }
        });
        ui.separator();

        let scroll_to_step = self.scroll_to_step.take();
        let mut top_step = None;
        egui::ScrollArea::both().id_salt("document").auto_shrink(false).show(ui, |ui| {
            let width = ui.available_width().max(2000.0);
            let viewport_top = ui.clip_rect().top();
            let mut hide_below: Option<usize> = None;
            for &(ai, depth, ref number) in &numbered {
                if hide_below.is_some_and(|d| depth > d) {
                    continue;
                }
                hide_below = None;
                let indent = depth as f32 * 24.0;
                let (file, symbol, ls, le, stale, tag) = {
                    let a = &self.map.paths[pi].anchors[ai];
                    (a.file.clone(), a.symbol.clone(), a.line_start, a.line_end, a.stale, a.author.tag())
                };
                let fi = self.idx.find_file(&file);
                let sym = fi.zip(self.map.paths[pi].anchors[ai].sym).map(|(fi, si)| &self.idx.files[fi].symbols[si]).map(|s| (s.start, s.end));
                let gone = match (fi, symbol.is_empty(), sym) {
                    (None, _, _) => Some("file gone"),
                    (Some(_), false, None) => Some("symbol gone"),
                    _ => None,
                };
                let name = if symbol.is_empty() { "(lines)" } else { symbol.as_str() };
                let place = match gone {
                    Some(g) => format!("{file} ({g})"),
                    None => format!("{file}:{}-{}", ls + 1, le + 1),
                };
                let selected = self.sel_anchor == Some(ai);
                let folded = self.folded.contains(&(pi, ai));
                let collapsed = self.collapsed.contains(&(pi, ai));
                let kids = self.map.descendants(pi, ai);
                let header = ui.horizontal(|ui| {
                    ui.add_space(indent);
                    if kids > 0 {
                        if ui.small_button(if folded { "▸" } else { "▾" }).on_hover_text(if folded { "unfold" } else { "fold this call's steps" }).clicked() {
                            action = Some(Action::ToggleFold(pi, ai));
                        }
                    } else {
                        ui.add_space(22.0);
                    }
                    let mut text = egui::RichText::new(format!("{number}  {}{name}  {place}{tag}{}", if stale { "STALE " } else { "" }, if folded { format!("  +{kids}") } else { String::new() })).strong();
                    if stale {
                        text = text.color(Color32::LIGHT_RED);
                    }
                    if ui.selectable_label(selected, text).clicked() {
                        action = Some(Action::SelectStep(pi, ai, true));
                    }
                    if let Some(c) = diff.as_ref().filter(|d| d.change == Change::Changed).and_then(|d| d.steps.get(ai).copied().flatten()) {
                        ui.colored_label(Color32::LIGHT_GREEN, c.tag());
                    }
                    if gone.is_none() && ui.small_button(if collapsed { "code" } else { "hide code" }).clicked() {
                        action = Some(Action::ToggleCode(pi, ai));
                    }
                    if sym.is_some_and(|s| s != (ls, le)) {
                        let whole = self.expanded_steps.contains(&(pi, ai));
                        if ui.small_button(if whole { "slice" } else { "whole symbol" }).clicked() {
                            action = Some(Action::ToggleStep(pi, ai));
                        }
                    }
                    if ui.small_button("delete").on_hover_text("remove this step; its children move up").clicked() {
                        action = Some(Action::DeleteStep(pi, ai));
                    }
                });
                let hrect = header.response.rect;
                if top_step.is_none() || hrect.top() <= viewport_top {
                    top_step = Some(ai); // the step the reader is in: the last header at or above the top
                }
                if scroll_to_step == Some(ai) {
                    ui.scroll_to_rect(hrect, Some(egui::Align::TOP));
                }
                if selected {
                    ui.painter().rect_filled(egui::Rect::from_min_size(hrect.left_top() + vec2(indent, 0.0), vec2(3.0, hrect.height())), 0.0, accent);
                }
                ui.horizontal(|ui| {
                    ui.add_space(indent + 22.0);
                    action = self.note_view(ui, pi, Some(ai), "what this step does for this path (double-click to write)").or(action.take());
                });
                if let (Some(fi), None, false) = (fi, gone, collapsed) {
                    let whole = self.expanded_steps.contains(&(pi, ai));
                    let (lo, hi) = if whole { sym.unwrap_or((ls, le)) } else { (ls, le) };
                    let hi = hi.min(self.idx.files[fi].lines.len().saturating_sub(1));
                    for li in lo..=hi {
                        let (rect, resp) = ui.allocate_exact_size(vec2(width, row_h), Sense::click());
                        let rect = rect.translate(vec2(indent, 0.0));
                        if whole && li >= ls && li <= le {
                            ui.painter().rect_filled(rect, 0.0, slice_bg);
                        }
                        if selected {
                            ui.painter().rect_filled(egui::Rect::from_min_size(rect.left_top(), vec2(3.0, row_h)), 0.0, accent);
                        }
                        self.draw_code_line(ui, rect, fi, li, &font);
                        if let Some(pos) = resp.hover_pos() {
                            action = self.code_hover(&resp, fi, li, self.code_col(rect, pos)).or(action.take());
                        }
                    }
                }
                if folded {
                    hide_below = Some(depth);
                }
                ui.add_space(10.0);
            }
            if let Some(d) = diff.as_ref().filter(|d| !d.removed.is_empty()) {
                ui.separator();
                ui.weak("steps removed since the parent revision:");
                for a in &d.removed {
                    ui.weak(format!("- {} {}{}", a.file, a.symbol, if a.note.is_empty() { String::new() } else { format!("  -- {}", a.note) }));
                }
            }
        });
        if top_step.is_some() {
            self.top_step = top_step;
        }
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
        let pending = self.idx.files[cur.file].pending;
        if pending {
            ui.weak(format!("waiting for {}", index::lang_for(&self.idx.files[cur.file].path).map_or("the server", |l| l.server)));
        }
        ui.separator();
        egui::ScrollArea::vertical().id_salt("xrefs").auto_shrink(false).show(ui, |ui| {
            ui.add_enabled_ui(!pending, |ui| {
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
            ui.separator();
            ui.label(format!("References ({})", s.refs.len()));
            for (path, line) in &s.refs {
                if let Some(fi) = self.idx.find_file(path) {
                    let text = self.idx.files[fi].lines.get(*line as usize).map(|l| l.trim()).unwrap_or("");
                    if ui.selectable_label(false, format!("{path}:{}: {text}", line + 1)).clicked() {
                        action = Some(Action::GoTo(fi, *line as usize));
                    }
                }
            }
            });
        });
        action
    }

    fn graph_view(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        if self.graph.nodes.is_empty() {
            ui.weak("nothing selected: pick a path or a symbol on the left");
            return None;
        }
        ui.horizontal(|ui| {
            ui.weak("drag background to pan, ctrl+scroll to zoom, drag a node's title to move it");
            if ui.small_button("fit").clicked() {
                self.graph.fit();
            }
            if ui.small_button("1:1").on_hover_text("zoom to 1:1 on the selected node; text is crisp only at 1:1").clicked() {
                self.graph.look_at_focus(&self.idx, self.metrics);
            }
            if ui.add_enabled(self.graph.manual, egui::Button::new("auto layout").small()).clicked() {
                action = Some(Action::Relayout);
            }
            if let Some(pi) = self.graph.path_id.filter(|&pi| pi < self.map.paths.len()) {
                ui.separator();
                ui.label(format!("path '{}': green edges are its steps, grey ones expansions, orange a call back up the tree", self.map.paths[pi].name));
            }
        });

        let m = self.metrics;
        self.graph.viewport = ui.available_size();
        if !self.graph.manual {
            self.graph.layout(&self.idx, m); // measured sizes from last frame settle the layout
        }
        let font = TextStyle::Monospace.resolve(ui.style());
        let focus_node = self.graph.focus_node();

        let mut scene_rect = self.graph.scene_rect.unwrap_or(egui::Rect::ZERO);
        egui::Scene::new().zoom_range(0.1..=1.5).show(ui, &mut scene_rect, |ui| {
            let painter = ui.painter().clone();
            let edge = Stroke::new(1.5, ui.visuals().weak_text_color());
            let back = Stroke::new(1.5, Color32::from_rgb(220, 160, 80));
            let step_stroke = Stroke::new(3.0, Color32::from_rgb(90, 200, 120));
            let rect_of = |g: &Graph, n: Node| g.node_rect(&self.idx, n, m);
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
                for bs in self.graph.callees_of(&self.idx, a) {
                    let Some(&b) = self.graph.by_sym.get(&bs) else { continue };
                    if a.0 == bs || self.graph.step_parent.get(&b) == Some(&a) {
                        continue;
                    }
                    let rb = rect_of(&self.graph, b);
                    if rb.left() >= ra.right() {
                        let (p0, p1) = (self.graph.edge_out(&self.idx, a, &self.idx.sym(bs).name, m), rb.left_top() + hy);
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
            // path edges: parent step -> child step
            for (&child, &parent) in &self.graph.step_parent {
                let rb = rect_of(&self.graph, child);
                let (p0, p1) = (self.graph.edge_out(&self.idx, parent, &self.idx.sym(child.0).name, m), rb.left_top() + hy);
                painter.add(curve(p0, p1, step_stroke));
                painter.circle_filled(p1, 4.0, step_stroke.color);
            }

            let base_color = ui.visuals().text_color();
            let dim = ui.visuals().weak_text_color();
            for r in self.graph.nodes.clone() {
                let s = self.idx.sym(r.0);
                let rect = rect_of(&self.graph, r);
                let focused = focus_node == Some(r);
                let on_path = self.graph.step.contains_key(&r);
                let off_path = self.graph.path_id.is_some() && !on_path;
                let border = if focused {
                    Stroke::new(2.0, ui.visuals().selection.stroke.color)
                } else if on_path {
                    Stroke::new(2.0, step_stroke.color)
                } else {
                    Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
                };
                let fill = if off_path { ui.visuals().panel_fill } else { ui.visuals().extreme_bg_color };
                let step = self.graph.step.get(&r).cloned();
                let mut hover: Option<(egui::Response, usize, usize)> = None;

                let frame = ui.scope_builder(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(rect.min, vec2(rect.width(), f32::INFINITY))), |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    egui::Frame::new().fill(fill).stroke(border).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                        ui.set_width(rect.width() - 12.0);
                        // title = drag handle; buttons registered after it so they stay clickable
                        ui.horizontal(|ui| {
                            let mut title = match &step {
                                Some((_, _, number)) => ui.colored_label(step_stroke.color, egui::RichText::new(number).strong()).union(ui.strong(&s.name)),
                                None => ui.strong(&s.name),
                            };
                            if off_path {
                                title = title.union(ui.weak("off path"));
                            }
                            let (lo, hi) = self.graph.range[&r];
                            title = title.union(ui.weak(format!("{}:{}-{}", self.idx.files[r.0.file].path, lo + 1, hi + 1)));
                            let drag = ui.interact(title.rect, ui.id().with(("drag", r)), Sense::click_and_drag());
                            if drag.dragged() {
                                *self.graph.pos.get_mut(&r).unwrap() += drag.drag_delta();
                                self.graph.manual = true;
                            }
                            if drag.clicked() {
                                action = Some(match (&step, self.graph.path_id) {
                                    (Some((_, ai, _)), Some(pi)) => Action::SelectStep(pi, *ai, false),
                                    _ => Action::Focus(r.0),
                                });
                            }
                            drag.on_hover_text("drag to move, click to select");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let (shown, total) = self.graph.lines_shown(r);
                                if total > PREVIEW_LINES && ui.small_button(if shown < total { "more" } else { "less" }).on_hover_text("show all / fewer lines").clicked() {
                                    action = Some(Action::ToggleExpand(r));
                                }
                                if ui.small_button("listing").on_hover_text("open in the listing").clicked() {
                                    action = Some(Action::OpenListing(r.0));
                                }
                                let (out_open, in_open) = (self.graph.is_expanded(r, true), self.graph.is_expanded(r, false));
                                let n_callees = self.graph.callees_of(&self.idx, r).len();
                                if ui.add_enabled(n_callees > 0, egui::Button::new(format!("callees > {n_callees}")).small().selected(out_open)).on_hover_text("show / hide what this calls").clicked() {
                                    action = Some(Action::ExpandCallees(r));
                                }
                                if ui.add_enabled(!s.callers.is_empty(), egui::Button::new(format!("{} < callers", s.callers.len())).small().selected(in_open)).on_hover_text("show / hide what calls this").clicked() {
                                    action = Some(Action::ExpandCallers(r));
                                }
                            });
                        });
                        // step note: the human's or the AI's annotation for this step of the path
                        if let (Some((_, ai, _)), Some(pi)) = (step, self.graph.path_id) {
                            let note = &self.map.paths[pi].anchors[ai].note;
                            if !note.is_empty() {
                                ui.label(egui::RichText::new(note).italics().color(step_stroke.color));
                            }
                        }
                        ui.separator();
                        let f = &self.idx.files[r.0.file];
                        let (shown, total) = self.graph.lines_shown(r);
                        let max_chars = self.graph.max_chars(&self.idx, r, m);
                        let mut job = LayoutJob::default();
                        let numfmt = egui::TextFormat { font_id: font.clone(), color: dim, ..Default::default() };
                        let (lo, _) = self.graph.range[&r];
                        for li in lo..lo + shown {
                            job.append(&format!("{:4} ", li + 1), 0.0, numfmt.clone());
                            append_line(&mut job, &f.lines[li], &f.hl[li], &font, base_color, max_chars);
                            job.append("\n", 0.0, numfmt.clone());
                        }
                        if shown < total {
                            job.append(&format!("     … {} more lines", total - shown), 0.0, numfmt.clone());
                        }
                        ui.set_clip_rect(egui::Rect::from_min_size(rect.min, vec2(rect.width(), f32::INFINITY)).intersect(ui.clip_rect()));
                        let code = ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend).sense(Sense::click()));
                        self.graph.call_rows.insert(r, code.rect.top());
                        // the lines that call the nodes hanging off this one
                        let children: Vec<Node> = self.graph.nodes.iter().copied().filter(|c| self.graph.step_parent.get(c) == Some(&r) || self.graph.origin.get(c).is_some_and(|(o, callees)| *o == r && *callees)).collect();
                        for c in children {
                            if let Some(li) = self.graph.call_line(&self.idx, r, &self.idx.sym(c.0).name).filter(|&li| li < lo + shown) {
                                let row = egui::Rect::from_min_size(pos2(code.rect.left(), code.rect.top() + (li - lo) as f32 * m.line_h), vec2(code.rect.width(), m.line_h));
                                ui.painter().rect_filled(row, 0.0, step_stroke.color.gamma_multiply(0.18));
                            }
                        }
                        if let Some(pos) = code.hover_pos() {
                            let li = lo + ((pos.y - code.rect.top()) / m.line_h).max(0.0) as usize;
                            let col = (((pos.x - code.rect.left()) / m.char_w) as usize).saturating_sub(5);
                            if li < lo + shown {
                                hover = Some((code, li, col));
                            }
                        }
                    })
                });
                if let Some((code, li, col)) = hover {
                    action = self.code_hover(&code, r.0.file, li, col).or(action.take());
                }
                self.graph.size.insert(r, vec2(rect.width(), frame.inner.response.rect.height()));
            }
        });
        self.graph.scene_rect = Some(scene_rect);
        action
    }

    fn listing(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let Some(fi) = self.cur_file else {
            ui.weak("click a symbol to open its file");
            return None;
        };
        let font = TextStyle::Monospace.resolve(ui.style());
        let row_h = ui.text_style_height(&TextStyle::Monospace);
        let row_stride = row_h + ui.spacing().item_spacing.y; // show_rows adds spacing between rows
        let sel_bg = ui.visuals().selection.bg_fill.linear_multiply(0.4);
        let shift = ui.input(|i| i.modifiers.shift);
        let n = self.idx.files[fi].lines.len();

        ui.horizontal(|ui| {
            ui.weak(format!("{}  —  click a line, shift-click to extend, then 'pin selection'; double-click or ctrl-click an identifier to jump to its definition", self.idx.files[fi].path));
            ui.label("line");
            let resp = ui.add(egui::TextEdit::singleline(&mut self.goto_line).desired_width(60.0));
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                if let Ok(line) = self.goto_line.trim().parse::<usize>() {
                    let line = line.clamp(1, n) - 1;
                    self.sel = Some((line, line));
                    self.scroll_to = Some(line.saturating_sub(8));
                }
            }
        });
        let mut area = egui::ScrollArea::both().id_salt("listing").auto_shrink(false);
        if let Some(line) = self.scroll_to.take() {
            area = area.vertical_scroll_offset(line as f32 * row_stride);
        }

        let mut clicked = None;
        let mut action = None;
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
                self.draw_code_line(ui, rect, fi, li, &font);
                let hovered = resp.hover_pos().and_then(|pos| self.code_hover(&resp, fi, li, self.code_col(rect, pos)));
                if hovered.is_some() {
                    action = hovered;
                } else if resp.clicked() {
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
        action
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
            Some(Action::Focus(r)) => self.go_to_symbol(r),
            Some(Action::OpenListing(r)) => {
                self.focus(r);
                self.tab = Tab::Listing;
            }
            Some(Action::SelectPath(pi)) => {
                self.select_path(pi);
                self.tab = Tab::Path;
            }
            Some(Action::ShowPath(pi)) => {
                if self.sel_path != Some(pi) {
                    self.select_path(pi);
                }
                self.tab = Tab::Graph;
            }
            Some(Action::SelectStep(pi, ai, in_document)) => self.select_step(pi, ai, in_document),
            Some(Action::ToggleStep(pi, ai)) => {
                if !self.expanded_steps.remove(&(pi, ai)) {
                    self.expanded_steps.insert((pi, ai));
                }
            }
            Some(Action::ToggleCode(pi, ai)) => {
                if !self.collapsed.remove(&(pi, ai)) {
                    self.collapsed.insert((pi, ai));
                }
            }
            Some(Action::ToggleFold(pi, ai)) => {
                if !self.folded.remove(&(pi, ai)) {
                    self.folded.insert((pi, ai));
                }
            }
            Some(Action::CollapseAll(pi, hide)) => {
                self.collapsed.retain(|&(p, _)| p != pi);
                if hide {
                    self.collapsed.extend((0..self.map.paths[pi].anchors.len()).map(|ai| (pi, ai)));
                }
            }
            Some(Action::EditNote(pi, ai)) => self.editing_note = Some((pi, ai)),
            // ponytail: no undo
            Some(Action::DeleteStep(pi, ai)) => {
                self.map.remove_anchor(pi, ai);
                self.sel_anchor = None;
                self.expanded_steps.clear();
                self.collapsed.clear();
                self.folded.clear();
                self.editing_note = None;
                self.dirty = true;
                self.refresh_graph();
            }
            Some(Action::DeletePath(pi)) => {
                self.map.paths.remove(pi);
                self.sel_path = None;
                self.sel_anchor = None;
                self.expanded_steps.clear();
                self.collapsed.clear();
                self.folded.clear();
                self.editing_note = None;
                self.dirty = true;
                self.graph.clear();
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
                if !self.graph.collapsed.remove(&r) {
                    self.graph.collapsed.insert(r);
                }
                self.graph.size.remove(&r); // re-measure
                self.graph.manual = false;
            }
            Some(Action::Peek(r)) => self.peek = Some(r),
            Some(Action::ClosePeek) => self.peek = None,
            Some(Action::GoTo(fi, line)) => self.open_line(fi, line),
            Some(Action::Jump(fi, line, col)) => {
                self.jump_to(fi, line, col);
                if self.tab != Tab::Graph {
                    self.tab = Tab::Listing;
                }
            }
            Some(Action::Relayout) => {
                self.graph.manual = false;
                self.graph.layout(&self.idx, self.metrics);
            }
            None => {}
        }
    }
}

/// `word` appears in `line` as a whole identifier.
fn has_word(line: &str, word: &str) -> bool {
    let is_id = |c: char| c.is_alphanumeric() || c == '_';
    line.match_indices(word).any(|(i, _)| !line[..i].chars().next_back().is_some_and(is_id) && !line[i + word.len()..].chars().next().is_some_and(is_id))
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_owned() } else { s.chars().take(n.saturating_sub(1)).chain(std::iter::once('…')).collect() }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
            self.save();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowLeft)) || ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1)) {
            self.back();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowRight)) || ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra2)) {
            self.forward();
        }
        let mono = TextStyle::Monospace.resolve(&ctx.style()); // resolve outside the fonts lock: ctx.style() inside it deadlocks
        self.metrics = ctx.fonts_mut(|f| Metrics { line_h: f.row_height(&mono), char_w: f.glyph_width(&mono, 'M') });
        ctx.request_repaint_after(if self.backend.is_some() { Duration::from_millis(100) } else { Duration::from_secs(1) }); // keep polling while idle
        self.poll_backend();
        self.poll_base();
        self.poll_disk();

        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                ui.label("search");
                if ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(220.0)).lost_focus() && enter {
                    self.run_search();
                }
                ui.separator();
                if ui.add_enabled(!self.history.is_empty(), egui::Button::new("<")).on_hover_text("back: alt+left or the mouse back button").clicked() {
                    self.back();
                }
                if ui.add_enabled(!self.forward.is_empty(), egui::Button::new(">")).on_hover_text("forward: alt+right or the mouse forward button").clicked() {
                    self.forward();
                }
                ui.selectable_value(&mut self.tab, Tab::Path, "Path");
                ui.selectable_value(&mut self.tab, Tab::Diff, "Diff");
                ui.selectable_value(&mut self.tab, Tab::Graph, "Graph");
                ui.selectable_value(&mut self.tab, Tab::Listing, "Listing");
                ui.selectable_value(&mut self.tab, Tab::Results, format!("Results ({})", self.results.len()));
                ui.separator();
                ui.label("new path");
                if ui.add(egui::TextEdit::singleline(&mut self.new_path).desired_width(140.0)).lost_focus() && enter {
                    self.create_path();
                }
                if ui.button("pin selection").on_hover_text("the listing's selected lines become a step of the selected path, under the selected step").clicked() {
                    self.add_selection();
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
                if !self.backend_progress.is_empty() {
                    ui.separator();
                    ui.weak(&self.backend_progress);
                }
            });
        });

        egui::TopBottomPanel::bottom("output").resizable(true).default_height(160.0).show(ctx, |ui| self.output_panel(ui));

        let mut action = None;
        egui::SidePanel::left("left").default_width(420.0).resizable(true).show(ctx, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.left, LeftTab::Paths, "Paths");
                ui.selectable_value(&mut self.left, LeftTab::Symbols, "Symbols");
                ui.selectable_value(&mut self.left, LeftTab::Files, "Files");
            });
            ui.separator();
            action = match self.left {
                LeftTab::Paths => self.paths_window(ui),
                LeftTab::Symbols => self.symbols_window(ui),
                LeftTab::Files => self.files_window(ui),
            };
        });
        egui::SidePanel::right("xrefs").default_width(300.0).resizable(true).show(ctx, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            if let Some(r) = self.peek.filter(|r| r.file < self.idx.files.len() && r.sym < self.idx.files[r.file].symbols.len()) {
                let s = self.idx.sym(r);
                ui.horizontal(|ui| {
                    ui.strong(format!("Peek: {}", s.name));
                    ui.weak(format!("{}:{}", self.idx.files[r.file].path, s.start + 1));
                    if ui.small_button("go").clicked() {
                        action = Some(Action::Focus(r));
                    }
                    if ui.small_button("x").clicked() {
                        action = Some(Action::ClosePeek);
                    }
                });
                egui::ScrollArea::both().id_salt("peek").max_height(ui.available_height() * 0.45).auto_shrink([false, true]).show(ui, |ui| self.code_block(ui, r, usize::MAX));
                ui.separator();
            }
            action = self.xrefs_window(ui).or(action.take());
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Path => action = self.path_document(ui).or(action.take()),
            Tab::Diff => action = self.diff_view(ui).or(action.take()),
            Tab::Graph => action = self.graph_view(ui).or(action.take()),
            Tab::Listing => action = self.listing(ui).or(action.take()),
            Tab::Results => self.results_view(ui),
        });
        self.apply(action);
        self.track_navigation();
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
    let mut idx = index::build(root);
    idx.run_backends(|m| eprintln!("{m}"));
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
