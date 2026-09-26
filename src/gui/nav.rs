use super::*;

#[derive(Default)]
pub(super) struct History {
    pub(super) back: Vec<Loc>,
    pub(super) forward: Vec<Loc>,
    pub(super) last: Option<Loc>,
}

#[derive(Clone, PartialEq)]
pub(super) struct Loc {
    tab: Tab,
    file: Option<String>,
    sel: Option<(usize, usize)>,
    focus: Option<(String, String)>,
    path: Option<usize>,
    step: Option<usize>,
}

impl Loc {
    pub(super) fn same_place(&self, o: &Loc) -> bool {
        self.tab == o.tab
            && self.file == o.file
            && self.focus == o.focus
            && self.path == o.path
            && self.step == o.step
    }
}

impl App {
    pub(super) fn here(&self) -> Loc {
        Loc {
            tab: self.tab,
            file: self.cur_file.map(|fi| self.idx.files[fi].path.clone()),
            sel: self.sel,
            focus: self.focus.map(|r| self.idx.key(r)),
            path: self.sel_path,
            step: self.sel_anchor,
        }
    }

    pub(super) fn track_navigation(&mut self) {
        let now = self.here();
        if let Some(prev) = self.history.last.take()
            && !prev.same_place(&now)
        {
            self.history.back.push(prev);
            self.history.forward.clear();
            if self.history.back.len() > 200 {
                self.history.back.remove(0);
            }
        }
        self.history.last = Some(now);
    }

    pub(super) fn back(&mut self) {
        let Some(loc) = self.history.back.pop() else {
            return;
        };
        self.history.forward.push(self.here());
        self.go(loc);
    }

    pub(super) fn forward(&mut self) {
        let Some(loc) = self.history.forward.pop() else {
            return;
        };
        self.history.back.push(self.here());
        self.go(loc);
    }

    pub(super) fn go(&mut self, loc: Loc) {
        self.sel_path = loc.path.filter(|&pi| pi < self.map.paths.len());
        match (self.sel_path, loc.step) {
            (Some(pi), Some(ai)) if ai < self.map.paths[pi].anchors.len() => {
                self.select_step(pi, ai, false)
            }
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
            self.scroll_to = Some(a);
        }
        self.tab = loc.tab;
        self.history.last = Some(self.here());
    }

    pub(super) fn step_removed(&mut self, pi: usize, ai: usize) {
        let shift = |a: usize| {
            if a == ai {
                None
            } else if a > ai {
                Some(a - 1)
            } else {
                Some(a)
            }
        };
        self.remap_steps(|(p, a)| {
            if p == pi {
                shift(a).map(|a| (p, a))
            } else {
                Some((p, a))
            }
        });
        for loc in self
            .history
            .back
            .iter_mut()
            .chain(self.history.forward.iter_mut())
            .chain(self.history.last.iter_mut())
        {
            if loc.path == Some(pi) {
                loc.step = loc.step.and_then(shift);
            }
        }
    }

    pub(super) fn path_removed(&mut self, pi: usize) {
        let shift = |p: usize| {
            if p == pi {
                None
            } else if p > pi {
                Some(p - 1)
            } else {
                Some(p)
            }
        };
        self.remap_steps(|(p, a)| shift(p).map(|p| (p, a)));
        self.history.back.retain(|l| l.path != Some(pi));
        self.history.forward.retain(|l| l.path != Some(pi));
        for loc in self
            .history
            .back
            .iter_mut()
            .chain(self.history.forward.iter_mut())
            .chain(self.history.last.iter_mut())
        {
            loc.path = loc.path.and_then(shift);
        }
        self.top_step = None;
    }

    pub(super) fn step_by(&mut self, d: i32) {
        let Some(pi) = self.sel_path.filter(|&pi| pi < self.map.paths.len()) else {
            return;
        };
        let order = self.map.tree_order(pi);
        if order.is_empty() {
            return;
        }
        let at = self
            .sel_anchor
            .and_then(|ai| order.iter().position(|&(a, _)| a == ai))
            .map_or(0, |i| {
                (i as i32 + d).clamp(0, order.len() as i32 - 1) as usize
            });
        self.select_step(pi, order[at].0, false);
    }

    pub(super) fn focus(&mut self, r: SymRef) {
        let s = self.idx.sym(r);
        let (start, end) = (s.start, s.end);
        self.cur_file = Some(r.file);
        self.sel = Some((start, end));
        self.scroll_to = Some(start);
        self.focus = Some(r);
        if !std::mem::take(&mut self.graph.hold_look) {
            self.graph.want_look = true;
        }
    }

    pub fn select_symbol(&mut self, r: SymRef) {
        if let Some(pi) = self.sel_path {
            let file = &self.idx.files[r.file].path;
            if let Some(ai) = self.map.paths[pi]
                .anchors
                .iter()
                .position(|a| a.sym == Some(r.sym) && a.file == *file && a.off_start == 0)
            {
                self.select_step(pi, ai, false);
                return;
            }
        }
        self.sel_anchor = None;
        self.focus(r);
    }

    pub fn select_step(&mut self, pi: usize, ai: usize, in_document: bool) {
        self.sel_path = Some(pi);
        self.sel_anchor = Some(ai);
        let mut up = self.map.paths[pi].anchors[ai].parent;
        while let Some(u) = up {
            if let Some(v) = self.steps.get_mut(&(pi, u)) {
                v.folded = false;
            }
            up = self.map.paths[pi].anchors[u].parent;
        }
        let a = &self.map.paths[pi].anchors[ai];
        let (file, sym, ls, le) = (a.file.clone(), a.sym, a.line_start, a.line_end);
        match (self.idx.find_file(&file), sym) {
            (Some(fi), Some(si)) => {
                self.focus(SymRef { file: fi, sym: si });
                self.sel = Some((ls, le));
                self.scroll_to = Some(ls);
            }
            (Some(fi), None) => {
                self.cur_file = Some(fi);
                self.sel = Some((ls, le));
                self.scroll_to = Some(ls);
                if let Some(r) = self.idx.by_line(&file, ls) {
                    self.focus = Some(r);
                }
            }
            _ => {}
        }
        if !in_document {
            self.scroll_to_step = Some((ai, 5));
        }
        if matches!(self.tab, Tab::Listing | Tab::Results | Tab::Diff) {
            self.tab = Tab::Path;
        }
    }

    pub(super) fn select_path(&mut self, pi: usize) {
        self.sel_path = Some(pi);
        match self.map.tree_order(pi).first() {
            Some(&(ai, _)) => self.select_step(pi, ai, false),
            None => self.sel_anchor = None,
        }
    }

    pub(super) fn go_to_symbol(&mut self, r: SymRef) {
        self.select_symbol(r);
        if self.sel_anchor.is_none() {
            if !matches!(self.tab, Tab::Graph | Tab::Path) {
                self.tab = Tab::Listing;
            } else if let Some(pi) = self.sel_path {
                self.status = format!(
                    "{} selected; not a step of '{}'",
                    self.idx.sym(r).name,
                    self.map.paths[pi].name
                );
            }
        }
    }

    pub(super) fn open_line(&mut self, file: usize, line: usize) {
        self.cur_file = Some(file);
        self.sel = Some((line, line));
        self.scroll_to = Some(line);
        self.tab = Tab::Listing;
    }
}
