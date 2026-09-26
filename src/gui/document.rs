use super::*;

pub const CONTEXT_LINES: usize = 10;

#[derive(Clone, Copy, Default)]
pub(super) struct StepView {
    pub(super) whole: bool,
    pub(super) hidden: bool,
    pub(super) folded: bool,
    pub(super) context: (usize, usize),
    pub(super) expanded: bool,
}

impl App {
    pub(super) fn step_view(&self, pi: usize, ai: usize) -> StepView {
        self.steps.get(&(pi, ai)).copied().unwrap_or_default()
    }

    pub(super) fn remap_steps(&mut self, f: impl Fn((usize, usize)) -> Option<(usize, usize)>) {
        self.steps = self
            .steps
            .drain()
            .filter_map(|(k, v)| f(k).map(|k| (k, v)))
            .collect();
    }

    pub(super) fn path_document(&mut self) {
        let Some(pi) = self.sel_path.filter(|&pi| pi < self.map.paths.len()) else {
            self.label(
                if self.map.paths.is_empty() {
                    "no paths yet: the agent writes them (path-new, path-add in the output panel)"
                } else {
                    "pick a path on the left"
                },
                WEAK,
            );
            return;
        };
        let px = self.px;
        let diff = self
            .diffs()
            .into_iter()
            .find(|d| d.name == self.map.paths[pi].name);
        let doc_id = ui::id("document");
        let header_id = |ai: usize| ui::id_n(ui::id("step"), ai);
        let numbered = self.map.numbered(&self.idx, pi);

        if let Some((_, doc_rect)) = self.ui.content_of(doc_id) {
            let mut top = None;
            for &(ai, _, _) in &numbered {
                if let Some(r) = self.ui.interaction_of(header_id(ai)).rect
                    && (top.is_none() || r.y <= doc_rect.y + 1)
                {
                    top = Some(ai);
                }
            }
            if top.is_some() {
                self.top_step = top;
            }
            if let Some((ai, tries)) = self.scroll_to_step.take() {
                match self.ui.interaction_of(header_id(ai)).rect {
                    Some(r) => {
                        let off =
                            self.scrolls.get(&doc_id).copied().unwrap_or(0) + (r.y - doc_rect.y);
                        self.scrolls.insert(doc_id, off.max(0));
                    }
                    None if tries > 0 => self.scroll_to_step = Some((ai, tries - 1)),
                    None => {}
                }
            }
        }

        let (name, kind, tag, n, note, group) = {
            let p = &self.map.paths[pi];
            (
                p.name.clone(),
                p.kind.name(),
                p.author.tag(),
                p.anchors.len(),
                p.note.clone(),
                p.group.clone(),
            )
        };
        self.ui.open(
            Kind::None,
            Layout::row().grow_x().pad(4).gap(8).cross(Align::Center),
            Style::default(),
            None,
        );
        self.ui.leaf(
            text(&name, px + px / 3, TEXT),
            Layout::row(),
            Style::default(),
            None,
        );
        self.label(&format!("[{kind}]{tag}  {n} steps"), WEAK);
        if !group.is_empty() {
            self.label(&format!("in {group}"), WEAK);
        }
        match diff.as_ref().map(|d| d.change) {
            Some(Change::Added) => self.label("new since the parent revision", GREEN),
            Some(Change::Changed) => self.label("changed since the parent revision", GREEN),
            _ => {}
        }
        self.ui
            .leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
        if self.small_button("graph", ui::id("doc-graph")).clicked {
            self.actions.push(Action::OpenPath(pi, Tab::Graph));
        }
        if self
            .small_button("hide all code", ui::id("doc-collapse"))
            .clicked
        {
            self.actions.push(Action::CollapseAll(pi, true));
        }
        if self.small_button("show all", ui::id("doc-expand")).clicked {
            self.actions.push(Action::CollapseAll(pi, false));
        }
        if self.small_button("fold all", ui::id("doc-fold")).clicked {
            self.actions.push(Action::FoldAll(pi, true));
        }
        if self
            .small_button("unfold all", ui::id("doc-unfold"))
            .clicked
        {
            self.actions.push(Action::FoldAll(pi, false));
        }
        if self
            .small_button("delete path", ui::id("doc-delete"))
            .clicked
        {
            self.actions.push(Action::DeletePath(pi));
        }
        self.ui.close();
        self.ui.leaf(
            wrapped(
                if note.is_empty() {
                    "(no path note)"
                } else {
                    &note
                },
                px,
                if note.is_empty() { WEAK } else { TEXT },
            ),
            Layout::row().grow_x().pad(4),
            Style::default(),
            None,
        );
        let from = self.map.links_to(&name);
        if !from.is_empty() {
            self.ui.open(
                Kind::None,
                Layout::row().grow_x().pad(4).gap(6).cross(Align::Center),
                Style::default(),
                None,
            );
            self.label("linked from", WEAK);
            for (k, (p, a)) in from.into_iter().enumerate() {
                let number = self
                    .map
                    .numbered(&self.idx, p)
                    .into_iter()
                    .find(|(x, _, _)| *x == a)
                    .map(|(_, _, n)| n)
                    .unwrap_or_default();
                if self
                    .small_button(
                        &format!("{} {number}", self.map.paths[p].name),
                        ui::id_n(ui::id("from"), k),
                    )
                    .clicked
                {
                    self.actions.push(Action::SelectStep(p, a, false));
                }
            }
            self.ui.close();
        }

        let number_of: HashMap<usize, String> =
            numbered.iter().map(|(ai, _, n)| (*ai, n.clone())).collect();
        self.ui.open(
            Kind::None,
            Layout::row().grow_x().pad(4).gap(6).cross(Align::Center),
            Style::bg(PANEL).border(BORDER_TOP | BORDER_BOTTOM, BORDER),
            None,
        );
        let mut chain = Vec::new();
        let mut cur = self
            .top_step
            .filter(|&ai| ai < self.map.paths[pi].anchors.len());
        while let Some(ai) = cur {
            chain.push(ai);
            cur = self.map.paths[pi].anchors[ai]
                .parent
                .filter(|&p| p < self.map.paths[pi].anchors.len() && !chain.contains(&p));
        }
        if chain.is_empty() {
            self.label(" ", WEAK);
        }
        for (i, &ai) in chain.iter().rev().enumerate() {
            if i > 0 {
                self.label("›", WEAK);
            }
            let a = &self.map.paths[pi].anchors[ai];
            let name = if a.symbol.is_empty() {
                "(lines)".to_owned()
            } else {
                a.symbol.clone()
            };
            let file = a.file.rsplit('/').next().unwrap_or("").to_owned();
            let crumb = format!(
                "{} {name}",
                number_of.get(&ai).map(String::as_str).unwrap_or("")
            );
            let cid = ui::id_n(ui::id("crumb"), ai);
            let hovered = self.ui.interaction_of(cid).hovered;
            let it = self.ui.leaf(
                runs(
                    vec![
                        (crumb, if hovered { ACCENT } else { TEXT }),
                        (format!(" {file}"), dim(WEAK, 140)),
                    ],
                    px,
                ),
                Layout::row().pad(2),
                Style::default(),
                Some(cid),
            );
            if it.clicked {
                self.actions.push(Action::SelectStep(pi, ai, false));
            }
        }
        self.ui.close();

        self.scroll_open(doc_id, Layout::col().grow().pad(6).gap(2), Style::default());
        self.doc_steps(pi, 0, "", diff.as_ref(), &mut vec![pi], None, &mut 0);
        if let Some(d) = diff.as_ref().filter(|d| !d.removed.is_empty()) {
            self.label("steps removed since the parent revision:", WEAK);
            for a in &d.removed {
                let s = format!(
                    "- {} {}{}",
                    a.file,
                    a.symbol,
                    if a.note.is_empty() {
                        String::new()
                    } else {
                        format!("  -- {}", a.note)
                    }
                );
                self.label(&s, WEAK);
            }
        }
        self.ui.close();
    }

    fn doc_steps(
        &mut self,
        pi: usize,
        base: usize,
        prefix: &str,
        diff: Option<&crate::map::PathDiff>,
        chain: &mut Vec<usize>,
        occ: Option<usize>,
        next_occ: &mut usize,
    ) {
        let px = self.px;
        let key = |name: &str, ai: usize| match occ {
            None => ui::id_n(ui::id(name), ai),
            Some(o) => ui::id_n(ui::id_with(ui::id_n(ui::id("linked"), o), name), ai),
        };
        let mut hide_below: Option<usize> = None;
        let mut first = true;
        for (ai, depth, number) in self.map.numbered(&self.idx, pi) {
            if hide_below.is_some_and(|d| depth > d) {
                continue;
            }
            hide_below = None;
            let number = format!("{prefix}{number}");
            let indent = (base + depth) as i32 * 3 * self.cell.0;
            let (file, symbol, ls, le, stale, tag, anote) = {
                let a = &self.map.paths[pi].anchors[ai];
                (
                    a.file.clone(),
                    a.symbol.clone(),
                    a.line_start,
                    a.line_end,
                    a.stale,
                    a.author.tag(),
                    a.note.clone(),
                )
            };
            let fi = self.idx.find_file(&file);
            let sym = fi
                .zip(self.map.paths[pi].anchors[ai].sym)
                .map(|(fi, si)| &self.idx.files[fi].symbols[si])
                .map(|s| (s.start, s.end));
            let gone = match (fi, symbol.is_empty(), sym) {
                (None, _, _) => Some("file gone"),
                (Some(_), false, None) => Some("symbol gone"),
                _ => None,
            };
            let name = if symbol.is_empty() {
                "(lines)".to_owned()
            } else {
                symbol.clone()
            };
            let place = match gone {
                Some(g) => format!("{file} ({g})"),
                None => format!("{file}:{}-{}", ls + 1, le + 1),
            };
            let selected = occ.is_none() && self.sel_anchor == Some(ai);
            let view = self.step_view(pi, ai);
            let (folded, collapsed, ctx) = (view.folded, view.hidden, view.context);
            let kids = self.map.descendants(pi, ai);
            self.ui.open(
                Kind::None,
                Layout::row().grow_x().gap(6).cross(Align::Center),
                Style {
                    bg: None,
                    border: if selected { BORDER_LEFT } else { 0 },
                    border_color: ACCENT,
                },
                None,
            );
            self.ui.leaf(
                Kind::None,
                Layout::row().w(indent + 4),
                Style::default(),
                None,
            );
            if kids > 0 {
                if self
                    .small_button(if folded { "▸" } else { "▾" }, key("fold", ai))
                    .clicked
                {
                    self.actions.push(Action::ToggleFold(pi, ai));
                }
            } else {
                self.ui.leaf(
                    Kind::None,
                    Layout::row().w(3 * self.cell.0),
                    Style::default(),
                    None,
                );
            }
            let title = format!("{number}  {}{name}  ", if stale { "STALE " } else { "" });
            let hid = key("step", ai);
            let hovered = self.ui.interaction_of(hid).hovered;
            let mut r = vec![
                (
                    title,
                    if stale {
                        RED
                    } else if hovered {
                        ACCENT
                    } else {
                        TEXT
                    },
                ),
                (place, WEAK),
                (tag.to_owned(), WEAK),
            ];
            if folded {
                r.push((format!("  +{kids}"), WEAK));
            }
            if let Some(c) = diff
                .filter(|d| d.change == Change::Changed)
                .and_then(|d| d.steps.get(ai).copied().flatten())
            {
                r.push((format!("  {}", c.tag()), GREEN));
            }
            if occ.is_some() && std::mem::take(&mut first) {
                r.push((format!("  in {}", self.map.paths[pi].name), WEAK));
            }
            let it = self.ui.leaf(
                runs(r, px),
                Layout::row().pad(2),
                Style {
                    bg: if selected { Some(SELECTED) } else { None },
                    ..Default::default()
                },
                Some(hid),
            );
            if it.clicked {
                self.actions.push(Action::SelectStep(pi, ai, occ.is_none()));
            }
            if gone.is_none()
                && self
                    .small_button_w(
                        if collapsed { "code" } else { "hide code" },
                        9,
                        key("hide", ai),
                    )
                    .clicked
            {
                self.actions.push(Action::ToggleCode(pi, ai));
            }
            if !collapsed
                && sym.is_some_and(|s| s != (ls, le))
                && self
                    .small_button_w(
                        if view.whole { "slice" } else { "whole symbol" },
                        12,
                        key("whole", ai),
                    )
                    .clicked
            {
                self.actions.push(Action::ToggleWhole(pi, ai));
            }
            if ctx != (0, 0) && self.small_button("no context", key("ctx0", ai)).clicked {
                self.actions.push(Action::Context(pi, ai, 0));
            }
            let link = self.map.paths[pi].anchors[ai].link.clone();
            let target = self.map.find(&link);
            if !link.is_empty() {
                match target {
                    None => self.label(&format!("→ {link} (missing)"), RED),
                    Some(t) => {
                        if self
                            .small_button(&format!("→ {link}"), key("link", ai))
                            .clicked
                        {
                            self.actions.push(Action::OpenPath(t, Tab::Path));
                        }
                        if chain.contains(&t) {
                            self.label("expanded above", WEAK);
                        } else if self
                            .small_button_w(
                                if view.expanded { "collapse" } else { "expand" },
                                8,
                                key("expand", ai),
                            )
                            .clicked
                        {
                            self.actions.push(Action::ToggleLink(pi, ai));
                        }
                    }
                }
            }
            self.ui
                .leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
            if occ.is_none() && self.small_button("delete", key("del", ai)).clicked {
                self.actions.push(Action::DeleteStep(pi, ai));
            }
            self.ui.close();
            self.ui
                .open(Kind::None, Layout::row().grow_x(), Style::default(), None);
            self.ui.leaf(
                Kind::None,
                Layout::row().w(indent + 4 + 3 * self.cell.0),
                Style::default(),
                None,
            );
            self.ui.leaf(
                wrapped(
                    if anote.is_empty() {
                        "(no note)"
                    } else {
                        &anote
                    },
                    px,
                    if anote.is_empty() {
                        dim(WEAK, 120)
                    } else {
                        GREEN
                    },
                ),
                Layout::row().grow_x().pad(2),
                Style::default(),
                None,
            );
            self.ui.close();
            if let (Some(fi), None, false) = (fi, gone, collapsed) {
                let (blo, bhi) = if view.whole {
                    sym.unwrap_or((ls, le))
                } else {
                    (ls, le)
                };
                let last = self.idx.files[fi].lines.len().saturating_sub(1);
                let (lo, hi) = (blo.saturating_sub(ctx.0), (bhi + ctx.1).min(last));
                let marked = (lo, hi) != (ls, le);
                self.ui.open(
                    Kind::None,
                    Layout::col().grow_x(),
                    Style {
                        bg: None,
                        border: if selected { BORDER_LEFT } else { 0 },
                        border_color: ACCENT,
                    },
                    None,
                );
                if lo > 0 {
                    self.ctx_button(pi, ai, -1, indent, key("ctx-a", ai));
                }
                self.ui
                    .open(Kind::None, Layout::row().grow_x(), Style::default(), None);
                self.ui.leaf(
                    Kind::None,
                    Layout::row().w(indent + 4),
                    Style::default(),
                    None,
                );
                self.code_block(
                    fi,
                    lo,
                    hi,
                    key("doccode", ai),
                    true,
                    &|li| (marked && li >= ls && li <= le).then_some(dim(SELECTED, 120)),
                    &|_| None,
                );
                self.ui.close();
                if hi < last {
                    self.ctx_button(pi, ai, 1, indent, key("ctx-b", ai));
                }
                self.ui.close();
            }
            if folded {
                hide_below = Some(depth);
            }
            self.ui
                .leaf(Kind::None, Layout::row().h(6), Style::default(), None);
            if let Some(t) = target.filter(|t| view.expanded && !folded && !chain.contains(t)) {
                *next_occ += 1;
                let o = *next_occ;
                chain.push(t);
                self.doc_steps(
                    t,
                    base + depth + 1,
                    &format!("{number} › "),
                    None,
                    chain,
                    Some(o),
                    next_occ,
                );
                chain.pop();
            }
        }
    }

    pub(super) fn ctx_button(&mut self, pi: usize, ai: usize, dir: i8, indent: i32, id: ui::Id) {
        let label = if dir < 0 {
            format!("▲ {CONTEXT_LINES} lines above")
        } else {
            format!("▼ {CONTEXT_LINES} lines below")
        };
        self.ui
            .open(Kind::None, Layout::row().grow_x(), Style::default(), None);
        self.ui.leaf(
            Kind::None,
            Layout::row().w(indent + 4),
            Style::default(),
            None,
        );
        if self.small_button(&label, id).clicked {
            self.actions.push(Action::Context(pi, ai, dir));
        }
        self.ui.close();
    }
}
