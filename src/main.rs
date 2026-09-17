mod cli;
mod index;
mod map;

use eframe::egui::{self, Align2, Color32, Key, Modifiers, Sense, Stroke, TextStyle, pos2, vec2};
use index::{Index, SymRef};
use map::{Author, Map};
use std::path::{Path, PathBuf};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Listing,
    Graph,
    Results,
}

struct App {
    idx: Index,
    map: Map,
    map_path: PathBuf,
    dirty: bool,

    sel_path: Option<usize>,
    sel_anchor: Option<usize>,

    tab: Tab,
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
        App {
            idx,
            map,
            map_path,
            dirty: false,
            sel_path: None,
            sel_anchor: None,
            tab: Tab::Listing,
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
        }
    }

    fn open(&mut self, file: usize, ls: usize, le: usize) {
        self.cur_file = Some(file);
        if self.tab == Tab::Results {
            self.tab = Tab::Listing;
        }
        self.sel = Some((ls, le));
        self.scroll_to = Some(ls.saturating_sub(8));
    }

    fn open_sym(&mut self, r: SymRef) {
        let s = self.idx.sym(r);
        let (start, end) = (s.start, s.end);
        self.open(r.file, start, end);
    }

    /// Innermost symbol containing the selection's anchor line: IDA's "current function".
    fn cur_sym(&self) -> Option<SymRef> {
        let (fi, (line, _)) = (self.cur_file?, self.sel?);
        self.idx.files[fi]
            .symbols
            .iter()
            .enumerate()
            .filter(|(_, s)| s.start <= line && line <= s.end)
            .max_by_key(|(_, s)| s.depth)
            .map(|(sym, _)| SymRef { file: fi, sym })
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

    fn add_anchor(&mut self) {
        let (Some(pi), Some(fi), Some((a, b))) = (self.sel_path, self.cur_file, self.sel) else {
            self.status = "select a path and lines in the listing first".into();
            return;
        };
        self.map.add_anchor(&self.idx, pi, fi, a.min(b), a.max(b), Author::Human);
        self.dirty = true;
    }

    // ponytail: no undo; deletes the selected anchor, else the selected path.
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
    Open(SymRef),
    SelectPath(usize),
    OpenAnchor(usize, usize),
    Promote(SymRef),
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
        let cur = self.cur_sym();
        let row_h = ui.text_style_height(&TextStyle::Monospace);
        egui::ScrollArea::vertical().id_salt("symbols").auto_shrink(false).show_rows(ui, row_h, rows.len(), |ui, range| {
            for r in &rows[range] {
                let s = self.idx.sym(*r);
                let text = format!("{}{:<26} {:<12} {}:{}", if s.depth > 0 { "  " } else { "" }, trunc(&s.name, 26), trunc(s.kind, 12), self.idx.files[r.file].path, s.start + 1);
                let resp = ui.selectable_label(cur == Some(*r), egui::RichText::new(text).monospace());
                if resp.clicked() {
                    action = Some(Action::Open(*r));
                }
                resp.on_hover_text(format!("{} callers, {} callees", s.callers.len(), s.callees.len()));
            }
        });
        action
    }

    fn paths_window(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        ui.strong(format!("Paths ({})", self.map.paths.len()));
        egui::ScrollArea::vertical().id_salt("paths").auto_shrink(false).show(ui, |ui| {
            for pi in 0..self.map.paths.len() {
                let selected = self.sel_path == Some(pi);
                let (name, tag, n) = {
                    let p = &self.map.paths[pi];
                    (p.name.clone(), p.author.tag(), p.anchors.len())
                };
                if ui.selectable_label(selected, format!("{name}{tag} ({n})")).clicked() {
                    action = Some(Action::SelectPath(pi));
                }
                if !selected {
                    continue;
                }
                ui.indent(pi, |ui| {
                    if ui.add(egui::TextEdit::multiline(&mut self.map.paths[pi].note).desired_rows(2).hint_text("note").desired_width(f32::INFINITY)).changed() {
                        self.dirty = true;
                    }
                    for ai in 0..self.map.paths[pi].anchors.len() {
                        let a = &self.map.paths[pi].anchors[ai];
                        let text = format!("{}{}:{}-{} {}{}", if a.stale { "! " } else { "" }, a.file, a.line_start + 1, a.line_end + 1, a.symbol, a.author.tag());
                        let color = if a.stale { Color32::LIGHT_RED } else { ui.visuals().text_color() };
                        if ui.selectable_label(self.sel_anchor == Some(ai), egui::RichText::new(text).color(color)).clicked() {
                            action = Some(Action::OpenAnchor(pi, ai));
                        }
                    }
                });
            }

            // ponytail: every root, one hop deep; the full tree lives in `tree` and the Graph tab.
            let roots = self.idx.roots();
            egui::CollapsingHeader::new(format!("Auto ({} roots)", roots.len())).show(ui, |ui| {
                for r in roots {
                    let s = self.idx.sym(r);
                    ui.horizontal(|ui| {
                        if ui.small_button("promote").on_hover_text("new path: this symbol + what it calls").clicked() {
                            action = Some(Action::Promote(r));
                        }
                        let header = egui::CollapsingHeader::new(format!("{} ({})", s.name, s.callees.len())).id_salt(r).show(ui, |ui| {
                            for &c in &s.callees {
                                if ui.selectable_label(false, &self.idx.sym(c).name).clicked() {
                                    action = Some(Action::Open(c));
                                }
                            }
                        });
                        if header.header_response.clicked() {
                            action = Some(Action::Open(r));
                        }
                    });
                }
            });
        });
        action
    }

    fn xrefs_window(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        let Some(cur) = self.cur_sym() else {
            ui.weak("no symbol under cursor");
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
                    action = Some(Action::Open(r));
                }
            }
            ui.separator();
            ui.label(format!("Xrefs from ({})", s.callees.len()));
            for &r in &s.callees {
                if ui.selectable_label(false, cli::describe(&self.idx, r)).clicked() {
                    action = Some(Action::Open(r));
                }
            }
        });
        action
    }

    fn listing(&mut self, ui: &mut egui::Ui) {
        let Some(fi) = self.cur_file else {
            ui.weak("click a symbol to open its file");
            return;
        };
        let font = TextStyle::Monospace.resolve(ui.style());
        let row_h = ui.text_style_height(&TextStyle::Monospace);
        let text_color = ui.visuals().text_color();
        let dim = ui.visuals().weak_text_color();
        let sel_bg = ui.visuals().selection.bg_fill.linear_multiply(0.4);
        let shift = ui.input(|i| i.modifiers.shift);
        let n = self.idx.files[fi].lines.len();

        let mut area = egui::ScrollArea::both().id_salt("listing").auto_shrink(false);
        if let Some(line) = self.scroll_to.take() {
            area = area.vertical_scroll_offset(line as f32 * row_h);
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

    /// One hop each way: callers in a column on the left, the symbol in the middle, callees on
    /// the right. Click a node to recentre. ponytail: no layout engine; add one for multi-hop.
    fn graph(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let Some(cur) = self.cur_sym() else {
            ui.weak("no symbol under cursor");
            return None;
        };
        let s = self.idx.sym(cur);
        let (callers, callees) = (s.callers.clone(), s.callees.clone());
        let rows = callers.len().max(callees.len()).max(1);
        let (node_h, gap) = (24.0, 8.0);
        let height = rows as f32 * (node_h + gap) + gap;
        let mut action = None;

        egui::ScrollArea::vertical().id_salt("graph").auto_shrink(false).show(ui, |ui| {
            let width = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(vec2(width, height.max(ui.available_height())), Sense::hover());
            let node_w = (width / 3.4).min(320.0);
            let cols = [rect.left() + 10.0, rect.center().x - node_w / 2.0, rect.right() - node_w - 10.0];
            let font = TextStyle::Monospace.resolve(ui.style());
            let stroke = Stroke::new(1.0, ui.visuals().weak_text_color());

            let mut node = |ui: &mut egui::Ui, r: SymRef, col: usize, row: usize, current: bool| -> egui::Rect {
                let y = rect.top() + gap + row as f32 * (node_h + gap);
                let nr = egui::Rect::from_min_size(pos2(cols[col], y), vec2(node_w, node_h));
                let resp = ui.interact(nr, ui.id().with((r, col)), Sense::click());
                let fill = if current { ui.visuals().selection.bg_fill } else if resp.hovered() { ui.visuals().widgets.hovered.bg_fill } else { ui.visuals().widgets.inactive.bg_fill };
                ui.painter().rect(nr, 3.0, fill, stroke, egui::StrokeKind::Inside);
                let name = &self.idx.sym(r).name;
                ui.painter().text(nr.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, trunc(name, (node_w / 8.0) as usize), font.clone(), ui.visuals().text_color());
                if resp.clicked() && !current {
                    action = Some(Action::Open(r));
                }
                nr
            };

            let center_row = rows / 2;
            let c = node(ui, cur, 1, center_row, true);
            for (i, &r) in callers.iter().enumerate() {
                let n = node(ui, r, 0, i, false);
                ui.painter().line_segment([n.right_center(), c.left_center()], stroke);
            }
            for (i, &r) in callees.iter().enumerate() {
                let n = node(ui, r, 2, i, false);
                ui.painter().line_segment([c.right_center(), n.left_center()], stroke);
            }
        });
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
            self.open(fi, li, li);
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
            Some(Action::Open(r)) => self.open_sym(r),
            Some(Action::SelectPath(pi)) => {
                self.sel_path = if self.sel_path == Some(pi) { None } else { Some(pi) };
                self.sel_anchor = None;
            }
            Some(Action::OpenAnchor(pi, ai)) => {
                self.sel_anchor = Some(ai);
                let a = &self.map.paths[pi].anchors[ai];
                let (ls, le, file) = (a.line_start, a.line_end, a.file.clone());
                if let Some(fi) = self.idx.find_file(&file) {
                    self.open(fi, ls, le);
                }
            }
            Some(Action::Promote(r)) => {
                self.sel_path = Some(self.map.promote(&self.idx, r, 1, Author::Human));
                self.sel_anchor = None;
                self.dirty = true;
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

        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                ui.label("search");
                if ui.add(egui::TextEdit::singleline(&mut self.search).desired_width(220.0)).lost_focus() && enter {
                    self.run_search();
                }
                ui.separator();
                ui.selectable_value(&mut self.tab, Tab::Listing, "Listing");
                ui.selectable_value(&mut self.tab, Tab::Graph, "Graph");
                ui.selectable_value(&mut self.tab, Tab::Results, format!("Results ({})", self.results.len()));
                ui.separator();
                ui.label("new path");
                if ui.add(egui::TextEdit::singleline(&mut self.new_path).desired_width(140.0)).lost_focus() && enter {
                    self.create_path();
                }
                if ui.button("add selection to path").clicked() {
                    self.add_anchor();
                }
                if ui.button("delete selected").clicked() {
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
                ui.separator();
                ui.weak("click a line, shift-click to extend, ctrl+s saves");
            });
        });

        egui::TopBottomPanel::bottom("output").resizable(true).default_height(180.0).show(ctx, |ui| self.output_panel(ui));

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
            Tab::Listing => self.listing(ui),
            Tab::Graph => action = self.graph(ui).or(action.take()),
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
        viewport: egui::ViewportBuilder::default().with_inner_size([1600.0, 1000.0]).with_title(&title),
        ..Default::default()
    };
    eframe::run_native("codemap", options, Box::new(move |_cc| Ok(Box::new(App::new(&root)))))
}
