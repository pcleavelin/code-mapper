use super::*;

impl App {
    pub(super) fn diffs(&self) -> Vec<PathDiff> {
        self.base
            .as_ref()
            .map(|b| self.map.diff(b))
            .unwrap_or_default()
    }

    pub(super) fn listing(&mut self) {
        let Some(fi) = self.cur_file else {
            self.label("click a symbol to open its file", WEAK);
            return;
        };
        let id = ui::id("listing");
        let row_h = self.cell.1;
        let n = self.idx.files[fi].lines.len();
        if let Some(line) = self.scroll_to.take() {
            let h = self.ui.content_of(id).map_or(600, |(_, r)| r.h);
            self.scrolls.insert(
                id,
                ((line as i32 - 3).max(0) * row_h).min((n as i32 * row_h - h).max(0)),
            );
        }
        self.ui.open(
            Kind::None,
            Layout::row().grow_x().pad(4).gap(8).cross(Align::Center),
            Style::default(),
            None,
        );
        let p = self.idx.files[fi].path.clone();
        self.label(&p, TEXT);
        self.label("click a line, shift-click to extend, then 'pin selection'; double-click or ctrl-click an identifier to jump; alt-click to peek", WEAK);
        self.ui
            .leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
        self.label("line", WEAK);
        self.field(Which::GotoLine, "", 8 * self.cell.0);
        self.ui.close();
        let it = self.scroll_open(id, Layout::col().grow().pad(4), Style::bg(FIELD));
        let (lo, hi) = self
            .sel
            .map(|(a, b)| (a.min(b), a.max(b)))
            .unwrap_or((usize::MAX, usize::MAX));
        let anchors: Vec<(usize, usize, bool)> = self
            .sel_path
            .map(|pi| {
                self.map.paths[pi]
                    .anchors
                    .iter()
                    .filter(|a| a.file == self.idx.files[fi].path)
                    .map(|a| (a.line_start, a.line_end, a.stale))
                    .collect()
            })
            .unwrap_or_default();
        let (first, visible) = self.rows_window(id, it.rect, n, row_h, 60);
        let last = (first + visible).min(n).saturating_sub(1);
        if n > 0 && first <= last {
            let bar = |li: usize| {
                anchors
                    .iter()
                    .find(|&&(s, e, _)| li >= s && li <= e)
                    .map(|&(_, _, stale)| if stale { RED } else { GREEN })
            };
            let (it, at) = self.code_block(
                fi,
                first,
                last,
                ui::id("lines"),
                true,
                &|li| (li >= lo && li <= hi).then_some(dim(SELECTED, 160)),
                &bar,
            );
            let mods = self.ui.input.mods;
            if let Some((li, _)) = at
                && it.clicked
                && !mods.ctrl
                && !mods.alt
                && !it.double_clicked
            {
                self.actions.push(Action::SelectLine(li, mods.shift));
            }
        }
        self.spacer(n.saturating_sub(first + visible) as i32 * row_h + row_h);
        self.ui.close();
    }

    pub(super) fn results_view(&mut self) {
        if self.results.is_empty() {
            self.label(
                if self.fields[Which::Search as usize].text.is_empty() {
                    "type a regex in the search box and press enter"
                } else {
                    "no hits"
                },
                WEAK,
            );
            return;
        }
        let id = ui::id("results");
        let row_h = self.cell.1 + 4;
        let it = self.scroll_open(id, Layout::col().grow().pad(4), Style::default());
        let (first, visible) = self.rows_window(id, it.rect, self.results.len(), row_h, 60);
        let rows: Vec<(usize, usize)> = self
            .results
            .iter()
            .copied()
            .skip(first)
            .take(visible)
            .collect();
        for (k, (fi, li)) in rows.into_iter().enumerate() {
            let f = &self.idx.files[fi];
            let line = f.lines[li].trim().to_owned();
            let it = self.row(
                vec![(format!("{}:{}: ", f.path, li + 1), WEAK), (line, TEXT)],
                ui::id_n(ui::id("hit"), first + k),
                false,
            );
            if it.clicked {
                self.actions.push(Action::GoTo(fi, li));
            }
        }
        self.spacer(self.results.len().saturating_sub(first + visible) as i32 * row_h);
        self.ui.close();
    }

    pub(super) fn diff_view(&mut self) {
        self.ui.open(
            Kind::None,
            Layout::row().grow_x().pad(4).gap(8).cross(Align::Center),
            Style::default(),
            None,
        );
        self.label("Changes against the parent revision", TEXT);
        self.label("@- in jj, HEAD in git", WEAK);
        if self.small_button("refresh", ui::id("diff-refresh")).clicked {
            self.load_base();
        }
        self.ui.close();
        if self.base.is_none() {
            let why = if self.work.base_rx.is_some() {
                "reading the parent revision..."
            } else {
                self.base_why.as_str()
            };
            self.label(&format!("no map to compare with: {why}"), WEAK);
            return;
        }
        let diffs = self.diffs();
        if diffs.iter().all(|d| d.change == Change::Same) {
            self.label("no changes", WEAK);
            return;
        }
        self.scroll_open(
            ui::id("diff"),
            Layout::col().grow().pad(4),
            Style::default(),
        );
        for (k, d) in diffs.iter().enumerate() {
            let (mark, color) = match d.change {
                Change::Same => continue,
                Change::Added => ("+", GREEN),
                Change::Removed => ("-", RED),
                Change::Changed => ("~", GREEN),
            };
            let summary = match d.change {
                Change::Added => format!("{} steps", d.steps.len()),
                Change::Removed => format!("{} steps", d.removed.len()),
                _ => {
                    let count = |c: StepChange| d.steps.iter().filter(|s| **s == Some(c)).count();
                    let mut parts = Vec::new();
                    for (n, what) in [
                        (count(StepChange::Added), "new"),
                        (d.removed.len(), "removed"),
                        (count(StepChange::Repinned), "re-pinned"),
                        (count(StepChange::NoteEdited), "note edited"),
                        (count(StepChange::Relinked), "link changed"),
                    ] {
                        if n > 0 {
                            parts.push(format!("{n} {what}"));
                        }
                    }
                    if d.note_changed {
                        parts.push("path note, kind or group changed".into());
                    }
                    parts.join(", ")
                }
            };
            let pi = self.map.find(&d.name);
            let it = self.row(
                vec![(format!("{mark} {}   ", d.name), color), (summary, WEAK)],
                ui::id_n(ui::id("diffrow"), k),
                false,
            );
            if it.clicked {
                match pi {
                    Some(pi) => self.actions.push(Action::OpenPath(pi, Tab::Path)),
                    None => {
                        self.status = format!(
                            "'{}' exists only in the parent revision; its {} steps are listed under the path it was removed from",
                            d.name,
                            d.removed.len()
                        )
                    }
                }
            }
            if let Some(pi) = pi.filter(|_| d.change == Change::Changed) {
                for (i, c) in d.steps.iter().enumerate() {
                    if let Some(c) = c {
                        let a = &self.map.paths[pi].anchors[i];
                        let s = format!(
                            "    {} {} {} {}:{}-{}  {}",
                            if *c == StepChange::Added { "+" } else { "~" },
                            self.step_label(pi, i),
                            a.symbol,
                            a.file,
                            a.line_start + 1,
                            a.line_end + 1,
                            c.tag()
                        );
                        self.label(&s, WEAK);
                    }
                }
            }
            for a in &d.removed {
                let s = format!("    - {} {} (removed)", a.file, a.symbol);
                self.label(&s, WEAK);
            }
        }
        self.ui.close();
    }
}
