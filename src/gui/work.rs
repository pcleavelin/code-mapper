//! Everything that runs off the frame: a language server thread per language that indexes and
//! answers hover and definition, the watcher on the source files, and the rebuild, link and
//! parent-map jobs, with the polling that folds their results into the app.

use super::*;

/// A request to a language's server thread.
pub(super) enum Req {
    Index(Vec<(String, u64)>),
    Hover(Probe),
    Def(Probe),
}

/// A position asked about: the file's path and text hash, so an answer for text that has
/// since changed is never used, and the 0-based line and UTF-16 column of an identifier.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(super) struct Probe {
    pub(super) path: String,
    pub(super) hash: u64,
    pub(super) line: u32,
    pub(super) col: u32,
}

/// A definition the server found: the file relative to the root when it is inside, the
/// absolute path either way, the 0-based line, and the lines around it drawn from the file
/// on disk (starting at line `first`) for a peek when the file is not indexed.
pub(super) struct Def {
    rel: Option<String>,
    abs: PathBuf,
    line: usize,
    first: usize,
    grid: Glyphs,
}

/// What the language-server thread sends back.
pub(super) enum Msg {
    File(ServerFile),
    Progress(String),
    Failed(&'static index::Lang, String),
    Done(&'static index::Lang),
    Hover(Probe, Option<String>),
    Def(Probe, Option<Def>),
}

/// A language's server, alive on its own thread for the life of the window. It indexes the
/// files it is sent a batch at a time and answers hover and definition requests between
/// files, so the pointer never waits behind a batch.
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
                let d = c.definition(&abs.join(&p.path), p.line, p.col).map(|(path, line, _)| {
                    let line = line as usize;
                    let text = std::fs::read_to_string(&path).unwrap_or_default().replace('\t', "    ");
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_owned();
                    let file = index::parse_file(&mut index::Parsers::default(), path.to_string_lossy().replace('\\', "/"), &text, &ext);
                    let first = line.saturating_sub(6);
                    let grid = if file.lines.is_empty() { Glyphs::new(0, 0) } else { build_grid(&file, first, (line + 24).min(file.lines.len() - 1)) };
                    Def { rel: lsp::relative(&path, &abs), abs: path, line, first, grid }
                });
                let _ = tx.send(Msg::Def(p, d));
            }
            None => {
                let Some((path, hash)) = queue.pop_front() else { continue };
                let f = index::index_file(&mut c, &abs, &path, hash);
                done += 1;
                let _ = tx.send(Msg::Progress(format!("{}: {done}/{n}", lang.server)));
                let _ = tx.send(Msg::File(f));
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

/// The indexed files' modification times, compared against the disk once a second off the main
/// thread. Each list the main thread sends is watched until a file differs from it, which is
/// reported once; the thread then waits for the refreshed list the main thread sends after the
/// re-index, so it never reports the same edit twice.
pub(super) fn watch(root: PathBuf, rx: Receiver<Vec<(String, Option<SystemTime>)>>, tx: Sender<()>) {
    while let Ok(list) = rx.recv() {
        while list.iter().all(|(p, mt)| mtime(&root.join(p)) == *mt) {
            std::thread::sleep(Duration::from_secs(1));
        }
        if tx.send(()).is_err() {
            return;
        }
    }
}

/// Run `f` on its own thread; its result arrives on the returned channel.
pub(super) fn bg<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Receiver<T> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx
}

/// A thread's result: None while it runs, Some(None) when it died without one, Some(Some(value))
/// when it answered. The receiver is dropped once either lands.
pub(super) fn landed<T>(rx: &mut Option<Receiver<T>>) -> Option<Option<T>> {
    let got = match rx.as_ref()?.try_recv() {
        Err(TryRecvError::Empty) => return None,
        r => r.ok(),
    };
    *rx = None;
    Some(got)
}

impl App {
    /// Ask jj for the parent revision's map on a thread; `poll_base` picks it up.
    pub(super) fn load_base(&mut self) {
        let root = self.idx.root.clone();
        self.base_rx = Some(bg(move || Map::base_from_vcs(&root)));
    }

    pub(super) fn poll_base(&mut self) {
        if let Some(base) = self.base_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.base = base;
            self.base_rx = None;
        }
    }

    /// The live server for a language, started on first use. None once it has failed to start.
    pub(super) fn server(&mut self, lang: &'static index::Lang) -> Option<&Sender<Req>> {
        if self.no_server.contains(lang.server) {
            return None;
        }
        if !self.servers.contains_key(lang.server) {
            let (rtx, rrx) = channel();
            let (mtx, mrx) = channel();
            let root = self.idx.root.clone();
            std::thread::spawn(move || serve(root, lang, rrx, mtx));
            self.servers.insert(lang.server, (rtx, mrx));
            self.status = format!("{}: starting", lang.server);
        }
        self.servers.get(lang.server).map(|(tx, _)| tx)
    }

    /// Send every pending language's files to its server; answers arrive through
    /// `poll_backend`. A language whose server cannot start keeps the tree-sitter answer.
    pub(super) fn start_backend(&mut self) {
        if !self.indexing.is_empty() {
            self.restart_backend = true;
            return;
        }
        for (lang, files) in self.idx.pending() {
            let n = files.len();
            match self.server(lang) {
                Some(tx) => {
                    let _ = tx.send(Req::Index(files));
                    self.indexing.insert(lang.server);
                    self.backend_progress = format!("{}: 0/{n}", lang.server);
                }
                None => self.idx.give_up(lang),
            }
        }
    }

    /// Merge whatever the server threads have answered since the last frame.
    pub(super) fn poll_backend(&mut self) {
        let mut msgs = Vec::new();
        for (_, rx) in self.servers.values() {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        let answered = !msgs.is_empty(); // any message at all means a server is past "starting"
        let mut files = Vec::new();
        let mut batch_ended = false;
        for m in msgs {
            match m {
                Msg::File(f) => files.push(f),
                Msg::Progress(p) => self.backend_progress = p,
                Msg::Failed(lang, e) => {
                    self.servers.remove(lang.server);
                    self.no_server.insert(lang.server);
                    self.indexing.remove(lang.server);
                    self.idx.give_up(lang);
                    self.hovers.clear();
                    self.asked = 0;
                    self.want_def = None;
                    self.status = format!("{e}: its files keep the tree-sitter resolver, no hover or go-to for them");
                    batch_ended = true;
                }
                Msg::Done(lang) => {
                    self.indexing.remove(lang.server);
                    batch_ended = true;
                }
                Msg::Hover(p, t) => {
                    self.asked = self.asked.saturating_sub(1);
                    self.hover_inflight = false;
                    self.hovers.insert(p, Some(t));
                }
                Msg::Def(p, d) => {
                    self.asked = self.asked.saturating_sub(1);
                    if self.want_def.as_ref().is_some_and(|(w, _)| *w == p) {
                        let (_, intent) = self.want_def.take().unwrap();
                        match d {
                            Some(d) => {
                                let found = match d.rel.as_deref().and_then(|r| self.idx.find_file(r)) {
                                    Some(fi) => match self.idx.by_line(&self.idx.files[fi].path, d.line).filter(|r| self.idx.sym(*r).start == d.line) {
                                        Some(r) => Peek::Sym(r),
                                        None => Peek::Line(fi, d.line),
                                    },
                                    None => Peek::Outside(d.abs, d.line, d.first, Rc::new(d.grid)),
                                };
                                self.land(found, intent);
                            }
                            None => self.status = "no definition found".into(),
                        }
                    }
                }
            }
        }
        // Every answer costs a map re-resolve and every drawn grid, so they are merged at most
        // once a second and once more when the batch ends; the re-link runs on a thread.
        self.merge_wait.append(&mut files);
        if !self.merge_wait.is_empty() && (batch_ended || self.last_merge.elapsed() >= Duration::from_secs(1)) {
            let files = std::mem::take(&mut self.merge_wait);
            self.with_index_change(|app| {
                for f in files {
                    app.idx.apply(f);
                }
            });
            self.start_link();
            self.last_merge = Instant::now();
            if self.status.ends_with(" symbols indexed") {
                self.status = self.indexed_status();
            }
        }
        if answered && self.status.ends_with(": starting") {
            self.status = self.indexed_status();
        }
        if batch_ended && self.indexing.is_empty() {
            self.idx.save_cache();
            self.backend_progress.clear();
            if std::mem::take(&mut self.restart_backend) {
                self.start_backend();
            }
        }
    }

    /// Once a second: pick up a map written by the CLI. Source changes are found by the watcher
    /// thread, so nothing here stats the tree; one it reports starts a rebuild of the index from
    /// disk and the cache on a thread, and the index in use is untouched until `poll_reindex`
    /// swaps the new one in.
    pub(super) fn poll_disk(&mut self) {
        if self.watch_rx.try_recv().is_ok() {
            let root = self.idx.root.clone();
            self.reindex_rx = Some(bg(move || index::build(&root)));
            self.status = "re-indexing (source changed)".into();
        }
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
                self.reload_map();
                self.map_mtime = mt;
                self.load_base();
                self.status = "map reloaded (changed on disk)".into();
            }
        }
    }

    /// Read `.codemap` again and keep as much of the reading position as the new map still
    /// supports: the path by name, the step when the step at that index is still the same lines
    /// of the same symbol, and a path's fold, collapse and context sets when its step count is
    /// unchanged. Paths are matched by name, so adding one does not shift another's state.
    pub(super) fn reload_map(&mut self) {
        let was: Vec<(String, usize)> = self.map.paths.iter().map(|p| (p.name.clone(), p.anchors.len())).collect();
        let path_name = self.sel_path.map(|pi| self.map.paths[pi].name.clone());
        let step = self.sel_anchor.zip(self.sel_path).map(|(ai, pi)| {
            let a = &self.map.paths[pi].anchors[ai];
            (ai, a.file.clone(), a.symbol.clone(), a.off_start, a.off_end)
        });
        self.map = Map::load(&self.map_path).unwrap_or_default();
        self.map.resolve_all(&self.idx);
        self.sel_path = path_name.and_then(|name| self.map.paths.iter().position(|p| p.name == name));
        self.sel_anchor = match (self.sel_path, step) {
            (Some(pi), Some((ai, file, symbol, os, oe))) => self.map.paths[pi]
                .anchors
                .get(ai)
                .filter(|a| a.file == file && a.symbol == symbol && (a.off_start, a.off_end) == (os, oe))
                .map(|_| ai),
            _ => None,
        };
        // old path index -> new one, for the paths whose steps cannot have moved
        let kept: HashMap<usize, usize> = was
            .iter()
            .enumerate()
            .filter_map(|(old, (name, n))| self.map.paths.iter().position(|p| p.name == *name && p.anchors.len() == *n).map(|new| (old, new)))
            .collect();
        self.remap_steps(|(pi, ai)| kept.get(&pi).map(|&p| (p, ai)));
    }

    /// Run `change` on the index and carry the selection, the listing and the map's anchors
    /// over by (file, symbol) identity, since symbol indices do not survive it.
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
        self.hovers.clear();
        self.grids.clear();
        self.map.resolve_all(&self.idx);
        self.focus = focus.and_then(|k| self.idx.by_key(&k));
        self.cur_file = cur_file.and_then(|p| self.idx.find_file(&p));
    }

    /// Give the watcher thread the indexed files' modification times so it starts looking.
    pub(super) fn watch_files(&self) {
        let _ = self.watch_tx.send(self.idx.files.iter().map(|f| (f.path.clone(), f.mtime)).collect());
    }

    /// Take the rebuilt index, then ask the servers about what changed and give the watcher
    /// thread the new modification times so it starts looking again.
    pub(super) fn poll_reindex(&mut self) {
        let Some(idx) = landed(&mut self.reindex_rx) else { return };
        match idx {
            Some(idx) => {
                self.with_index_change(|app| app.idx = idx);
                self.results.clear();
                self.status = format!("re-indexed: {} (source changed)", self.indexed_status());
            }
            None => self.status = "re-index failed; the index in use is the old one".into(), // the rebuild thread panicked
        }
        self.watch_files();
        self.start_backend();
    }

    /// Link the index on a thread over a text-free copy, since a link is a pass over every call
    /// site in the repo. One runs at a time; a change meanwhile queues another.
    pub(super) fn start_link(&mut self) {
        if self.link_rx.is_some() {
            self.relink = true;
            return;
        }
        let mut snap = self.idx.symbols_only();
        self.link_rx = Some(bg(move || {
            snap.link();
            snap
        }));
    }

    /// Take a finished link. A copy whose symbol tables no longer match the index is dropped:
    /// the change that made it stale queued the link that replaces it.
    pub(super) fn poll_link(&mut self) {
        let Some(done) = landed(&mut self.link_rx) else { return };
        if let Some(linked) = done {
            self.idx.take_edges(&linked);
        }
        if std::mem::take(&mut self.relink) {
            self.start_link();
        }
    }

    pub(super) fn indexed_status(&self) -> String {
        format!("{} files, {} symbols indexed", self.idx.files.len(), self.idx.files.iter().map(|f| f.symbols.len()).sum::<usize>())
    }

    /// Work in flight: a server request or batch of files, or the parent map, a re-index or a
    /// link on a thread.
    pub(super) fn working(&self) -> bool {
        self.asked > 0 || !self.indexing.is_empty() || self.base_rx.is_some() || self.reindex_rx.is_some() || self.link_rx.is_some()
    }
}
