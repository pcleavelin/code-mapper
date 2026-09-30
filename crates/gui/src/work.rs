use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

use cli::Failure;
use domain::{FileId, Index, Language, Line, LineCount, Location, Map, RelativePath, Root, Span};
use index::{FileVersion, Indexed, Parsers, ServerFile, Stamps, read_outside};
use io_lsp::StartError;
use io_map::{MapStore, MapText};
use io_vcs::Vcs;
use ui::{Count, Grid, Label};

use crate::app::App;
use crate::grid;
use crate::model::PathSlot;
use crate::model::{Dirty, Readable, StepKey, Warned};
use crate::peek::{Intent, Peek, Probe};
use crate::runtime::{self, Job, Landing, landed};
use crate::status::{IndexCounts, Status};
use std::mem;
use std::rc::Rc;

pub(crate) enum Request {
    Index(Vec<FileVersion>),
    Hover(Probe),
    Definition(Probe),
    References(Probe),
}

pub(crate) struct Found {
    file: Option<RelativePath>,
    path: PathBuf,
    line: Line,
    first: Line,
    grid: Grid,
}

pub(crate) enum Answer {
    File(ServerFile),
    Progress(Label),
    Failed(Language, StartError),
    Done(Language),
    Hover(Probe, Option<Label>),
    Definition(Probe, Option<Found>),
    References(Probe, Vec<Location>),
}

struct ServerLink {
    requests: Sender<Request>,
    answers: Receiver<Answer>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceChanged;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Repeat {
    Idle,
    Wanted,
}

pub(crate) struct WorkState {
    no_server: BTreeSet<Language>,
    started: BTreeSet<Language>,
    indexing: BTreeSet<Language>,
    progress: Label,
    restart: Repeat,
    merge_wait: Vec<ServerFile>,
    last_merge: Instant,
    relink: Repeat,
    base: Repeat,
}

impl Default for WorkState {
    fn default() -> Self {
        Self {
            no_server: BTreeSet::new(),
            started: BTreeSet::new(),
            indexing: BTreeSet::new(),
            progress: Label::default(),
            restart: Repeat::Idle,
            merge_wait: Vec::new(),
            last_merge: Instant::now(),
            relink: Repeat::Idle,
            base: Repeat::Idle,
        }
    }
}

pub(crate) struct Indexing<'work>(&'work BTreeSet<Language>);

impl fmt::Display for Indexing<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<String> = self
            .0
            .iter()
            .map(|language| format!("{:?}", language.program().as_str()))
            .collect();
        write!(formatter, "{{{}}}", names.join(", "))
    }
}

impl WorkState {
    pub(crate) fn no_server(&self, language: Language) -> bool {
        self.no_server.contains(&language)
    }

    pub(crate) fn started(&self, language: Language) -> bool {
        self.started.contains(&language)
    }

    pub(crate) const fn progress(&self) -> &Label {
        &self.progress
    }

    pub(crate) fn indexing(&self) -> Indexing<'_> {
        Indexing(&self.indexing)
    }

    pub(crate) fn is_indexing(&self) -> bool {
        !self.indexing.is_empty()
    }

    pub(crate) fn reading_base(&self) -> bool {
        self.base == Repeat::Wanted
    }

    pub(crate) fn unmerged(&self) -> Count {
        Count::new(self.merge_wait.len())
    }
}

pub(crate) struct Services {
    root: Root,
    servers: BTreeMap<Language, ServerLink>,
    link: Option<Job<Index>>,
    reindex: Option<Job<Indexed>>,
    base: Option<Job<Result<Map, Label>>>,
    watch: Sender<Stamps>,
    changed: Receiver<SourceChanged>,
    stamps: Stamps,
}

impl Services {
    pub(crate) fn new(root: &Root) -> Self {
        let (watch, lists) = channel();
        let (noticed, changed) = channel();
        let watched = root.clone();
        runtime::service(move || watch_sources(&watched, &lists, &noticed));
        Self {
            root: root.clone(),
            servers: BTreeMap::new(),
            link: None,
            reindex: None,
            base: None,
            watch,
            changed,
            stamps: Stamps::default(),
        }
    }

    pub(crate) const fn reindexing(&self) -> bool {
        self.reindex.is_some()
    }

    pub(crate) const fn linking(&self) -> bool {
        self.link.is_some()
    }

    pub(crate) fn watch(&mut self, stamps: Stamps) {
        drop(self.watch.send(stamps.clone()));
        self.stamps = stamps;
    }

    fn watch_stamps(&self) {
        drop(self.watch.send(self.stamps.clone()));
    }
}

fn watch_sources(root: &Root, lists: &Receiver<Stamps>, changed: &Sender<SourceChanged>) {
    while let Ok(stamps) = lists.recv() {
        while !stamps.changed(root) {
            thread::sleep(Duration::from_secs(1));
        }
        if changed.send(SourceChanged).is_err() {
            return;
        }
    }
}

const BATCH: Count = Count::new(8);
const CONTEXT_ABOVE: LineCount = LineCount::new(6);
const CONTEXT_BELOW: LineCount = LineCount::new(24);

fn definition_found(session: &mut io_lsp::LspSession, probe: &Probe) -> Option<Found> {
    let definition = session.definition(&probe.position)?;
    let contents = read_outside(&definition.path);
    let source = Parsers::default().parse(
        RelativePath::new(&definition.path.to_string_lossy()),
        &contents,
    );
    let line = definition.line;
    let first = Line::new(line.value().saturating_sub(CONTEXT_ABOVE.value()));
    let last = source.text().count().last();
    let span = last.and_then(|last| {
        Span::new(
            first,
            Line::new(line.value().saturating_add(CONTEXT_BELOW.value())).min(last),
        )
    });
    let grid = span.map_or_else(
        || Grid::new(Count::ZERO, Count::ZERO),
        |span| grid::build(&source, span),
    );
    Some(Found {
        file: definition.file,
        path: definition.path,
        line,
        first,
        grid,
    })
}

fn serve(root: &Root, language: Language, requests: &Receiver<Request>, answers: &Sender<Answer>) {
    let mut session = match index::start_session(root, language) {
        Ok(session) => session,
        Err(error) => {
            drop(answers.send(Answer::Failed(language, error)));
            return;
        }
    };
    let program = language.program();
    let mut queue: VecDeque<FileVersion> = VecDeque::new();
    let (mut total, mut done) = (0_usize, 0_usize);
    loop {
        let request = match requests.try_recv() {
            Ok(request) => Some(request),
            Err(TryRecvError::Empty) if queue.is_empty() => match requests.recv() {
                Ok(request) => Some(request),
                Err(_) => return,
            },
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => return,
        };
        match request {
            Some(Request::Index(files)) => {
                if queue.is_empty() {
                    (total, done) = (0, 0);
                }
                total += files.len();
                queue.extend(files);
            }
            Some(Request::Hover(probe)) => {
                let text = session
                    .hover(&probe.position)
                    .map(|text| Label::new(text.as_str()));
                drop(answers.send(Answer::Hover(probe, text)));
            }
            Some(Request::Definition(probe)) => {
                let found = definition_found(&mut session, &probe);
                drop(answers.send(Answer::Definition(probe, found)));
            }
            Some(Request::References(probe)) => {
                let found = session.references(&probe.position);
                drop(answers.send(Answer::References(probe, found)));
            }
            None => {
                let batch: Vec<FileVersion> =
                    (0..BATCH.get()).map_while(|_| queue.pop_front()).collect();
                if batch.is_empty() {
                    continue;
                }
                let answered = index::index_files(&mut session, &batch);
                done += batch.len();
                let progress = Label::new(format!("{program}: {done}/{total}"));
                drop(answers.send(Answer::Progress(progress)));
                for file in answered {
                    drop(answers.send(Answer::File(file)));
                }
                if queue.is_empty() {
                    drop(answers.send(Answer::Done(language)));
                }
            }
        }
    }
}

fn base_map(root: &Root) -> Result<Map, Label> {
    let failure = |failure: Failure| Label::new(failure.to_string());
    let Ok(vcs) = Vcs::detect(root) else {
        return Err(failure(Failure::NoRepository));
    };
    let revision = vcs.parent();
    let directory = MapStore::relative_directory();
    let Ok(text) = vcs.map_at(&revision, &directory) else {
        return Err(failure(Failure::NoMapAt {
            directory,
            revision,
            program: vcs.program(),
        }));
    };
    MapStore::base(&MapText::new(text.as_str()), &revision)
        .map_err(|error| failure(Failure::Parse(error)))
}

impl App {
    pub(crate) fn index_counts(&self) -> IndexCounts {
        let index = &self.model.index;
        IndexCounts {
            files: Count::new(index.files().count()),
            symbols: Count::new(index.files().map(|file| file.symbols().count()).sum()),
        }
    }

    pub(crate) fn indexed_status(&self) -> Status {
        Status::Indexed(self.index_counts())
    }

    pub(crate) fn working(&self) -> bool {
        self.model.queries.asked().get() > 0
            || self.model.work.is_indexing()
            || self.services.base.is_some()
            || self.services.reindex.is_some()
            || self.services.link.is_some()
    }

    pub(crate) fn load_base(&mut self) {
        let root = self.services.root.clone();
        self.services.base = Some(Job::start(move || base_map(&root)));
        self.model.work.base = Repeat::Wanted;
    }

    pub(crate) fn server(&mut self, language: Language) -> Option<&Sender<Request>> {
        if self.model.work.no_server(language) {
            return None;
        }
        if !self.services.servers.contains_key(&language) {
            let (requests, incoming) = channel();
            let (outgoing, answers) = channel();
            let root = self.services.root.clone();
            runtime::service(move || serve(&root, language, &incoming, &outgoing));
            self.services
                .servers
                .insert(language, ServerLink { requests, answers });
            self.model.work.started.insert(language);
            self.model.status = Status::Starting(language.program());
        }
        self.services
            .servers
            .get(&language)
            .map(|link| &link.requests)
    }

    pub(crate) fn ask(&mut self, language: Language, request: Request) -> bool {
        let Some(requests) = self.server(language) else {
            return false;
        };
        drop(requests.send(request));
        true
    }

    pub(crate) fn start_backend(&mut self) {
        if self.model.work.is_indexing() {
            self.model.work.restart = Repeat::Wanted;
            return;
        }
        for batch in self.model.index.pending() {
            let files = index::versions(&self.model.index, &batch.files);
            let count = files.len();
            let program = batch.language.program();
            if self.ask(batch.language, Request::Index(files)) {
                self.model.work.indexing.insert(batch.language);
                self.model.work.progress = Label::new(format!("{program}: 0/{count}"));
            } else {
                self.model.index.give_up(batch.language);
            }
        }
    }

    pub(crate) fn poll_backend(&mut self) {
        let mut answers = Vec::new();
        for link in self.services.servers.values() {
            while let Ok(answer) = link.answers.try_recv() {
                answers.push(answer);
            }
        }
        let answered = !answers.is_empty();
        let mut files = Vec::new();
        let mut batch_ended = false;
        for answer in answers {
            match answer {
                Answer::File(file) => files.push(file),
                Answer::Progress(progress) => self.model.work.progress = progress,
                Answer::Failed(language, error) => {
                    self.services.servers.remove(&language);
                    let work = &mut self.model.work;
                    work.started.remove(&language);
                    work.no_server.insert(language);
                    work.indexing.remove(&language);
                    self.model.index.give_up(language);
                    self.model.queries.server_gone();
                    self.model.status =
                        Status::ServerFailed(Label::new(cli::start_error(&error).as_str()));
                    batch_ended = true;
                }
                Answer::Done(language) => {
                    self.model.work.indexing.remove(&language);
                    batch_ended = true;
                }
                Answer::Hover(probe, text) => self.model.queries.hover_answered(probe, text),
                Answer::References(probe, found) => {
                    self.model.queries.references_answered();
                    self.references_landed(&probe, found);
                }
                Answer::Definition(probe, found) => {
                    if let Some(intent) = self.model.queries.definition_answered(&probe) {
                        self.definition_landed(found, intent);
                    }
                }
            }
        }
        self.model.work.merge_wait.append(&mut files);
        let due = self.model.work.last_merge.elapsed() >= Duration::from_secs(1);
        if !self.model.work.merge_wait.is_empty() && (batch_ended || due) {
            let merged = mem::take(&mut self.model.work.merge_wait);
            self.with_index_change(|app| {
                for file in merged {
                    index::apply(&mut app.model.index, file);
                }
            });
            self.start_link();
            self.model.work.last_merge = Instant::now();
            if self.model.status.is_indexed() {
                self.model.status = self.indexed_status();
            }
        }
        if answered && self.model.status.is_starting() {
            self.model.status = self.indexed_status();
        }
        if batch_ended && !self.model.work.is_indexing() {
            drop(index::save_cache(&self.model.index));
            self.model.work.progress = Label::default();
            if mem::replace(&mut self.model.work.restart, Repeat::Idle) == Repeat::Wanted {
                self.start_backend();
            }
        }
    }

    fn references_landed(&mut self, probe: &Probe, found: Vec<Location>) {
        let index = &mut self.model.index;
        let Some(file) = index.find_file(&probe.position.file).filter(|file| {
            index
                .file(*file)
                .is_some_and(|source| source.hash() == probe.hash)
        }) else {
            return;
        };
        if let Some(symbol) = index.file_mut(file).and_then(|source| {
            source
                .symbols_mut()
                .find(|symbol| symbol.span().start() == probe.position.line)
        }) {
            symbol.set_references(found);
        }
    }

    fn definition_landed(&mut self, found: Option<Found>, intent: Intent) {
        let Some(found) = found else {
            self.model.status = Status::NoDefinition;
            return;
        };
        let index = &self.model.index;
        let peek = match found.file.as_ref().and_then(|file| index.find_file(file)) {
            Some(file) => self.peek_at_line(file, found.line),
            None => Peek::Outside {
                file: found.path,
                line: found.line,
                first: found.first,
                grid: Rc::new(found.grid),
            },
        };
        self.land(peek, intent);
    }

    fn peek_at_line(&self, file: FileId, line: Line) -> Peek {
        let index = &self.model.index;
        let path = index.file(file).map(|source| source.path().clone());
        path.and_then(|path| index.by_line(&path, line))
            .filter(|symbol| {
                index
                    .symbol(*symbol)
                    .is_some_and(|found| found.span().start() == line)
            })
            .map_or(Peek::Line { file, line }, Peek::Symbol)
    }

    pub(crate) fn land(&mut self, found: Peek, intent: Intent) {
        match (intent, found) {
            (Intent::Jump, Peek::Symbol(symbol)) => self.model.jumped_to_symbol(symbol),
            (Intent::Jump, Peek::Line { file, line }) => self.model.open_line(file, line),
            (_, found) => {
                if let Peek::Outside { file, line, .. } = &found {
                    self.model.status = Status::DefinedOutside {
                        file: file.clone(),
                        line: *line,
                    };
                }
                self.model.peek = Some(found);
            }
        }
    }

    pub(crate) fn poll_base(&mut self) {
        let (map, why) = match landed(&mut self.services.base) {
            Landing::Waiting => return,
            Landing::Landed(Ok(map)) => (Some(map), Label::default()),
            Landing::Landed(Err(why)) => (None, why),
            Landing::Failed => (
                None,
                Label::new("the thread reading the parent revision's map failed"),
            ),
        };
        self.model.base.map = map;
        self.model.base.why = why;
        self.model.work.base = Repeat::Idle;
    }

    pub(crate) fn poll_disk(&mut self) {
        if self.services.changed.try_recv().is_ok() {
            let root = self.services.root.clone();
            self.services.reindex = Some(Job::start(move || index::build(&root)));
            self.model.status = Status::Reindexing;
        }
        let disk = &mut self.model.disk;
        if disk.last_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        disk.last_poll = Instant::now();
        let stamp = self.model.store.stamp();
        if stamp == self.model.disk.stamp {
            return;
        }
        if self.model.disk.dirty == Dirty::Unsaved {
            if self.model.disk.warned == Warned::Quiet {
                self.model.status = Status::DiskChanged;
                self.model.disk.warned = Warned::Warned;
            }
            return;
        }
        self.model.disk.stamp = stamp;
        if let Err(error) = self.reload_map() {
            self.model.disk.readable = Readable::Broken;
            self.model.status = Status::MapUnreadableKept(error);
            return;
        }
        self.model.disk.readable = Readable::Reads;
        self.load_base();
        self.model.status = Status::MapReloaded;
    }

    fn reload_map(&mut self) -> Result<(), Label> {
        let model = &mut self.model;
        let before: Vec<(domain::PathName, usize)> = model
            .map
            .paths()
            .iter()
            .map(|path| (path.name().clone(), path.steps().len()))
            .collect();
        let path_name = model
            .nav
            .path()
            .and_then(|path| model.path(path))
            .map(|path| path.name().clone());
        let step = model.nav.step_key().and_then(|key| {
            model
                .step(key)
                .map(|step| (key.step, step.anchor().clone()))
        });
        let loaded = model
            .store
            .load()
            .map_err(|error| Label::new(Failure::Load(error).to_string()))?;
        model.map = loaded;
        model.map.resolve_all(&model.index);
        let path = path_name.and_then(|name| model.find_path(&name));
        let step = match (path, step) {
            (Some(path), Some((slot, anchor))) => model
                .step(StepKey { path, step: slot })
                .filter(|found| {
                    let now = found.anchor();
                    now.file() == anchor.file()
                        && now.symbol() == anchor.symbol()
                        && now.start() == anchor.start()
                        && now.end() == anchor.end()
                })
                .map(|_| slot),
            _ => None,
        };
        model.reselect(path, step);
        let kept: BTreeMap<usize, usize> = before
            .iter()
            .enumerate()
            .filter_map(|(old, (name, count))| {
                model
                    .map
                    .paths()
                    .iter()
                    .position(|found| found.name() == name && found.steps().len() == *count)
                    .map(|new| (old, new))
            })
            .collect();
        model.views.remap(|key| {
            kept.get(&key.path.get()).map(|new| StepKey {
                path: PathSlot::new(*new),
                step: key.step,
            })
        });
        Ok(())
    }

    pub(crate) fn with_index_change(&mut self, change: impl FnOnce(&mut Self)) {
        let before = &self.model;
        let index = &before.index;
        let focus_key = before
            .nav
            .focus()
            .and_then(|symbol| index.symbol_key(symbol));
        let open_path = before
            .nav
            .file()
            .and_then(|open| index.file(open))
            .map(|open| open.path().clone());
        let peek_symbol = match &before.peek {
            Some(Peek::Symbol(symbol)) => index.symbol_key(*symbol),
            _ => None,
        };
        let peek_line = match &before.peek {
            Some(Peek::Line { file, line }) => index
                .file(*file)
                .map(|source| (source.path().clone(), *line)),
            _ => None,
        };
        let graph = self.model.graph.save(&self.model.index);
        change(self);
        let model = &mut self.model;
        model.graph.restore(&model.index, &graph);
        if let Some(key) = peek_symbol {
            model.peek = model.index.by_key(&key).map(Peek::Symbol);
        } else if let Some((path, line)) = peek_line {
            model.peek = model
                .index
                .find_file(&path)
                .map(|found| Peek::Line { file: found, line });
        }
        model.queries.forget_hovers();
        self.grids.clear();
        model.map.resolve_all(&model.index);
        let focus = focus_key.and_then(|key| model.index.by_key(&key));
        let file = open_path.and_then(|path| model.index.find_file(&path));
        model.refocus(focus, file);
    }

    pub(crate) fn poll_reindex(&mut self) {
        match landed(&mut self.services.reindex) {
            Landing::Waiting => return,
            Landing::Landed(indexed) => {
                let Indexed { index, stamps } = indexed;
                self.with_index_change(|app| app.model.index = index);
                self.model.results.clear();
                self.model.status = Status::Reindexed(self.index_counts());
                self.services.watch(stamps);
            }
            Landing::Failed => {
                self.model.status = Status::ReindexFailed;
                self.services.watch_stamps();
            }
        }
        self.start_backend();
    }

    pub(crate) fn start_link(&mut self) {
        if self.services.link.is_some() {
            self.model.work.relink = Repeat::Wanted;
            return;
        }
        let mut snapshot = self.model.index.symbols_only();
        self.services.link = Some(Job::start(move || {
            index::link(&mut snapshot);
            snapshot
        }));
    }

    pub(crate) fn poll_link(&mut self) {
        match landed(&mut self.services.link) {
            Landing::Waiting => return,
            Landing::Landed(linked) => self.model.index.take_edges(&linked),
            Landing::Failed => {}
        }
        if mem::replace(&mut self.model.work.relink, Repeat::Idle) == Repeat::Wanted {
            self.start_link();
        }
    }
}
