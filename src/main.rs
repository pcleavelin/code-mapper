mod cli;
mod index;
mod map;

use eframe::egui::{self, Align2, Color32, Key, Modifiers, Sense, Stroke, TextStyle, pos2, vec2};
use index::{Index, SymRef};
use map::{Author, Map};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Graph,
    Listing,
    Results,
}

// ---- graph: the main view ---------------------------------------------------------

const NODE_W: f32 = 520.0;
const COL_W: f32 = NODE_W + 90.0;
const ROW_GAP: f32 = 24.0;
const PREVIEW_LINES: usize = 12;
const MAX_NODE_LINES: usize = 200;

/// Nodes are symbols, each showing its code. Layout is column-based: callers to the left,
/// callees to the right, a path as a left-to-right chain. Positions persist across frames and
/// can be dragged. ponytail: no layout engine, no edge routing; drag nodes if it gets tangled.
struct Graph {
    nodes: Vec<SymRef>,
    pos: HashMap<SymRef, egui::Pos2>,
    next_y: HashMap<i32, f32>,
    focus: Option<SymRef>,
    chain: Vec<SymRef>, // when showing a path: step order
    chain_path: Option<usize>,
    expanded: std::collections::HashSet<SymRef>, // nodes showing all their lines
    size: HashMap<SymRef, egui::Vec2>,           // measured last frame; estimate until then
    scene_rect: egui::Rect,
}

impl Default for Graph {
    fn default() -> Self {
        Graph {
            nodes: Vec::new(),
            pos: HashMap::new(),
            next_y: HashMap::new(),
            focus: None,
            chain: Vec::new(),
            chain_path: None,
            expanded: Default::default(),
            size: HashMap::new(),
            scene_rect: egui::Rect::ZERO,
        }
    }
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

    fn node_height(&self, idx: &Index, r: SymRef, line_h: f32) -> f32 {
        if let Some(sz) = self.size.get(&r) {
            return sz.y;
        }
        let (shown, total) = self.lines_shown(idx, r);
        34.0 + shown as f32 * line_h + if shown < total { line_h } else { 0.0 } + 12.0
    }

    fn node_rect(&self, idx: &Index, r: SymRef, line_h: f32) -> egui::Rect {
        egui::Rect::from_min_size(self.pos[&r], vec2(NODE_W, self.node_height(idx, r, line_h)))
    }

    /// Place `r` in column `col` below whatever is already there. No-op if present.
    fn add(&mut self, idx: &Index, r: SymRef, col: i32, line_h: f32) {
        if self.pos.contains_key(&r) {
            return;
        }
        let y = *self.next_y.get(&col).unwrap_or(&0.0);
        self.pos.insert(r, pos2(col as f32 * COL_W, y));
        self.next_y.insert(col, y + self.node_height(idx, r, line_h) + ROW_GAP);
        self.nodes.push(r);
    }

    fn col_of(&self, r: SymRef) -> i32 {
        (self.pos[&r].x / COL_W).round() as i32
    }

    /// Focus symbol in the middle, its callers one column left, its callees one column right.
    fn build_around(&mut self, idx: &Index, r: SymRef, line_h: f32) {
        self.clear();
        self.add(idx, r, 0, line_h);
        self.focus = Some(r);
        self.expand_callers(idx, r, line_h);
        self.expand_callees(idx, r, line_h);
    }

    fn expand_callees(&mut self, idx: &Index, r: SymRef, line_h: f32) {
        let col = self.col_of(r) + 1;
        for &c in &idx.sym(r).callees {
            self.add(idx, c, col, line_h);
        }
    }

    fn expand_callers(&mut self, idx: &Index, r: SymRef, line_h: f32) {
        let col = self.col_of(r) - 1;
        for &c in &idx.sym(r).callers {
            self.add(idx, c, col, line_h);
        }
    }

    /// A path as a chain: step 1 leftmost, each step one column to the right.
    fn build_chain(&mut self, idx: &Index, map: &Map, pi: usize, line_h: f32) {
        self.clear();
        self.chain_path = Some(pi);
        for a in &map.paths[pi].anchors {
            let Some(fi) = idx.find_file(&a.file) else { continue };
            let Some(si) = idx.files[fi].symbols.iter().position(|s| s.name == a.symbol) else { continue };
            let r = SymRef { file: fi, sym: si };
            let col = self.chain.len() as i32;
            self.add(idx, r, col, line_h);
            self.chain.push(r);
        }
        self.focus = self.chain.first().copied();
    }

    fn fit(&mut self) {
        self.scene_rect = egui::Rect::ZERO; // Scene resets an invalid rect to fit the contents
    }

    /// Readable zoom centred on the focused node (fit-all is unreadable past a few nodes).
    fn look_at_focus(&mut self, idx: &Index, line_h: f32) {
        if let Some(r) = self.focus {
            let c = self.node_rect(idx, r, line_h).center();
            self.scene_rect = egui::Rect::from_center_size(c, vec2(COL_W * 2.6, 1000.0));
        } else {
            self.fit();
        }
    }
}

struct App {
    idx: Index,
    map: Map,
    map_path: PathBuf,
    dirty: bool,

    sel_path: Option<usize>,
    sel_anchor: Option<usize>,

    tab: Tab,
    graph: Graph,
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

// ---- actions ----------------------------------------------------------------------

impl App {
    fn new(root: &Path) -> App {
        let idx = index::build(root);
        let map_path = root.join(".codemap");
        let mut map = Map::load(&map_path).unwrap_or_default();
        map.resolve_all(&idx);
        let status = format!("{} files, {} symbols indexed", idx.files.len(), idx.files.iter().map(|f| f.symbols.len()).sum::<usize>());
        let mut app = App {
            idx,
            map,
            map_path,
            dirty: false,
            sel_path: None,
            sel_anchor: None,
            tab: Tab::Graph,
            graph: Graph::default(),
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
            app.focus(r, 14.0);
        }
        app
    }

    /// Make `r` the current symbol: listing position, xrefs, and graph focus (rebuilding the
    /// graph around it if it is not already on screen).
    fn focus(&mut self, r: SymRef, line_h: f32) {
        let s = self.idx.sym(r);
        let (start, end) = (s.start, s.end);
        self.cur_file = Some(r.file);
        self.sel = Some((start, end));
        self.scroll_to = Some(start.saturating_sub(3));
        if !self.graph.pos.contains_key(&r) {
            self.graph.build_around(&self.idx, r, line_h);
        }
        self.graph.focus = Some(r);
        self.graph.look_at_focus(&self.idx, line_h);
        if self.tab == Tab::Results {
            self.tab = Tab::Graph;
        }
    }

    fn open_line(&mut self, file: usize, line: usize) {
        self.cur_file = Some(file);
        self.sel = Some((line, line));
        self.scroll_to = Some(line.saturating_sub(8));
        self.tab = Tab::Listing;
    }

    fn show_path(&mut self, pi: usize, line_h: f32) {
        self.graph.build_chain(&self.idx, &self.map, pi, line_h);
        self.graph.look_at_focus(&self.idx, line_h);
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
        self.sel_path = Some(self.map.add_path(&name, Author::Human));
        self.sel_anchor = None;
        self.new_path.clear();
        self.dirty = true;
    }

    /// Add the listing selection (or, in the graph, the focused symbol) to the selected path.
    fn add_anchor(&mut self) {
        let Some(pi) = self.sel_path else {
            self.status = "select a path first (Paths window)".into();
            return;
        };
        let (fi, ls, le) = match (self.tab, self.cur_file, self.sel, self.graph.focus) {
            (Tab::Listing, Some(fi), Some((a, b)), _) => (fi, a.min(b), a.max(b)),
            (_, _, _, Some(r)) => {
                let s = self.idx.sym(r);
                (r.file, s.start, s.end)
            }
            _ => {
                self.status = "select lines in the listing or a node in the graph first".into();
                return;
            }
        };
        self.map.add_anchor(&self.idx, pi, fi, ls, le, Author::Human);
        self.dirty = true;
        self.status = format!("step {} added to '{}'", self.map.paths[pi].anchors.len(), self.map.paths[pi].name);
    }

    // ponytail: no undo; deletes the selected step, else the selected path.
    fn delete_selected(&mut self) {
        let Some(pi) = self.sel_path else { return };
        match self.sel_anchor {
            Some(ai) if ai < self.map.paths[pi].anchors.len() => {
                self.map.paths[pi].anchors.remove(ai);
                self.sel_anchor = None;
            }
            _ => {
                self.map.paths.remove(pi);
                self.sel_path = None;
                if self.graph.chain_path == Some(pi) {
                    self.graph.chain_path = None;
                }
            }
        }
        self.dirty = true;
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

    fn run_cmd(&mut self) {
        let line = std::mem::take(&mut self.cmd);
        let args = cli::tokenize(&line);
        self.output.push_str(&format!("> {line}\n"));
        match cli::exec(&self.idx, &mut self.map, &args, Author::Human, &mut self.output) {
            Ok(true) => {
                self.dirty = true;
                self.output.push_str("(map changed, ctrl+s to save)\n");
            }
            Ok(false) => {}
            Err(e) => self.output.push_str(&format!("error: {e}\n")),
        }
    }
}

// ---- windows ----------------------------------------------------------------------

enum Action {
    Focus(SymRef),
    OpenListing(SymRef),
    SelectPath(usize),
    ShowPath(usize),
    OpenAnchor(usize, usize),
    MoveAnchor(usize, usize, isize),
    Promote(SymRef),
    ExpandCallers(SymRef),
    ExpandCallees(SymRef),
    ToggleExpand(SymRef),
    AddToPath(SymRef),
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
            ui.weak("a path = an ordered chain of code steps");
        });
        egui::ScrollArea::vertical().id_salt("paths").auto_shrink(false).show(ui, |ui| {
            for pi in 0..self.map.paths.len() {
                let selected = self.sel_path == Some(pi);
                let (name, tag, n) = {
                    let p = &self.map.paths[pi];
                    (p.name.clone(), p.author.tag(), p.anchors.len())
                };
                ui.horizontal(|ui| {
                    if ui.selectable_label(selected, format!("{name}{tag}  ({n} steps)")).clicked() {
                        action = Some(Action::SelectPath(pi));
                    }
                    if ui.small_button("graph").on_hover_text("show this path as a chain in the graph").clicked() {
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
                    let n = self.map.paths[pi].anchors.len();
                    for ai in 0..n {
                        let a = &self.map.paths[pi].anchors[ai];
                        let name = if a.symbol.is_empty() { "(lines)" } else { a.symbol.as_str() };
                        let text = format!("{}. {}{}  {}:{}-{}{}", ai + 1, if a.stale { "! " } else { "" }, name, a.file, a.line_start + 1, a.line_end + 1, a.author.tag());
                        let color = if a.stale { Color32::LIGHT_RED } else { ui.visuals().text_color() };
                        ui.horizontal(|ui| {
                            if ui.small_button("▲").clicked() && ai > 0 {
                                action = Some(Action::MoveAnchor(pi, ai, -1));
                            }
                            if ui.small_button("▼").clicked() && ai + 1 < n {
                                action = Some(Action::MoveAnchor(pi, ai, 1));
                            }
                            let resp = ui.selectable_label(self.sel_anchor == Some(ai), egui::RichText::new(text).color(color));
                            if resp.clicked() {
                                action = Some(Action::OpenAnchor(pi, ai));
                            }
                            resp.on_hover_text(if a.stale { "stale: the code changed since this step was pinned. Delete and re-add it." } else { "click to view; ▲▼ reorder" });
                        });
                    }
                });
            }

            // ponytail: every root, one hop deep; the full tree lives in `tree` and the graph.
            let roots = self.idx.roots();
            egui::CollapsingHeader::new(format!("Entry points ({})", roots.len())).show(ui, |ui| {
                ui.weak("symbols nothing calls. promote = new path from one + what it calls");
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
            if let Some(pi) = self.graph.chain_path {
                ui.separator();
                ui.label(format!("showing path '{}' as a chain", self.map.paths[pi].name));
            }
        });

        let font = TextStyle::Monospace.resolve(ui.style());
        let line_h = ui.ctx().fonts_mut(|f| f.row_height(&font));
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

        let mut scene_rect = self.graph.scene_rect;
        egui::Scene::new().zoom_range(0.1..=1.5).show(ui, &mut scene_rect, |ui| {
            let painter = ui.painter().clone();
            let edge = Stroke::new(1.5, ui.visuals().weak_text_color());
            let chain_stroke = Stroke::new(3.0, Color32::from_rgb(90, 200, 120));

            // sizes from line counts so edges land on node borders
            let rect_of = |g: &Graph, r: SymRef| g.node_rect(&self.idx, r, line_h);

            // call edges: right side of caller -> left side of callee
            for &a in &self.graph.nodes {
                let ra = rect_of(&self.graph, a);
                for &b in &self.idx.sym(a).callees {
                    if !self.graph.pos.contains_key(&b) || a == b {
                        continue;
                    }
                    let rb = rect_of(&self.graph, b);
                    // leave and arrive at header height, with a wide horizontal departure so the
                    // curve is visible beside the node instead of diving behind it
                    let (p0, p1) = (ra.right_top() + vec2(0.0, 14.0), rb.left_top() + vec2(0.0, 14.0));
                    let dx = ((p1.x - p0.x).abs() * 0.5).max(160.0);
                    painter.add(egui::epaint::CubicBezierShape::from_points_stroke([p0, p0 + vec2(dx, 0.0), p1 - vec2(dx, 0.0), p1], false, Color32::TRANSPARENT, edge));
                    painter.circle_filled(p1, 3.0, edge.color);
                }
            }
            // chain edges: step i -> step i+1, numbered
            for (i, w) in self.graph.chain.windows(2).enumerate() {
                let (p0, p1) = (rect_of(&self.graph, w[0]).right_top() + vec2(0.0, 14.0), rect_of(&self.graph, w[1]).left_top() + vec2(0.0, 14.0));
                let dx = ((p1.x - p0.x).abs() * 0.5).max(160.0);
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke([p0, p0 + vec2(dx, 0.0), p1 - vec2(dx, 0.0), p1], false, Color32::TRANSPARENT, chain_stroke));
                painter.text((p0 + p1.to_vec2()) / 2.0 - vec2(0.0, 12.0), Align2::CENTER_CENTER, format!("{}", i + 2), font.clone(), chain_stroke.color);
            }

            for r in self.graph.nodes.clone() {
                let s = self.idx.sym(r);
                let rect = rect_of(&self.graph, r);
                let focused = self.graph.focus == Some(r);
                let on_path = in_path.contains(&r);
                let border = if focused {
                    Stroke::new(2.0, ui.visuals().selection.stroke.color)
                } else if on_path {
                    Stroke::new(2.0, chain_stroke.color)
                } else {
                    Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
                };

                let inner = ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                    egui::Frame::new().fill(ui.visuals().extreme_bg_color).stroke(border).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
                        ui.set_width(NODE_W - 12.0);
                        // header = drag handle
                        let header = ui.horizontal(|ui| {
                            ui.strong(&s.name);
                            ui.weak(format!("{}:{}", self.idx.files[r.file].path, s.start + 1));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let expanded = self.graph.expanded.contains(&r);
                                if ui.small_button(if expanded { "▴" } else { "▾" }).on_hover_text("show all / fewer lines").clicked() {
                                    action = Some(Action::ToggleExpand(r));
                                }
                                if ui.small_button("+path").on_hover_text("add this symbol as a step of the selected path").clicked() {
                                    action = Some(Action::AddToPath(r));
                                }
                                if ui.small_button("listing").on_hover_text("open in the listing").clicked() {
                                    action = Some(Action::OpenListing(r));
                                }
                                if ui.small_button(format!("callees ▶ {}", s.callees.len())).clicked() {
                                    action = Some(Action::ExpandCallees(r));
                                }
                                if ui.small_button(format!("{} ◀ callers", s.callers.len())).clicked() {
                                    action = Some(Action::ExpandCallers(r));
                                }
                            });
                        });
                        let drag = ui.interact(header.response.rect, ui.id().with(("drag", r)), Sense::click_and_drag());
                        if drag.dragged() {
                            *self.graph.pos.get_mut(&r).unwrap() += drag.drag_delta();
                        }
                        if drag.clicked() {
                            action = Some(Action::Focus(r));
                        }
                        ui.separator();
                        let f = &self.idx.files[r.file];
                        let (shown, total) = self.graph.lines_shown(&self.idx, r);
                        let mut code = String::new();
                        for li in s.start..s.start + shown {
                            code.push_str(&format!("{:4} {}\n", li + 1, f.lines[li]));
                        }
                        if shown < total {
                            code.push_str(&format!("     … {} more lines (▾ to show, or listing)", total - shown));
                        }
                        ui.set_clip_rect(egui::Rect::from_min_size(rect.min, vec2(NODE_W, f32::INFINITY)).intersect(ui.clip_rect()));
                        ui.add(egui::Label::new(egui::RichText::new(code.trim_end_matches('\n')).monospace()).wrap_mode(egui::TextWrapMode::Extend));
                    });
                });
                self.graph.size.insert(r, inner.response.rect.size());
            }
        });
        self.graph.scene_rect = scene_rect;
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
                p.text(rect.left_top() + vec2(64.0, 0.0), Align2::LEFT_TOP, &f.lines[li], font.clone(), text_color);
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

    fn apply(&mut self, action: Option<Action>, line_h: f32) {
        match action {
            Some(Action::Focus(r)) => self.focus(r, line_h),
            Some(Action::OpenListing(r)) => {
                self.focus(r, line_h);
                self.tab = Tab::Listing;
            }
            Some(Action::SelectPath(pi)) => {
                self.sel_path = if self.sel_path == Some(pi) { None } else { Some(pi) };
                self.sel_anchor = None;
            }
            Some(Action::ShowPath(pi)) => {
                self.sel_path = Some(pi);
                self.sel_anchor = None;
                self.show_path(pi, line_h);
                self.tab = Tab::Graph;
            }
            Some(Action::OpenAnchor(pi, ai)) => {
                self.sel_anchor = Some(ai);
                let a = &self.map.paths[pi].anchors[ai];
                let (ls, le, file, sym) = (a.line_start, a.line_end, a.file.clone(), a.symbol.clone());
                if let Some(fi) = self.idx.find_file(&file) {
                    if let Some(si) = self.idx.files[fi].symbols.iter().position(|s| s.name == sym) {
                        if self.graph.chain_path != Some(pi) {
                            self.show_path(pi, line_h);
                        }
                        self.focus(SymRef { file: fi, sym: si }, line_h);
                    }
                    self.cur_file = Some(fi);
                    self.sel = Some((ls, le));
                    self.scroll_to = Some(ls.saturating_sub(3));
                }
            }
            Some(Action::MoveAnchor(pi, ai, delta)) => {
                let bi = (ai as isize + delta) as usize;
                self.map.paths[pi].anchors.swap(ai, bi);
                self.sel_anchor = Some(bi);
                self.dirty = true;
                if self.graph.chain_path == Some(pi) {
                    self.show_path(pi, line_h);
                }
            }
            Some(Action::Promote(r)) => {
                let pi = self.map.promote(&self.idx, r, 1, Author::Human);
                self.sel_path = Some(pi);
                self.sel_anchor = None;
                self.dirty = true;
                self.show_path(pi, line_h);
                self.tab = Tab::Graph;
            }
            Some(Action::ExpandCallers(r)) => {
                self.graph.expand_callers(&self.idx, r, line_h);
            }
            Some(Action::ExpandCallees(r)) => {
                self.graph.expand_callees(&self.idx, r, line_h);
            }
            Some(Action::ToggleExpand(r)) => {
                if !self.graph.expanded.remove(&r) {
                    self.graph.expanded.insert(r);
                }
            }
            Some(Action::AddToPath(r)) => {
                self.graph.focus = Some(r);
                self.tab = Tab::Graph;
                self.add_anchor();
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
        let line_h = ctx.fonts_mut(|f| f.row_height(&mono));

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
                if ui.button("add selection to path").on_hover_text("listing: selected lines. graph: focused node").clicked() {
                    self.add_anchor();
                }
                if ui.button("delete selected").on_hover_text("the selected step, else the selected path").clicked() {
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
        self.apply(action, line_h);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.dirty {
            self.save();
        }
    }
}

// ---- entry -------------------------------------------------------------------------

fn cli_main(root: &Path, args: &[String]) -> i32 {
    if args[0] == "help" {
        print!("{}", cli::HELP);
        return 0;
    }
    let idx = index::build(root);
    let map_path = root.join(".codemap");
    let mut map = Map::load(&map_path).unwrap_or_default();
    map.resolve_all(&idx);
    let mut out = String::new();
    match cli::exec(&idx, &mut map, args, Author::Ai, &mut out) {
        Ok(dirty) => {
            print!("{out}");
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
            eprintln!("{e}");
            2
        }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "help" || a == "--help" || a == "-h") {
        print!("{}", cli::HELP);
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
