//! The path document, the reader's landing view: one path as a column of steps with their
//! notes and code, and the per-step view state (folds, hidden code, context).

use super::*;

/// Lines one press of a context button adds above or below a step's code.
pub const CONTEXT_LINES: usize = 10;

impl App {
    /// Carry the whole-symbol, code, fold and context records over to new (path, step) keys:
    /// `f` gives a record's new key, or None when the record goes.
    pub(super) fn remap_steps(&mut self, f: impl Fn((usize, usize)) -> Option<(usize, usize)>) {
        for set in [&mut self.expanded_steps, &mut self.collapsed, &mut self.folded] {
            *set = set.drain().filter_map(&f).collect();
        }
        self.context = self.context.drain().filter_map(|(k, v)| f(k).map(|k| (k, v))).collect();
    }

    /// The reader's landing view: the selected path as one document.
    pub(super) fn path_document(&mut self) {
        let Some(pi) = self.sel_path.filter(|&pi| pi < self.map.paths.len()) else {
            self.label(if self.map.paths.is_empty() { "no paths yet: the agent writes them (path-new, path-add in the output panel)" } else { "pick a path on the left" }, WEAK);
            return;
        };
        let px = self.px;
        let diff = self.diffs().into_iter().find(|d| d.name == self.map.paths[pi].name);
        let doc_id = ui::id("document");
        let header_id = |ai: usize| ui::id_n(ui::id("step"), ai);
        let numbered = self.map.numbered(&self.idx, pi);

        // from last frame's rectangles: the step under the top of the viewport, and a pending
        // scroll to a selected step
        if let Some((_, doc_rect)) = self.ui.content_of(doc_id) {
            let mut top = None;
            for &(ai, _, _) in &numbered {
                if let Some(r) = self.ui.interaction_of(header_id(ai)).rect {
                    if top.is_none() || r.y <= doc_rect.y + 1 {
                        top = Some(ai);
                    }
                }
            }
            if top.is_some() {
                self.top_step = top;
            }
            if let Some((ai, tries)) = self.scroll_to_step.take() {
                match self.ui.interaction_of(header_id(ai)).rect {
                    Some(r) => {
                        let off = self.scrolls.get(&doc_id).copied().unwrap_or(0) + (r.y - doc_rect.y);
                        self.scrolls.insert(doc_id, off.max(0));
                    }
                    None if tries > 0 => self.scroll_to_step = Some((ai, tries - 1)),
                    None => {}
                }
            }
        }

        // header
        let (name, kind, tag, n, note) = {
            let p = &self.map.paths[pi];
            (p.name.clone(), p.kind.name(), p.author.tag(), p.anchors.len(), p.note.clone())
        };
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(8).cross(Align::Center), Style::default(), None);
        self.ui.leaf(text(&name, px + px / 3, TEXT), Layout::row(), Style::default(), None);
        self.label(&format!("[{kind}]{tag}  {n} steps"), WEAK);
        match diff.as_ref().map(|d| d.change) {
            Some(Change::Added) => self.label("new since the parent revision", GREEN),
            Some(Change::Changed) => self.label("changed since the parent revision", GREEN),
            _ => {}
        }
        self.ui.leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
        if self.small_button("graph", ui::id("doc-graph")).clicked {
            self.actions.push(Action::OpenPath(pi, Tab::Graph));
        }
        if self.small_button("hide all code", ui::id("doc-collapse")).clicked {
            self.actions.push(Action::CollapseAll(pi, true));
        }
        if self.small_button("show all", ui::id("doc-expand")).clicked {
            self.actions.push(Action::CollapseAll(pi, false));
        }
        if self.small_button("fold all", ui::id("doc-fold")).clicked {
            self.actions.push(Action::FoldAll(pi, true));
        }
        if self.small_button("unfold all", ui::id("doc-unfold")).clicked {
            self.actions.push(Action::FoldAll(pi, false));
        }
        if self.small_button("delete path", ui::id("doc-delete")).clicked {
            self.actions.push(Action::DeletePath(pi));
        }
        self.ui.close();
        self.ui.leaf(wrapped(if note.is_empty() { "(no path note)" } else { &note }, px, if note.is_empty() { WEAK } else { TEXT }), Layout::row().grow_x().pad(4), Style::default(), None);

        // breadcrumb: the ancestors of the step under the top of the viewport
        let number_of: HashMap<usize, String> = numbered.iter().map(|(ai, _, n)| (*ai, n.clone())).collect();
        self.ui.open(Kind::None, Layout::row().grow_x().pad(4).gap(6).cross(Align::Center), Style::bg(PANEL).border(BORDER_TOP | BORDER_BOTTOM, BORDER), None);
        let mut chain = Vec::new();
        let mut cur = self.top_step.filter(|&ai| ai < self.map.paths[pi].anchors.len());
        while let Some(ai) = cur {
            chain.push(ai);
            cur = self.map.paths[pi].anchors[ai].parent.filter(|&p| p < self.map.paths[pi].anchors.len() && !chain.contains(&p));
        }
        if chain.is_empty() {
            self.label(" ", WEAK);
        }
        for (i, &ai) in chain.iter().rev().enumerate() {
            if i > 0 {
                self.label("›", WEAK);
            }
            let a = &self.map.paths[pi].anchors[ai];
            let name = if a.symbol.is_empty() { "(lines)".to_owned() } else { a.symbol.clone() };
            let file = a.file.rsplit('/').next().unwrap_or("").to_owned();
            let crumb = format!("{} {name}", number_of.get(&ai).map(String::as_str).unwrap_or(""));
            let cid = ui::id_n(ui::id("crumb"), ai);
            let hovered = self.ui.interaction_of(cid).hovered;
            let it = self.ui.leaf(runs(vec![(crumb, if hovered { ACCENT } else { TEXT }), (format!(" {file}"), dim(WEAK, 140))], px), Layout::row().pad(2), Style::default(), Some(cid));
            if it.clicked {
                self.actions.push(Action::SelectStep(pi, ai, false));
            }
        }
        self.ui.close();

        // the steps
        self.scroll_open(doc_id, Layout::col().grow().pad(6).gap(2), Style::default());
        let mut hide_below: Option<usize> = None;
        for &(ai, depth, ref number) in &numbered {
            if hide_below.is_some_and(|d| depth > d) {
                continue;
            }
            hide_below = None;
            let indent = depth as i32 * 3 * self.cell.0;
            let (file, symbol, ls, le, stale, tag, anote) = {
                let a = &self.map.paths[pi].anchors[ai];
                (a.file.clone(), a.symbol.clone(), a.line_start, a.line_end, a.stale, a.author.tag(), a.note.clone())
            };
            let fi = self.idx.find_file(&file);
            let sym = fi.zip(self.map.paths[pi].anchors[ai].sym).map(|(fi, si)| &self.idx.files[fi].symbols[si]).map(|s| (s.start, s.end));
            let gone = match (fi, symbol.is_empty(), sym) {
                (None, _, _) => Some("file gone"),
                (Some(_), false, None) => Some("symbol gone"),
                _ => None,
            };
            let name = if symbol.is_empty() { "(lines)".to_owned() } else { symbol.clone() };
            let place = match gone {
                Some(g) => format!("{file} ({g})"),
                None => format!("{file}:{}-{}", ls + 1, le + 1),
            };
            let selected = self.sel_anchor == Some(ai);
            let folded = self.folded.contains(&(pi, ai));
            let collapsed = self.collapsed.contains(&(pi, ai));
            let ctx = self.context.get(&(pi, ai)).copied().unwrap_or((0, 0));
            let kids = self.map.descendants(pi, ai);
            // header row
            self.ui.open(Kind::None, Layout::row().grow_x().gap(6).cross(Align::Center), Style { bg: None, border: if selected { BORDER_LEFT } else { 0 }, border_color: ACCENT }, None);
            self.ui.leaf(Kind::None, Layout::row().w(indent + 4), Style::default(), None);
            if kids > 0 {
                if self.small_button(if folded { "▸" } else { "▾" }, ui::id_n(ui::id("fold"), ai)).clicked {
                    self.actions.push(Action::ToggleFold(pi, ai));
                }
            } else {
                self.ui.leaf(Kind::None, Layout::row().w(3 * self.cell.0), Style::default(), None);
            }
            let title = format!("{number}  {}{name}  ", if stale { "STALE " } else { "" });
            let hid = header_id(ai);
            let hovered = self.ui.interaction_of(hid).hovered;
            let mut r = vec![(title, if stale { RED } else if hovered { ACCENT } else { TEXT }), (place, WEAK), (tag.to_owned(), WEAK)];
            if folded {
                r.push((format!("  +{kids}"), WEAK));
            }
            if let Some(c) = diff.as_ref().filter(|d| d.change == Change::Changed).and_then(|d| d.steps.get(ai).copied().flatten()) {
                r.push((format!("  {}", c.tag()), GREEN));
            }
            let it = self.ui.leaf(runs(r, px), Layout::row().pad(2), Style { bg: if selected { Some(SELECTED) } else { None }, ..Default::default() }, Some(hid));
            if it.clicked {
                self.actions.push(Action::SelectStep(pi, ai, true));
            }
            // toggles keep their width across labels, and the one destructive button sits alone
            // at the far right, so nothing slides under a pointer that clicks twice
            if gone.is_none() && self.small_button_w(if collapsed { "code" } else { "hide code" }, 9, ui::id_n(ui::id("hide"), ai)).clicked {
                self.actions.push(Action::ToggleCode(pi, ai));
            }
            if !collapsed && sym.is_some_and(|s| s != (ls, le)) {
                let whole = self.expanded_steps.contains(&(pi, ai));
                if self.small_button_w(if whole { "slice" } else { "whole symbol" }, 12, ui::id_n(ui::id("whole"), ai)).clicked {
                    self.actions.push(Action::ToggleStep(pi, ai));
                }
            }
            if ctx != (0, 0) && self.small_button("no context", ui::id_n(ui::id("ctx0"), ai)).clicked {
                self.actions.push(Action::Context(pi, ai, 0));
            }
            self.ui.leaf(Kind::None, Layout::row().grow_x(), Style::default(), None);
            if self.small_button("delete", ui::id_n(ui::id("del"), ai)).clicked {
                self.actions.push(Action::DeleteStep(pi, ai));
            }
            self.ui.close();
            // note
            self.ui.open(Kind::None, Layout::row().grow_x(), Style::default(), None);
            self.ui.leaf(Kind::None, Layout::row().w(indent + 4 + 3 * self.cell.0), Style::default(), None);
            self.ui.leaf(wrapped(if anote.is_empty() { "(no note)" } else { &anote }, px, if anote.is_empty() { dim(WEAK, 120) } else { GREEN }), Layout::row().grow_x().pad(2), Style::default(), None);
            self.ui.close();
            // code: the slice, or the whole symbol, plus the context asked for above and below;
            // the slice is highlighted whenever anything else shows
            if let (Some(fi), None, false) = (fi, gone, collapsed) {
                let whole = self.expanded_steps.contains(&(pi, ai));
                let (blo, bhi) = if whole { sym.unwrap_or((ls, le)) } else { (ls, le) };
                let last = self.idx.files[fi].lines.len().saturating_sub(1);
                let (lo, hi) = (blo.saturating_sub(ctx.0), (bhi + ctx.1).min(last));
                let marked = (lo, hi) != (ls, le);
                self.ui.open(Kind::None, Layout::col().grow_x(), Style { bg: None, border: if selected { BORDER_LEFT } else { 0 }, border_color: ACCENT }, None);
                if lo > 0 {
                    self.ctx_button(pi, ai, -1, indent);
                }
                self.ui.open(Kind::None, Layout::row().grow_x(), Style::default(), None);
                self.ui.leaf(Kind::None, Layout::row().w(indent + 4), Style::default(), None);
                self.code_block(fi, lo, hi, ui::id_n(ui::id("doccode"), ai), true, &|li| (marked && li >= ls && li <= le).then_some(dim(SELECTED, 120)), &|_| None);
                self.ui.close();
                if hi < last {
                    self.ctx_button(pi, ai, 1, indent);
                }
                self.ui.close();
            }
            if folded {
                hide_below = Some(depth);
            }
            self.ui.leaf(Kind::None, Layout::row().h(6), Style::default(), None);
        }
        if let Some(d) = diff.as_ref().filter(|d| !d.removed.is_empty()) {
            self.label("steps removed since the parent revision:", WEAK);
            for a in &d.removed {
                let s = format!("- {} {}{}", a.file, a.symbol, if a.note.is_empty() { String::new() } else { format!("  -- {}", a.note) });
                self.label(&s, WEAK);
            }
        }
        self.ui.close();
    }

    /// The row above (`dir` -1) or below (1) a step's code whose button shows more lines there.
    pub(super) fn ctx_button(&mut self, pi: usize, ai: usize, dir: i8, indent: i32) {
        let (label, name) = if dir < 0 { (format!("▲ {CONTEXT_LINES} lines above"), "ctx-a") } else { (format!("▼ {CONTEXT_LINES} lines below"), "ctx-b") };
        self.ui.open(Kind::None, Layout::row().grow_x(), Style::default(), None);
        self.ui.leaf(Kind::None, Layout::row().w(indent + 4), Style::default(), None);
        if self.small_button(&label, ui::id_n(ui::id(name), ai)).clicked {
            self.actions.push(Action::Context(pi, ai, dir));
        }
        self.ui.close();
    }
}
