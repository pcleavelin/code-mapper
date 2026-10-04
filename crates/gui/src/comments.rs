use std::fmt;
use std::time::{Duration, Instant};

use domain::{
    Author, Comment, CommentId, CommentTarget, CommentText, Comments, FileId, Index, Line, Span,
    StepAddress, Tour, TourName,
};
use io_comments::{CommentSaveError, CommentStamp, CommentStore};
use ui::{Count, Label};

use crate::app::App;
use crate::field::Which;
use crate::model::{Gone, Model, StepKey, Tab};
use crate::nav::Scrolling;
use crate::status::{Status, Tone};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DraftOn {
    Tour(TourName),
    Step(StepAddress),
    Lines { file: FileId, span: Span },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommentAct {
    Start(DraftOn),
    Submit,
    Cancel,
    Dismiss(CommentId),
    Next,
    ClosePopup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommentCounts {
    pub(crate) total: Count,
    pub(crate) answered: Count,
}

impl Gone {
    pub(crate) fn of(model: &Model, comment: &Comment) -> Self {
        match comment.target() {
            CommentTarget::Tour(name) if model.map.tour(name).is_none() => Self::Tour,
            CommentTarget::Tour(_) => Self::Nothing,
            CommentTarget::Step(address) => match model.map.tour(&address.tour) {
                None => Self::Tour,
                Some(tour) if tour.step(&address.step).is_none() => Self::Step,
                Some(_) => Self::Nothing,
            },
            CommentTarget::Code(anchor) => {
                match (model.index.find_file(anchor.file()), comment.resolution()) {
                    (None, _) => Self::File,
                    (Some(_), None) if anchor.symbol().is_some() => Self::Symbol,
                    (Some(_), None) => Self::Lines,
                    (Some(_), Some(_)) => Self::Nothing,
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum InsetAt {
    Top,
    After(Line),
}

impl InsetAt {
    pub(crate) fn of(comment: &Comment) -> Self {
        comment
            .resolution()
            .map_or(Self::Top, |resolution| Self::After(resolution.span.end()))
    }
}

pub(crate) struct CommentShelf {
    all: Comments,
    stamp: Option<CommentStamp>,
    last_poll: Instant,
    pub(crate) draft: Option<DraftOn>,
    walked: Option<CommentId>,
    popup: Option<CommentId>,
}

impl CommentShelf {
    pub(crate) fn load(store: &CommentStore, index: &Index) -> Self {
        let mut all = store.load().unwrap_or_default();
        all.resolve_all(index);
        Self {
            all,
            stamp: store.stamp(),
            last_poll: Instant::now(),
            draft: None,
            walked: None,
            popup: None,
        }
    }

    pub(crate) fn resolve_all(&mut self, index: &Index) {
        self.all.resolve_all(index);
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Comment> {
        self.all.iter()
    }

    pub(crate) fn on_tour<'a>(&'a self, name: &'a TourName) -> impl Iterator<Item = &'a Comment> {
        self.all.on_tour(name)
    }

    pub(crate) fn on_gone_steps<'a>(&'a self, tour: &'a Tour) -> impl Iterator<Item = &'a Comment> {
        self.all.on_gone_steps(tour)
    }

    pub(crate) fn on_step<'a>(
        &'a self,
        address: &'a StepAddress,
    ) -> impl Iterator<Item = &'a Comment> {
        self.all.on_step(address)
    }

    pub(crate) fn in_file<'a>(
        &'a self,
        index: &'a Index,
        file: FileId,
    ) -> impl Iterator<Item = &'a Comment> {
        let path = index.file(file).map(|source| source.path().clone());
        self.all
            .iter()
            .filter(move |comment| comment.target().file() == path.as_ref())
    }

    pub(crate) fn counts(&self) -> CommentCounts {
        CommentCounts {
            total: Count::new(self.all.iter().count()),
            answered: Count::new(self.all.iter().filter(|comment| !comment.is_open()).count()),
        }
    }

    pub(crate) fn drafting(&self, on: &DraftOn) -> bool {
        self.draft.as_ref() == Some(on)
    }

    pub(crate) fn walked(&self) -> Option<&CommentId> {
        self.walked.as_ref()
    }

    pub(crate) fn popup(&self) -> Option<&Comment> {
        self.popup.as_ref().and_then(|id| self.all.get(id))
    }
}

fn save_error(error: &CommentSaveError) -> Label {
    Label::new(match error {
        CommentSaveError::CreateDirectory { error, .. }
        | CommentSaveError::Write { error, .. }
        | CommentSaveError::Remove { error, .. } => error.to_string(),
    })
}

impl App {
    pub(crate) fn comment(&mut self, act: CommentAct) {
        match act {
            CommentAct::Start(on) => {
                let model = &mut self.model;
                model.shelf.draft = Some(on);
                model.fields.start_empty(Which::Comment);
            }
            CommentAct::Submit => self.add_comment(),
            CommentAct::Cancel => self.cancel_comment(),
            CommentAct::Dismiss(id) => self.dismiss_comment(&id),
            CommentAct::Next => self.model.next_comment(),
            CommentAct::ClosePopup => self.model.shelf.popup = None,
        }
    }

    pub(crate) fn cancel_comment(&mut self) {
        let model = &mut self.model;
        model.shelf.draft = None;
        model.fields.release(Which::Comment);
    }

    fn add_comment(&mut self) {
        let model = &mut self.model;
        let Some(on) = model.shelf.draft.clone() else {
            return;
        };
        let Some(text) = CommentText::new(model.fields.get(Which::Comment).text().as_str().trim())
        else {
            model.status = Status::Comment(CommentStatus::Empty);
            return;
        };
        let target = match on {
            DraftOn::Tour(name) => CommentTarget::Tour(name),
            DraftOn::Step(address) => CommentTarget::Step(address),
            DraftOn::Lines { file, span } => match CommentTarget::code(&model.index, file, span) {
                Ok(target) => target,
                Err(error) => {
                    model.status = Status::refused(&model.map, error);
                    return;
                }
            },
        };
        let id = model
            .shelf
            .all
            .add(&model.index, target, Author::Human, text);
        let written = model
            .shelf
            .all
            .get(&id)
            .map(|comment| model.comments.write(comment));
        if let Some(Err(error)) = written {
            drop(model.shelf.all.dismiss(&id));
            model.status = Status::Comment(CommentStatus::NotSaved(save_error(&error)));
            return;
        }
        model.shelf.stamp = model.comments.stamp();
        model.shelf.draft = None;
        model.fields.start_empty(Which::Comment);
        model.fields.release(Which::Comment);
        model.status = Status::Comment(CommentStatus::Added(id));
    }

    fn dismiss_comment(&mut self, id: &CommentId) {
        let model = &mut self.model;
        if let Err(error) = model.comments.remove(id) {
            model.status = Status::Comment(CommentStatus::NotSaved(save_error(&error)));
            return;
        }
        let Ok(removed) = model.shelf.all.dismiss(id) else {
            return;
        };
        if model.shelf.popup.as_ref() == Some(id) {
            model.shelf.popup = None;
        }
        model.shelf.stamp = model.comments.stamp();
        model.status = if removed.is_open() {
            Status::Comment(CommentStatus::Withdrawn(id.clone()))
        } else {
            Status::Comment(CommentStatus::Dismissed(id.clone()))
        };
    }

    pub(crate) fn poll_comments(&mut self) {
        let model = &mut self.model;
        if model.shelf.last_poll.elapsed() < Duration::from_secs(1) {
            return;
        }
        model.shelf.last_poll = Instant::now();
        let stamp = model.comments.stamp();
        if stamp == model.shelf.stamp {
            return;
        }
        model.shelf.stamp = stamp;
        let Ok(mut loaded) = model.comments.load() else {
            return;
        };
        loaded.resolve_all(&model.index);
        let before = model.shelf.counts().answered;
        model.shelf.all = loaded;
        let after = model.shelf.counts().answered;
        if after > before {
            model.status = Status::Comment(CommentStatus::RepliesArrived(Count::new(
                after.get() - before.get(),
            )));
        }
    }
}

impl Model {
    pub(crate) fn next_comment(&mut self) {
        let Some(found) = self
            .shelf
            .all
            .next_after(self.shelf.walked.as_ref())
            .cloned()
        else {
            self.status = Status::Comment(CommentStatus::NoComments);
            return;
        };
        let id = found.id().clone();
        let gone = Gone::of(self, &found);
        self.shelf.walked = Some(id.clone());
        self.shelf.popup = gone.has_no_view().then(|| id.clone());
        match found.target() {
            CommentTarget::Tour(name) => {
                if let Some(tour) = self.find_tour(name) {
                    self.open_tour(tour, Tab::Tour);
                }
            }
            CommentTarget::Step(address) => {
                let tour = self.find_tour(&address.tour);
                let step = tour.and_then(|tour| self.step_slot(tour, &address.step));
                match (tour, step) {
                    (Some(tour), Some(step)) => {
                        self.open_step(StepKey { tour, step }, Scrolling::Scroll);
                    }
                    (Some(tour), None) => self.open_tour(tour, Tab::Tour),
                    (None, _) => {}
                }
            }
            CommentTarget::Code(anchor) => {
                if let Some(file) = self.index.find_file(anchor.file()) {
                    match found.resolution() {
                        Some(resolution) => self.open_lines(file, resolution.span),
                        None => self.open_line(file, Line::new(0)),
                    }
                }
            }
        }
        self.status = Status::Comment(CommentStatus::Shown { id, gone });
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommentStatus {
    Empty,
    Added(CommentId),
    Dismissed(CommentId),
    Withdrawn(CommentId),
    NotSaved(Label),
    RepliesArrived(Count),
    NoComments,
    Shown { id: CommentId, gone: Gone },
}

impl CommentStatus {
    pub(crate) const fn tone(&self) -> Tone {
        match self {
            Self::Added(_) | Self::Dismissed(_) | Self::Withdrawn(_) | Self::RepliesArrived(_) => {
                Tone::Done
            }
            Self::Empty
            | Self::Shown {
                gone: Gone::Lines | Gone::Symbol | Gone::Step | Gone::Tour | Gone::File,
                ..
            } => Tone::Warning,
            Self::NotSaved(_) => Tone::Problem,
            Self::NoComments
            | Self::Shown {
                gone: Gone::Nothing,
                ..
            } => Tone::Plain,
        }
    }
}

impl fmt::Display for CommentStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("type the comment first; escape cancels it"),
            Self::Added(id) => write!(
                formatter,
                "comment {id} saved; the agent lists it with 'codemap . comments'"
            ),
            Self::Dismissed(id) => write!(formatter, "comment {id} dismissed"),
            Self::Withdrawn(id) => write!(formatter, "comment {id} withdrawn"),
            Self::NotSaved(error) => write!(formatter, "comment not saved: {}", error.as_str()),
            Self::RepliesArrived(count) if count.get() == 1 => formatter
                .write_str("a comment was answered; click the comment count to go to the reply"),
            Self::RepliesArrived(count) => write!(
                formatter,
                "{count} comments were answered; click the comment count to go to each reply"
            ),
            Self::NoComments => formatter.write_str("no comments"),
            Self::Shown { id, gone } => match gone {
                Gone::Nothing => write!(formatter, "comment {id}"),
                Gone::Lines | Gone::Symbol => write!(
                    formatter,
                    "comment {id}: its lines are gone; it is shown at the top of its file"
                ),
                Gone::Step => write!(
                    formatter,
                    "comment {id}: its step is gone; it is shown under the tour note"
                ),
                Gone::Tour => write!(
                    formatter,
                    "comment {id}: its tour is gone; it is shown under the comment count"
                ),
                Gone::File => write!(
                    formatter,
                    "comment {id}: its file is gone; it is shown under the comment count"
                ),
            },
        }
    }
}
