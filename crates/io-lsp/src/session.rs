use std::io::{BufReader, Write};
use std::path::{self, Path, PathBuf};
use std::process::{self, Child, ChildStdin, ChildStdout};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::{Duration, Instant};

use domain::{Language, Location, RelativePath};
use io_process::{Argument, Process, ProcessError, Program};
use serde_json::Value;

use crate::answer::{CallItem, Definition, DocumentPosition, HoverText, Outline, StartError};
use crate::convert::{self, Uri};
use crate::wire::{self, LocationShape, Notice, Outgoing, ProgressKind};

const ANSWER_WAIT: Duration = Duration::from_secs(120);
const READY_POLL: Duration = Duration::from_millis(200);
const QUIET_AFTER: Duration = Duration::from_millis(500);
const SILENT_START: Duration = Duration::from_secs(3);
const IN_FLIGHT: InFlight = InFlight(64);

#[derive(Clone, Copy, Debug)]
struct InFlight(usize);

impl InFlight {
    fn allows(self, sent: usize, answered: usize) -> bool {
        sent - answered < self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RequestId(u64);

impl RequestId {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }

    fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct OpenProgress(usize);

impl OpenProgress {
    fn begun(self) -> Self {
        Self(self.0 + 1)
    }

    fn ended(self) -> Self {
        Self(self.0.saturating_sub(1))
    }

    fn is_open(self) -> bool {
        self.0 > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerStatus {
    Unreported,
    Busy,
    Quiescent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unanswered {
    Gone,
    Refused,
}

#[derive(Debug)]
pub struct LspSession {
    child: Child,
    input: ChildStdin,
    messages: Receiver<Value>,
    last_id: RequestId,
    progress: OpenProgress,
    last_progress: Instant,
    status: ServerStatus,
    root: PathBuf,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the language server's stdout is read on its own thread so a request can time out"
)]
fn listen(output: ChildStdout) -> Receiver<Value> {
    let (sender, receiver) = channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(output);
        while let Some(message) = wire::read_message(&mut reader) {
            if sender.send(message).is_err() {
                break;
            }
        }
    });
    receiver
}

impl LspSession {
    pub fn start(language: Language, root: &Path) -> Result<Self, StartError> {
        let failed = || StartError::Failed(language.program());
        let root = path::absolute(root).ok().ok_or_else(failed)?;
        let program = Program::new(language.program().as_str());
        let arguments: Vec<Argument> = language
            .arguments()
            .iter()
            .map(|argument| Argument::new(argument.as_str()))
            .collect();
        let spawned = Process::spawn(&program, &arguments, &root).map_err(|error| match error {
            ProcessError::Missing(_) => StartError::Missing(language.program()),
            ProcessError::Start { .. } | ProcessError::NoPipe(_) => failed(),
        })?;
        let mut session = Self {
            child: spawned.child,
            input: spawned.input,
            messages: listen(spawned.output),
            last_id: RequestId::default(),
            progress: OpenProgress::default(),
            last_progress: Instant::now(),
            status: ServerStatus::Unreported,
            root,
        };
        let uri = Uri::of(&session.root);
        let name = session
            .root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        session
            .request(Outgoing::initialize(process::id(), uri.as_str(), &name))
            .ok()
            .ok_or_else(failed)?;
        session.notify(Outgoing::initialized());
        Ok(session)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn send(&mut self, message: &Value) {
        drop(self.input.write_all(&wire::frame(message)));
        drop(self.input.flush());
    }

    fn notify(&mut self, outgoing: Outgoing) {
        self.send(&outgoing.notification());
    }

    fn reply(&mut self, id: &Value, result: &Value) {
        self.send(&wire::reply(id, result));
    }

    fn issue(&mut self, outgoing: Outgoing) -> RequestId {
        self.last_id = self.last_id.next();
        let id = self.last_id;
        self.send(&outgoing.request(id.value()));
        id
    }

    fn request(&mut self, outgoing: Outgoing) -> Result<Value, Unanswered> {
        let id = self.issue(outgoing);
        loop {
            let message = self
                .messages
                .recv_timeout(ANSWER_WAIT)
                .ok()
                .ok_or(Unanswered::Gone)?;
            if wire::answer_id(&message) == Some(id.value()) {
                return wire::outcome(message).ok().ok_or(Unanswered::Refused);
            }
            self.handle(&message);
        }
    }

    fn request_all(&mut self, requests: Vec<Outgoing>) -> Vec<Result<Value, Unanswered>> {
        let count = requests.len();
        let first = self.last_id.next().value();
        let mut answers: Vec<Option<Result<Value, Unanswered>>> =
            (0..count).map(|_| None).collect();
        let mut requests = requests.into_iter();
        let (mut sent, mut answered) = (0, 0);
        while answered < count {
            while sent < count && IN_FLIGHT.allows(sent, answered) {
                let Some(outgoing) = requests.next() else {
                    break;
                };
                self.issue(outgoing);
                sent += 1;
            }
            let Ok(message) = self.messages.recv_timeout(ANSWER_WAIT) else {
                break;
            };
            let slot = wire::answer_id(&message)
                .and_then(|id| id.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .and_then(|offset| answers.get_mut(offset))
                .filter(|slot| slot.is_none());
            if let Some(slot) = slot {
                *slot = Some(wire::outcome(message).ok().ok_or(Unanswered::Refused));
                answered += 1;
                continue;
            }
            self.handle(&message);
        }
        answers
            .into_iter()
            .map(|answer| answer.unwrap_or(Err(Unanswered::Gone)))
            .collect()
    }

    fn handle(&mut self, message: &Value) {
        match wire::notice(message) {
            Notice::Progress(kind) => {
                match kind {
                    ProgressKind::Begin => self.progress = self.progress.begun(),
                    ProgressKind::End => self.progress = self.progress.ended(),
                    ProgressKind::Other => {}
                }
                self.last_progress = Instant::now();
            }
            Notice::Status(quiescent) => {
                self.status = match quiescent {
                    None => ServerStatus::Unreported,
                    Some(true) => ServerStatus::Quiescent,
                    Some(false) => ServerStatus::Busy,
                };
            }
            Notice::Configuration { id, count } => self.reply(&id, &wire::nulls(count)),
            Notice::Request { id } => self.reply(&id, &Value::Null),
            Notice::Ignored => {}
        }
    }

    pub fn wait_ready(&mut self, longest: Duration) {
        let start = Instant::now();
        let mut seen_progress = false;
        loop {
            match self.messages.recv_timeout(READY_POLL) {
                Ok(message) => self.handle(&message),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            seen_progress |= self.progress.is_open();
            let quiet = !self.progress.is_open() && self.last_progress.elapsed() > QUIET_AFTER;
            let done = match self.status {
                ServerStatus::Quiescent => quiet,
                ServerStatus::Busy => false,
                ServerStatus::Unreported => {
                    quiet && (seen_progress || start.elapsed() > SILENT_START)
                }
            };
            if done || start.elapsed() > longest {
                return;
            }
        }
    }

    fn uri(&self, file: &RelativePath) -> Uri {
        Uri::of(&self.root.join(file.as_str()))
    }

    pub fn outlines(&mut self, files: &[RelativePath]) -> Vec<Vec<Outline>> {
        let requests = files
            .iter()
            .map(|file| Outgoing::document_symbol(self.uri(file).as_str()))
            .collect();
        self.request_all(requests)
            .into_iter()
            .map(|answer| {
                wire::symbols(&answer.unwrap_or(Value::Null))
                    .into_iter()
                    .map(convert::outline)
                    .collect()
            })
            .collect()
    }

    pub fn call_items(&mut self, positions: &[DocumentPosition]) -> Vec<Vec<CallItem>> {
        let requests = positions
            .iter()
            .map(|position| {
                Outgoing::prepare_call_hierarchy(
                    self.uri(&position.file).as_str(),
                    position.line.value(),
                    position.character.value(),
                )
            })
            .collect();
        self.request_all(requests)
            .into_iter()
            .map(|answer| {
                wire::items(&answer.unwrap_or(Value::Null))
                    .into_iter()
                    .map(CallItem::new)
                    .collect()
            })
            .collect()
    }

    fn located(&self, answer: Result<Value, Unanswered>, shape: &LocationShape) -> Vec<Location> {
        convert::locations(
            wire::locations(&answer.unwrap_or(Value::Null), shape),
            &self.root,
        )
    }

    pub fn outgoing_targets(&mut self, items: &[CallItem]) -> Vec<Vec<Location>> {
        let requests = items
            .iter()
            .map(|item| Outgoing::outgoing_calls(item.as_value()))
            .collect();
        self.request_all(requests)
            .into_iter()
            .map(|answer| self.located(answer, &LocationShape::Outgoing))
            .collect()
    }

    pub fn references(&mut self, position: &DocumentPosition) -> Vec<Location> {
        let answer = self.request(Outgoing::references(
            self.uri(&position.file).as_str(),
            position.line.value(),
            position.character.value(),
        ));
        self.located(answer, &LocationShape::Reference)
    }

    pub fn incoming_callers(&mut self, position: &DocumentPosition) -> Vec<Location> {
        let items = self
            .request(Outgoing::prepare_call_hierarchy(
                self.uri(&position.file).as_str(),
                position.line.value(),
                position.character.value(),
            ))
            .unwrap_or(Value::Null);
        let requests = wire::items(&items)
            .iter()
            .map(Outgoing::incoming_calls)
            .collect();
        let mut callers: Vec<Location> = self
            .request_all(requests)
            .into_iter()
            .flat_map(|answer| self.located(answer, &LocationShape::Incoming))
            .collect();
        callers.sort();
        callers.dedup();
        callers
    }

    pub fn hover(&mut self, position: &DocumentPosition) -> Option<HoverText> {
        let answer = self
            .request(Outgoing::hover(
                self.uri(&position.file).as_str(),
                position.line.value(),
                position.character.value(),
            ))
            .ok()?;
        HoverText::from_parts(&wire::hover_parts(&answer))
    }

    pub fn definition(&mut self, position: &DocumentPosition) -> Option<Definition> {
        let answer = self
            .request(Outgoing::definition(
                self.uri(&position.file).as_str(),
                position.line.value(),
                position.character.value(),
            ))
            .ok()?;
        convert::definition(&wire::definition(&answer)?, &self.root)
    }

    pub fn shutdown(mut self) {
        drop(self.request(Outgoing::shutdown()));
        self.notify(Outgoing::exit());
        drop(self.child.wait());
    }
}

impl Drop for LspSession {
    fn drop(&mut self) {
        drop(self.child.kill());
    }
}
