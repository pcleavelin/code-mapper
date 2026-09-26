use super::*;

pub(super) enum Req {
    Index(Vec<(String, u64)>),
    Hover(Probe),
    Def(Probe),
    Refs(Probe),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) struct Probe {
    pub(super) path: String,
    pub(super) hash: u64,
    pub(super) line: u32,
    pub(super) col: u32,
}

pub(super) struct Def {
    rel: Option<String>,
    abs: PathBuf,
    line: usize,
    first: usize,
    grid: Glyphs,
}

pub(super) enum Msg {
    File(ServerFile),
    Progress(String),
    Failed(&'static index::Lang, String),
    Done(&'static index::Lang),
    Hover(Probe, Option<String>),
    Def(Probe, Option<Def>),
    Refs(Probe, Vec<(String, u32)>),
}

pub(super) fn serve(root: PathBuf, lang: &'static index::Lang, rx: Receiver<Req>, tx: Sender<Msg>) {
    let (mut c, abs) = match index::start_server(&root, lang) {
        Ok(x) => x,
        Err(e) => {
            let _ = tx.send(Msg::Failed(lang, e));
            return;
        }
    };
    let mut queue: VecDeque<(String, u64)> = VecDeque::new();
    let (mut n, mut done) = (0, 0);
    loop {
        let req = match rx.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) if queue.is_empty() => match rx.recv() {
                Ok(r) => Some(r),
                Err(_) => return,
            },
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => return,
        };
        match req {
            Some(Req::Index(files)) => {
                if queue.is_empty() {
                    (n, done) = (0, 0);
                }
                n += files.len();
                queue.extend(files);
            }
            Some(Req::Hover(p)) => {
                let t = c.hover(&abs.join(&p.path), p.line, p.col);
                let _ = tx.send(Msg::Hover(p, t));
            }
            Some(Req::Def(p)) => {
                let d = c
                    .definition(&abs.join(&p.path), p.line, p.col)
                    .map(|(path, line, _)| {
                        let line = line as usize;
                        let text = std::fs::read_to_string(&path)
                            .unwrap_or_default()
                            .replace('\t', "    ");
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_owned();
                        let file = index::parse_file(
                            &mut index::Parsers::default(),
                            path.to_string_lossy().replace('\\', "/"),
                            &text,
                            &ext,
                        );
                        let first = line.saturating_sub(6);
                        let grid = if file.lines.is_empty() {
                            Glyphs::new(0, 0)
                        } else {
                            build_grid(&file, first, (line + 24).min(file.lines.len() - 1))
                        };
                        Def {
                            rel: lsp::relative(&path, &abs),
                            abs: path,
                            line,
                            first,
                            grid,
                        }
                    });
                let _ = tx.send(Msg::Def(p, d));
            }
            Some(Req::Refs(p)) => {
                let refs = index::references_at(&mut c, &abs, &p.path, p.line, p.col);
                let _ = tx.send(Msg::Refs(p, refs));
            }
            None => {
                let batch: Vec<(String, u64)> = (0..8).map_while(|_| queue.pop_front()).collect();
                if batch.is_empty() {
                    continue;
                }
                for f in index::index_files(&mut c, &abs, &batch) {
                    done += 1;
                    let _ = tx.send(Msg::Progress(format!("{}: {done}/{n}", lang.server)));
                    let _ = tx.send(Msg::File(f));
                }
                if queue.is_empty() {
                    let _ = tx.send(Msg::Done(lang));
                }
            }
        }
    }
}

pub(super) fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).ok().and_then(|m| m.modified().ok())
}

pub(super) fn watch(
    root: PathBuf,
    rx: Receiver<Vec<(String, Option<SystemTime>)>>,
    tx: Sender<()>,
) {
    while let Ok(list) = rx.recv() {
        while list.iter().all(|(p, mt)| mtime(&root.join(p)) == *mt) {
            std::thread::sleep(Duration::from_secs(1));
        }
        if tx.send(()).is_err() {
            return;
        }
    }
}

pub(super) fn bg<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx
}

pub(super) fn landed<T>(rx: &mut Option<Receiver<T>>) -> Option<Option<T>> {
    let got = match rx.as_ref()?.try_recv() {
        Err(TryRecvError::Empty) => return None,
        r => r.ok(),
    };
    *rx = None;
    Some(got)
}

pub(super) struct Work {
    pub(super) servers: HashMap<&'static str, (Sender<Req>, Receiver<Msg>)>,
    pub(super) no_server: HashSet<&'static str>,
    pub(super) indexing: HashSet<&'static str>,
    pub(super) progress: String,
    pub(super) restart: bool,
    pub(super) merge_wait: Vec<ServerFile>,
    pub(super) last_merge: Instant,
    pub(super) link_rx: Option<Receiver<Index>>,
    pub(super) relink: bool,
    pub(super) watch_tx: Sender<Vec<(String, Option<SystemTime>)>>,
    pub(super) watch_rx: Receiver<()>,
    pub(super) reindex_rx: Option<Receiver<Index>>,
    pub(super) base_rx: Option<Receiver<Result<Map, String>>>,
}

impl Work {
    pub(super) fn new(root: &Path) -> Work {
        let (watch_tx, list_rx) = channel();
        let (changed_tx, watch_rx) = channel();
        let root = root.to_path_buf();
        std::thread::spawn(move || watch(root, list_rx, changed_tx));
        Work {
            servers: HashMap::new(),
            no_server: HashSet::new(),
            indexing: HashSet::new(),
            progress: String::new(),
            restart: false,
            merge_wait: Vec::new(),
            last_merge: Instant::now(),
            link_rx: None,
            relink: false,
            watch_tx,
            watch_rx,
            reindex_rx: None,
            base_rx: None,
        }
    }
}

pub(super) struct MapFile {
    pub(super) path: PathBuf,
    pub(super) stamp: Option<(SystemTime, usize)>,
    pub(super) broken: bool,
    pub(super) dirty: bool,
    pub(super) last_poll: Instant,
    pub(super) warned: bool,
}

impl App {
    pub(super) fn load_base(&mut self) {
        let root = self.idx.root.clone();
        self.work.base_rx = Some(bg(move || Map::base_from_vcs(&root)));
    }

    pub(super) fn poll_base(&mut self) {
        if let Some(base) = landed(&mut self.work.base_rx) {
            (self.base, self.base_why) = match base {
                Some(Ok(m)) => (Some(m), String::new()),
                Some(Err(e)) => (None, e),
                None => (
                    None,
                    "the thread reading the parent revision's map failed".into(),
                ),
            };
        }
    }

    pub(super) fn server(&mut self, lang: &'static index::Lang) -> Option<&Sender<Req>> {
        if self.work.no_server.contains(lang.server) {
            return None;
        }
        if !self.work.servers.contains_key(lang.server) {
            let (rtx, rrx) = channel();
            let (mtx, mrx) = channel();
            let root = self.idx.root.clone();
            std::thread::spawn(move || serve(root, lang, rrx, mtx));
            self.work.servers.insert(lang.server, (rtx, mrx));
            self.status = format!("{}: starting", lang.server);
        }
        self.work.servers.get(lang.server).map(|(tx, _)| tx)
    }

    pub(super) fn start_backend(&mut self) {
        if !self.work.indexing.is_empty() {
            self.work.restart = true;
            return;
        }
        for (lang, files) in self.idx.pending() {
            let n = files.len();
            match self.server(lang) {
                Some(tx) => {
                    let _ = tx.send(Req::Index(files));
                    self.work.indexing.insert(lang.server);
                    self.work.progress = format!("{}: 0/{n}", lang.server);
                }
                None => self.idx.give_up(lang),
            }
        }
    }

    pub(super) fn poll_backend(&mut self) {
        let mut msgs = Vec::new();
        for (_, rx) in self.work.servers.values() {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        let answered = !msgs.is_empty();
        let mut files = Vec::new();
        let mut batch_ended = false;
        for m in msgs {
            match m {
                Msg::File(f) => files.push(f),
                Msg::Progress(p) => self.work.progress = p,
                Msg::Failed(lang, e) => {
                    self.work.servers.remove(lang.server);
                    self.work.no_server.insert(lang.server);
                    self.work.indexing.remove(lang.server);
                    self.idx.give_up(lang);
                    self.lookup.hovers.clear();
                    self.lookup.asked = 0;
                    self.lookup.want_def = None;
                    self.status = format!(
                        "{e}: its files keep the tree-sitter resolver, no hover or go-to for them"
                    );
                    batch_ended = true;
                }
                Msg::Done(lang) => {
                    self.work.indexing.remove(lang.server);
                    batch_ended = true;
                }
                Msg::Hover(p, t) => {
                    self.lookup.asked = self.lookup.asked.saturating_sub(1);
                    self.lookup.inflight = false;
                    self.lookup.hovers.insert(p, Some(t));
                }
                Msg::Refs(p, refs) => {
                    self.lookup.asked = self.lookup.asked.saturating_sub(1);
                    let at = self
                        .idx
                        .find_file(&p.path)
                        .filter(|&fi| self.idx.files[fi].hash == p.hash);
                    if let Some(s) = at.and_then(|fi| {
                        self.idx.files[fi]
                            .symbols
                            .iter_mut()
                            .find(|s| s.start == p.line as usize)
                    }) {
                        s.refs = refs;
                    }
                }
                Msg::Def(p, d) => {
                    self.lookup.asked = self.lookup.asked.saturating_sub(1);
                    if self.lookup.want_def.as_ref().is_some_and(|(w, _)| *w == p) {
                        let (_, intent) = self.lookup.want_def.take().unwrap();
                        match d {
                            Some(d) => {
                                let found =
                                    match d.rel.as_deref().and_then(|r| self.idx.find_file(r)) {
                                        Some(fi) => match self
                                            .idx
                                            .by_line(&self.idx.files[fi].path, d.line)
                                            .filter(|r| self.idx.sym(*r).start == d.line)
                                        {
                                            Some(r) => Peek::Sym(r),
                                            None => Peek::Line(fi, d.line),
                                        },
                                        None => {
                                            Peek::Outside(d.abs, d.line, d.first, Rc::new(d.grid))
                                        }
                                    };
                                self.land(found, intent);
                            }
                            None => self.status = "no definition found".into(),
                        }
                    }
                }
            }
        }
        self.work.merge_wait.append(&mut files);
        if !self.work.merge_wait.is_empty()
            && (batch_ended || self.work.last_merge.elapsed() >= Duration::from_secs(1))
        {
            let files = std::mem::take(&mut self.work.merge_wait);
            self.with_index_change(|app| {
                for f in files {
                    app.idx.apply(f);
                }
            });
            self.start_link();
            self.work.last_merge = Instant::now();
            if self.status.ends_with(" symbols indexed") {
                self.status = self.indexed_status();
            }
        }
        if answered && self.status.ends_with(": starting") {
            self.status = self.indexed_status();
        }
        if batch_ended && self.work.indexing.is_empty() {
            self.idx.save_cache();
            self.work.progress.clear();
            if std::mem::take(&mut self.work.restart) {
                self.start_backend();
            }
        }
    }

    pub(super) fn poll_disk(&mut self) {
        if self.work.watch_rx.try_recv().is_ok() {
            let root = self.idx.root.clone();
            self.work.reindex_rx = Some(bg(move || index::build(&root)));
            self.status = "re-indexing (source changed)".into();
        }
        if self.file.last_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.file.last_poll = Instant::now();
        let mt = crate::map::stamp(&self.file.path);
        if mt != self.file.stamp {
            if self.file.dirty {
                if !self.file.warned {
                    self.status = "map changed on disk while you have unsaved changes: save overwrites it, or use the command line to reload".into();
                    self.file.warned = true;
                }
            } else {
                self.file.stamp = mt;
                if let Err(e) = self.reload_map() {
                    self.file.broken = true;
                    self.status = format!(
                        "{e}; the map in memory is kept and cannot be saved until the one on disk reads"
                    );
                    return;
                }
                self.file.broken = false;
                self.load_base();
                self.status = "map reloaded (changed on disk)".into();
            }
        }
    }

    pub(super) fn reload_map(&mut self) -> Result<(), String> {
        let was: Vec<(String, usize)> = self
            .map
            .paths
            .iter()
            .map(|p| (p.name.clone(), p.anchors.len()))
            .collect();
        let path_name = self.sel_path.map(|pi| self.map.paths[pi].name.clone());
        let step = self.sel_anchor.zip(self.sel_path).map(|(ai, pi)| {
            let a = &self.map.paths[pi].anchors[ai];
            (ai, a.file.clone(), a.symbol.clone(), a.off_start, a.off_end)
        });
        self.map = Map::load(&self.file.path)?;
        self.map.resolve_all(&self.idx);
        self.sel_path =
            path_name.and_then(|name| self.map.paths.iter().position(|p| p.name == name));
        self.sel_anchor = match (self.sel_path, step) {
            (Some(pi), Some((ai, file, symbol, os, oe))) => self.map.paths[pi]
                .anchors
                .get(ai)
                .filter(|a| {
                    a.file == file && a.symbol == symbol && (a.off_start, a.off_end) == (os, oe)
                })
                .map(|_| ai),
            _ => None,
        };
        let kept: HashMap<usize, usize> = was
            .iter()
            .enumerate()
            .filter_map(|(old, (name, n))| {
                self.map
                    .paths
                    .iter()
                    .position(|p| p.name == *name && p.anchors.len() == *n)
                    .map(|new| (old, new))
            })
            .collect();
        self.remap_steps(|(pi, ai)| kept.get(&pi).map(|&p| (p, ai)));
        Ok(())
    }

    pub(super) fn with_index_change(&mut self, change: impl FnOnce(&mut App)) {
        let focus = self.focus.map(|r| self.idx.key(r));
        let cur_file = self.cur_file.map(|fi| self.idx.files[fi].path.clone());
        let peek_sym = match self.peek {
            Some(Peek::Sym(r)) => Some(self.idx.key(r)),
            _ => None,
        };
        let peek_line = match &self.peek {
            Some(Peek::Line(fi, li)) => Some((self.idx.files[*fi].path.clone(), *li)),
            _ => None,
        };
        let graph = self.graph.save(&self.idx);
        change(self);
        self.graph.restore(&self.idx, graph);
        if let Some(k) = peek_sym {
            self.peek = self.idx.by_key(&k).map(Peek::Sym);
        } else if let Some((p, li)) = peek_line {
            self.peek = self.idx.find_file(&p).map(|fi| Peek::Line(fi, li));
        }
        self.lookup.hovers.clear();
        self.grids.clear();
        self.map.resolve_all(&self.idx);
        self.focus = focus.and_then(|k| self.idx.by_key(&k));
        self.cur_file = cur_file.and_then(|p| self.idx.find_file(&p));
    }

    pub(super) fn watch_files(&self) {
        let _ = self.work.watch_tx.send(
            self.idx
                .files
                .iter()
                .map(|f| (f.path.clone(), f.mtime))
                .collect(),
        );
    }

    pub(super) fn poll_reindex(&mut self) {
        let Some(idx) = landed(&mut self.work.reindex_rx) else {
            return;
        };
        match idx {
            Some(idx) => {
                self.with_index_change(|app| app.idx = idx);
                self.results.clear();
                self.status = format!("re-indexed: {} (source changed)", self.indexed_status());
            }
            None => self.status = "re-index failed; the index in use is the old one".into(),
        }
        self.watch_files();
        self.start_backend();
    }

    pub(super) fn start_link(&mut self) {
        if self.work.link_rx.is_some() {
            self.work.relink = true;
            return;
        }
        let mut snap = self.idx.symbols_only();
        self.work.link_rx = Some(bg(move || {
            snap.link();
            snap
        }));
    }

    pub(super) fn poll_link(&mut self) {
        let Some(done) = landed(&mut self.work.link_rx) else {
            return;
        };
        if let Some(linked) = done {
            self.idx.take_edges(&linked);
        }
        if std::mem::take(&mut self.work.relink) {
            self.start_link();
        }
    }

    pub(super) fn indexed_status(&self) -> String {
        format!(
            "{} files, {} symbols indexed",
            self.idx.files.len(),
            self.idx
                .files
                .iter()
                .map(|f| f.symbols.len())
                .sum::<usize>()
        )
    }

    pub(super) fn working(&self) -> bool {
        self.lookup.asked > 0
            || !self.work.indexing.is_empty()
            || self.work.base_rx.is_some()
            || self.work.reindex_rx.is_some()
            || self.work.link_rx.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_job_that_panics_lands_as_nothing() {
        let mut rx = Some(bg(|| -> u32 { panic!("the job failed") }));
        let landed_once = loop {
            if let Some(got) = landed(&mut rx) {
                break got;
            }
            std::thread::yield_now();
        };
        assert_eq!(landed_once, None);
        assert!(rx.is_none());
    }
}
