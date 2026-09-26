//! The bars and panels around the centre: top bar, status bar, and the docked panels' contents
//! (the output panel, the Paths, Symbols and Files tabs, the xrefs panel); `dock` places them.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum LeftTab {
    Paths,
    Symbols,
    Files,
}

impl App {
    pub(super) fn top_bar(&mut self) {
        self.ui.open(
            Kind::None,
            Layout::row().grow_x().pad(4).gap(6).cross(Align::Center),
            Style::bg(PANEL).border(BORDER_BOTTOM, BORDER),
            None,
        );
        self.label("search", WEAK);
        self.field(Which::Search, "regex", 24 * self.cell.0);
        let (has_back, has_forward) = (
            !self.history.back.is_empty(),
            !self.history.forward.is_empty(),
        );
        if self.nav_button("<", ui::id("back"), has_back).clicked {
            self.actions.push(Action::Back);
        }
        if self.nav_button(">", ui::id("forward"), has_forward).clicked {
            self.actions.push(Action::Forward);
        }
        let n = self.results.len();
        let results = format!("Results ({n})");
        for (t, name) in [
            (Tab::Path, "Path"),
            (Tab::Diff, "Diff"),
            (Tab::Graph, "Graph"),
            (Tab::Listing, "Listing"),
            (Tab::Results, results.as_str()),
        ] {
            if self
                .button(
                    name,
                    ui::id_with(ui::id("tab"), name.split(' ').next().unwrap_or(name)),
                    self.tab == t,
                )
                .clicked
            {
                self.actions.push(Action::Tab(t));
            }
        }
        self.ui
            .leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
        self.label("new path", WEAK);
        self.field(Which::NewPath, "name", 16 * self.cell.0);
        if self.button("pin selection", ui::id("pin"), false).clicked {
            self.actions.push(Action::PinSelection);
        }
        let save = if self.file.dirty { "save *" } else { "save" };
        if self.button(save, ui::id("save"), false).clicked {
            self.actions.push(Action::Save);
        }
        self.ui.close();
    }

    pub(super) fn status_bar(&mut self) {
        self.ui.open(
            Kind::None,
            Layout::row().grow_x().pad(4).gap(12).cross(Align::Center),
            Style::bg(PANEL).border(BORDER_TOP, BORDER),
            None,
        );
        let p = self.file.path.display().to_string().replace('\\', "/");
        self.label(&p, WEAK);
        let s: String = self
            .status
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        self.label(&s, TEXT);
        if !self.work.progress.is_empty() {
            let b = self.work.progress.clone();
            self.label(&b, WEAK);
        }
        self.ui.close();
    }

    pub(super) fn output_panel(&mut self) {
        let px = self.px;
        let id = ui::id("output");
        self.ui
            .open(Kind::None, Layout::col().grow(), Style::bg(FIELD), None);
        self.grip(Panel::Output);
        self.label("Output", WEAK);
        self.ui.close();
        // a command's output exists only after this frame's layout, so the pin to the end is
        // asked for two frames running: the second one lands on the new content
        if self.output_bottom > 0 {
            self.output_bottom -= 1;
            self.scrolls.insert(id, i32::MAX);
        }
        let it = self.scroll_open(id, Layout::col().grow().pad(4), Style::default());
        let row_h = self.cell.1;
        let n = self.output.lines().count();
        let (first, visible) = self.rows_window(id, it.rect, n, row_h, 20);
        for line in self.output.lines().skip(first).take(visible) {
            self.ui
                .leaf(text(line, px, TEXT), Layout::row(), Style::default(), None);
        }
        self.spacer(n.saturating_sub(first + visible) as i32 * row_h);
        self.ui.close();
        let focus = self.fields[Which::Cmd as usize].focused;
        let it = self.ui.open(
            Kind::None,
            Layout::row().grow_x().pad(4).gap(4).cross(Align::Center),
            Style::bg(if focus { PANEL } else { FIELD }).border(BORDER_TOP, BORDER),
            Some(ui::id("cmd")),
        );
        if it.clicked {
            self.take_focus(Which::Cmd);
        }
        self.ui
            .leaf(text(">", px, WEAK), Layout::row(), Style::default(), None);
        let r = self.fields[Which::Cmd as usize].runs("");
        self.ui
            .leaf(runs(r, px), Layout::row().grow_x(), Style::default(), None);
        self.ui.close();
        self.ui.close();
    }

    pub(super) fn left_panel(&mut self) {
        self.ui
            .open(Kind::None, Layout::col().grow(), Style::bg(PANEL), None);
        self.grip(Panel::Nav);
        for (t, name) in [
            (LeftTab::Paths, "Paths"),
            (LeftTab::Symbols, "Symbols"),
            (LeftTab::Files, "Files"),
        ] {
            if self
                .button(name, ui::id_with(ui::id("left"), name), self.left == t)
                .clicked
            {
                self.left = t;
            }
        }
        self.ui.close();
        match self.left {
            LeftTab::Paths => self.paths_window(),
            LeftTab::Symbols => self.symbols_window(),
            LeftTab::Files => self.files_window(),
        }
        self.ui.close();
    }

    pub(super) fn paths_window(&mut self) {
        let diffs = self.diffs();
        let base = ui::id("paths");
        let it = self.scroll_open(base, Layout::col().grow().pad(4), Style::default());
        let cols = it.rect.map_or(44, |r| {
            ((r.w - 8 - ui::SCROLLBAR_W) / self.cell.0.max(1)).max(20)
        }) as usize;
        // a group opens or closes on a click; one never clicked is open while it holds the
        // path being read
        let reading = self
            .sel_path
            .and_then(|pi| self.map.paths.get(pi))
            .map(|p| p.group.clone())
            .unwrap_or_default();
        let mut closed_at: Option<usize> = None;
        for row in self.map.rows() {
            let depth = match &row {
                Row::Group { depth, .. } | Row::Path { depth, .. } => *depth,
            };
            if closed_at.is_some_and(|d| depth > d) {
                continue;
            }
            closed_at = None;
            let pad = "  ".repeat(depth);
            let pi = match row {
                Row::Group { group, paths, .. } => {
                    let holds = reading == group || reading.starts_with(&format!("{group}/"));
                    let open = self.groups_open.get(&group).copied().unwrap_or(holds);
                    let label = format!(
                        "{pad}{} {}/  {paths} paths",
                        if open { "▾" } else { "▸" },
                        group.rsplit('/').next().unwrap_or(&group)
                    );
                    let stale = self.map.paths.iter().any(|p| {
                        (p.group == group || p.group.starts_with(&format!("{group}/")))
                            && p.anchors.iter().any(|a| a.stale)
                    });
                    if self
                        .row(
                            vec![(label, if stale { RED } else { TEXT })],
                            ui::id_with(ui::id("group"), &group),
                            false,
                        )
                        .clicked
                    {
                        self.actions.push(Action::OpenGroup(group.clone(), !open));
                    }
                    if !open {
                        closed_at = Some(depth);
                    }
                    continue;
                }
                Row::Path { pi, .. } => pi,
            };
            let (name, kind, tag, n, stale) = {
                let p = &self.map.paths[pi];
                (
                    p.name.clone(),
                    p.kind.name(),
                    p.author.tag(),
                    p.anchors.len(),
                    p.anchors.iter().filter(|a| a.stale).count(),
                )
            };
            let mark = match diffs.iter().find(|d| d.name == name).map(|d| d.change) {
                Some(Change::Added) => "+ ",
                Some(Change::Changed) => "~ ",
                _ => "",
            };
            let color = if stale > 0 {
                RED
            } else if !mark.is_empty() {
                GREEN
            } else {
                TEXT
            };
            let selected = self.sel_path == Some(pi);
            let rest = format!(" [{kind}]{tag}  {n} steps");
            let it = self.row(
                vec![(
                    format!(
                        "{pad}{mark}{}{rest}",
                        trunc(
                            &name,
                            cols.saturating_sub(pad.len() + mark.len() + rest.chars().count())
                        )
                    ),
                    color,
                )],
                ui::id_n(base, pi),
                selected,
            );
            if it.clicked {
                self.actions.push(Action::OpenPath(pi, Tab::Path));
            }
            if !selected {
                continue;
            }
            // the outline: every step of the path being read, folded subtrees hidden
            let numbered = self.map.numbered(&self.idx, pi);
            let mut hide_below: Option<usize> = None;
            for (ai, depth, number) in numbered {
                if hide_below.is_some_and(|d| depth > d) {
                    continue;
                }
                hide_below = None;
                let a = &self.map.paths[pi].anchors[ai];
                let hidden = if self.step_view(pi, ai).folded {
                    hide_below = Some(depth);
                    self.map.descendants(pi, ai)
                } else {
                    0
                };
                let name = if a.symbol.is_empty() {
                    "(lines)"
                } else {
                    a.symbol.as_str()
                };
                let file = a.file.rsplit('/').next().unwrap_or("").to_owned();
                let stale = a.stale;
                let link = if a.link.is_empty() {
                    String::new()
                } else {
                    format!("  → {}", a.link)
                };
                let line = format!(
                    "{pad}  {}{number}  {name}{link}{}",
                    "  ".repeat(depth),
                    if hidden > 0 {
                        format!("  +{hidden}")
                    } else {
                        String::new()
                    }
                );
                let at_top = self.top_step == Some(ai);
                let marked = if self.sel_anchor == Some(ai) {
                    Some(SELECTED)
                } else if at_top {
                    Some(dim(SELECTED, 110))
                } else {
                    None
                };
                let it = self.row_bg(
                    vec![
                        (
                            line,
                            if stale {
                                RED
                            } else if at_top {
                                TEXT
                            } else {
                                WEAK
                            },
                        ),
                        (format!("  {file}"), dim(WEAK, 140)),
                    ],
                    ui::id_n(ui::id("outline"), ai),
                    marked,
                );
                if it.clicked {
                    self.actions.push(Action::SelectStep(pi, ai, false));
                }
            }
            // the outline follows the document: when the topmost step changes and its row is
            // out of view, scroll it to the upper third, never under a scrollbar drag
            if let (Some(ai), Some((_, view))) = (self.top_step, self.ui.content_of(base))
                && self.outline_shown != Some(ai)
                && !self.ui.dragging()
            {
                if let Some(row) = self.ui.interaction_of(ui::id_n(ui::id("outline"), ai)).rect
                    && (row.y < view.y || row.bottom() > view.bottom())
                {
                    let off = self.scrolls.get(&base).copied().unwrap_or(0) + (row.y - view.y)
                        - view.h / 3;
                    self.scrolls.insert(base, off.max(0));
                }
                self.outline_shown = Some(ai);
            }
        }
        for d in diffs.iter().filter(|d| d.change == Change::Removed) {
            let s = format!("- {}  (removed, {} steps)", d.name, d.removed.len());
            self.label(&s, WEAK);
        }
        self.ui.close();
    }

    pub(super) fn symbols_window(&mut self) {
        self.ui.open(
            Kind::None,
            Layout::row().grow_x().pad(4).gap(4).cross(Align::Center),
            Style::default(),
            None,
        );
        self.label("+ = in a path", WEAK);
        self.field(Which::SymFilter, "filter", 16 * self.cell.0);
        self.ui.close();
        let filter = self.fields[Which::SymFilter as usize].text.to_lowercase();
        let rows: Vec<SymRef> = self
            .idx
            .files
            .iter()
            .enumerate()
            .flat_map(|(file, f)| (0..f.symbols.len()).map(move |sym| SymRef { file, sym }))
            .filter(|r| {
                filter.is_empty()
                    || self.idx.sym(*r).name.to_lowercase().contains(&filter)
                    || self.idx.files[r.file].path.to_lowercase().contains(&filter)
            })
            .collect();
        let id = ui::id("symbols");
        let it = self.scroll_open(id, Layout::col().grow().pad(4), Style::default());
        let row_h = self.cell.1 + 4;
        let cols = it.rect.map_or(60, |r| {
            ((r.w - 8 - ui::SCROLLBAR_W) / self.cell.0.max(1)).max(30)
        }) as usize;
        let (first, visible) = self.rows_window(id, it.rect, rows.len(), row_h, 40);
        for &r in rows.iter().skip(first).take(visible) {
            let s = self.idx.sym(r);
            let covered = if self
                .map
                .covers(&self.idx.files[r.file].path, s.start, s.end)
            {
                "+"
            } else {
                " "
            };
            let indent = if s.depth > 0 { "  " } else { "" };
            let place = format!("{}:{}", self.idx.files[r.file].path, s.start + 1);
            let line = format!(
                "{covered} {indent}{:<20} {:<8} {}",
                trunc(&s.name, 20),
                trunc(&s.kind, 8),
                trunc_left(&place, cols.saturating_sub(32 + indent.len()))
            );
            let pending = self.idx.files[r.file].pending;
            let it = self.row(
                vec![(line, if pending { dim(WEAK, 120) } else { TEXT })],
                ui::id_n(ui::id("sym"), r.file * 100_000 + r.sym),
                self.focus == Some(r),
            );
            if it.clicked {
                self.actions.push(Action::Focus(r));
            }
        }
        self.spacer(rows.len().saturating_sub(first + visible) as i32 * row_h);
        self.ui.close();
    }

    pub(super) fn files_window(&mut self) {
        self.ui.open(
            Kind::None,
            Layout::row().grow_x().pad(4),
            Style::default(),
            None,
        );
        self.label("covered/total symbols", WEAK);
        self.ui.close();
        let cov: Vec<(usize, usize)> = self
            .idx
            .files
            .iter()
            .map(|f| {
                (
                    f.symbols
                        .iter()
                        .filter(|s| self.map.covers(&f.path, s.start, s.end))
                        .count(),
                    f.symbols.len(),
                )
            })
            .collect();
        self.scroll_open(
            ui::id("files"),
            Layout::col().grow().pad(4),
            Style::default(),
        );
        let all: Vec<usize> = (0..self.idx.files.len()).collect();
        self.files_tree(&all, 0, &cov);
        self.ui.close();
    }

    /// `files` are sorted by path and share their first `depth` components; a run with the
    /// same next component is a directory.
    pub(super) fn files_tree(&mut self, files: &[usize], depth: usize, cov: &[(usize, usize)]) {
        let comp = |app: &App, fi: usize| {
            app.idx.files[fi]
                .path
                .split('/')
                .nth(depth)
                .unwrap_or("")
                .to_owned()
        };
        let is_file = |app: &App, fi: usize| app.idx.files[fi].path.split('/').count() == depth + 1;
        let indent = "  ".repeat(depth);
        let mut i = 0;
        while i < files.len() {
            let fi = files[i];
            if is_file(self, fi) {
                let (c, t) = cov[fi];
                let label = if t > 0 {
                    format!("{indent}{}  {c}/{t}", comp(self, fi))
                } else {
                    format!("{indent}{}", comp(self, fi))
                };
                let it = self.row(
                    vec![(label, if t > 0 && c == 0 { WEAK } else { TEXT })],
                    ui::id_n(ui::id("file"), fi),
                    self.cur_file == Some(fi),
                );
                if it.clicked {
                    self.actions.push(Action::GoTo(fi, 0));
                }
                i += 1;
                continue;
            }
            let dir = comp(self, fi);
            let j = i + files[i..]
                .iter()
                .take_while(|&&g| !is_file(self, g) && comp(self, g) == dir)
                .count();
            let (c, t) = files[i..j]
                .iter()
                .fold((0, 0), |acc, &g| (acc.0 + cov[g].0, acc.1 + cov[g].1));
            let prefix: String = self.idx.files[fi]
                .path
                .split('/')
                .take(depth + 1)
                .collect::<Vec<_>>()
                .join("/");
            let open = (depth == 0) ^ self.dir_toggled.contains(&prefix);
            let it = self.row(
                vec![(
                    format!("{indent}{} {dir}/  {c}/{t}", if open { "▾" } else { "▸" }),
                    TEXT,
                )],
                ui::id_with(ui::id("dir"), &prefix),
                false,
            );
            if it.clicked {
                self.actions.push(Action::ToggleDir(prefix.clone()));
            }
            if open {
                self.files_tree(&files[i..j], depth + 1, cov);
            }
            i = j;
        }
    }

    pub(super) fn xrefs_panel(&mut self, (w, panel_h): (i32, i32)) {
        self.ui
            .open(Kind::None, Layout::col().grow(), Style::bg(PANEL), None);
        self.grip(Panel::Xrefs);
        self.label("Xrefs", WEAK);
        self.ui.close();
        // the peek: a symbol's definition, a line inside one, or text from a file outside the repo
        let peek = match self.peek.clone() {
            Some(Peek::Sym(r))
                if r.file < self.idx.files.len()
                    && r.sym < self.idx.files[r.file].symbols.len() =>
            {
                let s = self.idx.sym(r);
                Some((
                    format!("Peek: {}", s.name),
                    format!("{}:{}", self.idx.files[r.file].path, s.start + 1),
                    Some(Action::Focus(r)),
                    Ok((r.file, s.start, s.end, None)),
                ))
            }
            Some(Peek::Line(fi, li)) if fi < self.idx.files.len() => {
                let last = self.idx.files[fi].lines.len().saturating_sub(1);
                Some((
                    "Peek".to_owned(),
                    format!("{}:{}", self.idx.files[fi].path, li + 1),
                    Some(Action::GoTo(fi, li)),
                    Ok((fi, li.saturating_sub(6), (li + 20).min(last), Some(li))),
                ))
            }
            Some(Peek::Outside(p, li, first, grid)) => Some((
                "Peek (outside)".to_owned(),
                format!("{}:{}", p.display().to_string().replace('\\', "/"), li + 1),
                None,
                Err((first, li, grid)),
            )),
            _ => None,
        };
        if let Some((title, place, go, body)) = peek {
            // the header fits the panel whatever the path's length: the buttons never leave it
            let cols = (w / self.cell.0.max(1)) as usize;
            let title = trunc(&title, 24.min(cols / 2));
            let place = trunc_left(&place, cols.saturating_sub(title.chars().count() + 14));
            self.ui.open(
                Kind::None,
                Layout::row().grow_x().pad(4).gap(6).cross(Align::Center),
                Style::default(),
                None,
            );
            self.label(&title, TEXT);
            self.label(&place, WEAK);
            self.ui
                .leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
            if let Some(go) = go
                && self.small_button("go", ui::id("peek-go")).clicked
            {
                self.actions.push(go);
            }
            if self.small_button_w("x", 3, ui::id("peek-x")).clicked {
                self.actions.push(Action::ClosePeek);
            }
            self.ui.close();
            let rows = match &body {
                Ok((fi, start, end, _)) => {
                    end.min(&self.idx.files[*fi].lines.len().saturating_sub(1))
                        .saturating_sub(*start)
                        + 1
                }
                Err((_, _, grid)) => grid.h,
            };
            let h = (rows as i32 * self.cell.1 + 8)
                .min((self.ui.size.1 / 3).max(100))
                .min(panel_h / 2);
            self.scroll_open(
                ui::id("peek"),
                Layout::col().grow_x().h(h).pad(4),
                Style::bg(FIELD),
            );
            match body {
                Ok((fi, start, end, mark)) => {
                    self.code_block(
                        fi,
                        start,
                        end,
                        ui::id("peekcode"),
                        true,
                        &|li| (mark == Some(li)).then_some(dim(SELECTED, 120)),
                        &|_| None,
                    );
                }
                Err((first, mark, grid)) => {
                    let px = self.px;
                    let rh = self.cell.1;
                    let g = grid.clone();
                    let draw = move |gfx: &mut Gfx, r: Rect| {
                        if mark >= first && mark < first + g.h {
                            gfx.rect(
                                Rect::new(r.x, r.y + (mark - first) as i32 * rh, r.w, rh),
                                dim(SELECTED, 120),
                            );
                        }
                        gfx.glyphs(r.x, r.y, px, &g);
                    };
                    self.ui.leaf(
                        Kind::Custom(Box::new(draw)),
                        Layout::row().grow_x().h(grid.h as i32 * rh),
                        Style::default(),
                        Some(ui::id("peekout")),
                    );
                }
            }
            self.ui.close();
        }
        let Some(cur) = self.focus else {
            self.label("no symbol selected", WEAK);
            self.ui.close();
            return;
        };
        let s = self.idx.sym(cur);
        let (name, place) = (
            s.name.clone(),
            format!(
                "{} {}:{}-{}",
                s.kind,
                self.idx.files[cur.file].path,
                s.start + 1,
                s.end + 1
            ),
        );
        let pending = self.idx.files[cur.file].pending;
        let (callers, callees, refs) = (s.callers.clone(), s.callees.clone(), s.refs.clone());
        // references are asked of the server for the symbol on show, once per text of its file
        let f = &self.idx.files[cur.file];
        let (line, col) = index::name_position(&self.idx, cur);
        let probe = Probe {
            path: f.path.clone(),
            hash: f.hash,
            line,
            col,
        };
        let asking = f.backend == index::Backend::Server && refs.is_empty();
        if asking
            && !self.lookup.refs_asked.contains(&probe)
            && let Some(lang) = index::lang_for(&probe.path)
            && let Some(tx) = self.server(lang)
        {
            let _ = tx.send(Req::Refs(probe.clone()));
            self.lookup.asked += 1;
            self.lookup.refs_asked.insert(probe);
        }
        self.ui.open(
            Kind::None,
            Layout::col().grow_x().pad(4),
            Style::default(),
            None,
        );
        self.label(&name, TEXT);
        self.label(&place, WEAK);
        if pending {
            let server =
                index::lang_for(&self.idx.files[cur.file].path).map_or("the server", |l| l.server);
            self.label(&format!("waiting for {server}"), WEAK);
        }
        self.ui.close();
        self.scroll_open(
            ui::id("xrefs"),
            Layout::col().grow().pad(4),
            Style::default(),
        );
        let dimmed = if pending { dim(WEAK, 120) } else { TEXT };
        for (title, list, name) in [
            ("Xrefs to", &callers, "xto"),
            ("Xrefs from", &callees, "xfrom"),
        ] {
            self.label(&format!("{title} ({})", list.len()), WEAK);
            for (i, r) in list.iter().enumerate() {
                let it = self.row(
                    vec![(cli::describe(&self.idx, *r), dimmed)],
                    ui::id_n(ui::id(name), i),
                    false,
                );
                if it.clicked && !pending {
                    self.actions.push(Action::Focus(*r));
                }
            }
        }
        let shown = refs
            .iter()
            .filter(|(p, _)| self.idx.find_file(p).is_some())
            .count();
        self.label(
            &if asking && self.lookup.asked > 0 {
                "References (asking the server)".to_owned()
            } else {
                format!("References ({shown})")
            },
            WEAK,
        );
        for (i, (path, line)) in refs.iter().enumerate() {
            if let Some(fi) = self.idx.find_file(path) {
                let t = self.idx.files[fi]
                    .lines
                    .get(*line as usize)
                    .map(|l| l.trim())
                    .unwrap_or("")
                    .to_owned();
                let it = self.row(
                    vec![(format!("{path}:{}: ", line + 1), WEAK), (t, dimmed)],
                    ui::id_n(ui::id("xref"), i),
                    false,
                );
                if it.clicked {
                    self.actions.push(Action::GoTo(fi, *line as usize));
                }
            }
        }
        self.ui.close();
        self.ui.close();
    }
}
