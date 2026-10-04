use std::collections::BTreeMap;
use std::fmt;

use crate::id::ShortId;
use crate::index::{FileId, Index};
use crate::map::{Anchor, Author, MapError, Resolution, StepAddress, Tour, TourName, pin_span};
use crate::text::{RelativePath, Span};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommentId(String);

impl CommentId {
    const DIGITS: usize = 6;

    pub fn new(id: &str) -> Option<Self> {
        ShortId::valid(id, Self::DIGITS).then(|| Self(id.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn fresh(target: &CommentTarget, text: &CommentText, taken: impl Fn(&Self) -> bool) -> Self {
        let place = match target {
            CommentTarget::Step(address) => format!("step\n{}\n{}", address.tour, address.step),
            CommentTarget::Tour(name) => format!("tour\n{name}"),
            CommentTarget::Code(anchor) => format!(
                "code\n{}\n{}\n{}\n{}",
                anchor.file(),
                anchor.symbol().map_or("", |symbol| symbol.as_str()),
                anchor.start(),
                anchor.end()
            ),
        };
        let seed = format!("{place}\n{}", text.as_str());
        Self(ShortId::fresh(&seed, Self::DIGITS, |id| taken(&Self(id.to_owned()))).into_text())
    }
}

impl fmt::Display for CommentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommentText(String);

impl CommentText {
    pub fn new(text: &str) -> Option<Self> {
        (!text.trim().is_empty()).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.0.lines()
    }
}

impl fmt::Display for CommentText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReplyText(String);

impl ReplyText {
    pub fn new(text: &str) -> Option<Self> {
        (!text.trim().is_empty()).then(|| Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.0.lines()
    }
}

impl fmt::Display for ReplyText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommentTarget {
    Step(StepAddress),
    Tour(TourName),
    Code(Anchor),
}

impl CommentTarget {
    pub fn code(index: &Index, file: FileId, span: Span) -> Result<Self, MapError> {
        pin_span(index, file, span).map(|pinned| Self::Code(pinned.anchor))
    }

    pub fn tour(&self) -> Option<&TourName> {
        match self {
            Self::Step(address) => Some(&address.tour),
            Self::Tour(name) => Some(name),
            Self::Code(_) => None,
        }
    }

    pub fn file(&self) -> Option<&RelativePath> {
        match self {
            Self::Code(anchor) => Some(anchor.file()),
            Self::Step(_) | Self::Tour(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub text: ReplyText,
    pub author: Author,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommentState {
    Open,
    Answered(Reply),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    id: CommentId,
    target: CommentTarget,
    author: Author,
    text: CommentText,
    state: CommentState,
    resolution: Option<Resolution>,
}

impl Comment {
    pub fn new(
        id: CommentId,
        target: CommentTarget,
        author: Author,
        text: CommentText,
        state: CommentState,
    ) -> Self {
        Self {
            id,
            target,
            author,
            text,
            state,
            resolution: None,
        }
    }

    pub fn id(&self) -> &CommentId {
        &self.id
    }

    pub fn target(&self) -> &CommentTarget {
        &self.target
    }

    pub fn author(&self) -> Author {
        self.author
    }

    pub fn text(&self) -> &CommentText {
        &self.text
    }

    pub fn state(&self) -> &CommentState {
        &self.state
    }

    pub fn reply(&self) -> Option<&Reply> {
        match &self.state {
            CommentState::Open => None,
            CommentState::Answered(reply) => Some(reply),
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == CommentState::Open
    }

    pub fn resolution(&self) -> Option<Resolution> {
        self.resolution
    }

    pub fn resolve(&mut self, index: &Index) {
        self.resolution = match &self.target {
            CommentTarget::Code(anchor) => anchor.resolve(index),
            CommentTarget::Step(_) | CommentTarget::Tour(_) => None,
        };
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommentError {
    NoSuchComment(CommentId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Comments {
    comments: BTreeMap<CommentId, Comment>,
}

impl Comments {
    pub fn new(comments: Vec<Comment>) -> Self {
        Self {
            comments: comments
                .into_iter()
                .map(|comment| (comment.id.clone(), comment))
                .collect(),
        }
    }

    pub fn add(
        &mut self,
        index: &Index,
        target: CommentTarget,
        author: Author,
        text: CommentText,
    ) -> CommentId {
        let id = CommentId::fresh(&target, &text, |id| self.comments.contains_key(id));
        let mut comment = Comment::new(id.clone(), target, author, text, CommentState::Open);
        comment.resolve(index);
        self.comments.insert(id.clone(), comment);
        id
    }

    pub fn reply(&mut self, id: &CommentId, reply: Reply) -> Result<&Comment, CommentError> {
        let comment = self
            .comments
            .get_mut(id)
            .ok_or_else(|| CommentError::NoSuchComment(id.clone()))?;
        comment.state = CommentState::Answered(reply);
        Ok(comment)
    }

    pub fn dismiss(&mut self, id: &CommentId) -> Result<Comment, CommentError> {
        self.comments
            .remove(id)
            .ok_or_else(|| CommentError::NoSuchComment(id.clone()))
    }

    pub fn get(&self, id: &CommentId) -> Option<&Comment> {
        self.comments.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Comment> {
        self.comments.values()
    }

    pub fn open(&self) -> impl Iterator<Item = &Comment> {
        self.iter().filter(|comment| comment.is_open())
    }

    pub fn on_tour<'a>(&'a self, name: &'a TourName) -> impl Iterator<Item = &'a Comment> {
        self.iter()
            .filter(move |comment| comment.target == CommentTarget::Tour(name.clone()))
    }

    pub fn on_step<'a>(&'a self, address: &'a StepAddress) -> impl Iterator<Item = &'a Comment> {
        self.iter().filter(
            move |comment| matches!(&comment.target, CommentTarget::Step(step) if step == address),
        )
    }

    pub fn on_gone_steps<'a>(&'a self, tour: &'a Tour) -> impl Iterator<Item = &'a Comment> {
        self.iter().filter(move |comment| {
            matches!(
                &comment.target,
                CommentTarget::Step(address)
                    if &address.tour == tour.name() && tour.step(&address.step).is_none()
            )
        })
    }

    pub fn next_after(&self, after: Option<&CommentId>) -> Option<&Comment> {
        let answered = self.iter().any(|comment| !comment.is_open());
        let wanted = |comment: &&Comment| comment.is_open() != answered;
        let later = self
            .iter()
            .filter(wanted)
            .find(|comment| after.is_some_and(|after| &comment.id > after));
        later.or_else(|| self.iter().find(wanted))
    }

    pub fn in_file<'a>(&'a self, file: &'a RelativePath) -> impl Iterator<Item = &'a Comment> {
        self.iter()
            .filter(move |comment| comment.target.file() == Some(file))
    }

    pub fn is_empty(&self) -> bool {
        self.comments.is_empty()
    }

    pub fn resolve_all(&mut self, index: &Index) {
        for comment in self.comments.values_mut() {
            comment.resolve(index);
        }
    }
}

#[cfg(test)]
mod tests;
