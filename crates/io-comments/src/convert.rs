use std::path::Path;

use domain::{
    Anchor, Comment, CommentId, CommentState, CommentTarget, CommentText, Line, LineOffset,
    RelativePath, Reply, ReplyText, StepAddress, StepId, SymbolName, TextHash, TourName,
};

use crate::CommentFileText;
use crate::error::{CommentFault, CommentParseError, FieldKey, FieldValue};
use crate::wire;
use crate::wire::{CommentKey, TargetKind, WireComment, WireFault};

fn anywhere(fault: CommentFault) -> WireFault {
    WireFault { line: None, fault }
}

fn missing(key: CommentKey) -> WireFault {
    anywhere(CommentFault::MissingField(FieldKey::new(
        key.name().as_str(),
    )))
}

fn tour_name(wire: &WireComment) -> Result<TourName, WireFault> {
    let name = wire
        .tour
        .as_deref()
        .ok_or_else(|| missing(CommentKey::Tour))?;
    TourName::new(name).map_err(|_error| anywhere(CommentFault::InvalidName(FieldValue::new(name))))
}

fn target_from_wire(wire: &WireComment) -> Result<CommentTarget, WireFault> {
    match wire.target.ok_or_else(|| missing(CommentKey::Target))? {
        TargetKind::Tour => Ok(CommentTarget::Tour(tour_name(wire)?)),
        TargetKind::Step => {
            let tour = tour_name(wire)?;
            let step = wire
                .step
                .as_deref()
                .ok_or_else(|| missing(CommentKey::Step))?;
            let step = StepId::new(step)
                .ok_or_else(|| anywhere(CommentFault::InvalidStepId(FieldValue::new(step))))?;
            Ok(CommentTarget::Step(StepAddress { tour, step }))
        }
        TargetKind::Code => {
            let file = wire
                .file
                .as_deref()
                .ok_or_else(|| missing(CommentKey::File))?;
            let (start, end) = wire.lines.ok_or_else(|| missing(CommentKey::Lines))?;
            let hash = wire.hash.ok_or_else(|| missing(CommentKey::Hash))?;
            let symbol = (!wire.symbol.is_empty()).then(|| SymbolName::new(&wire.symbol));
            Ok(CommentTarget::Code(Anchor::new(
                RelativePath::new(file),
                symbol,
                LineOffset::new(start),
                LineOffset::new(end),
                TextHash::new(hash),
            )))
        }
    }
}

fn comment_from_wire(id: CommentId, wire: &WireComment) -> Result<Comment, WireFault> {
    let target = target_from_wire(wire)?;
    let author = wire.author.ok_or_else(|| missing(CommentKey::Author))?;
    let text = CommentText::new(&wire.text).ok_or_else(|| {
        anywhere(CommentFault::EmptyText(FieldKey::new(
            CommentKey::Text.name().as_str(),
        )))
    })?;
    let state = match (&wire.reply, wire.reply_author) {
        (None, None) => CommentState::Open,
        (Some(reply), Some(replier)) => CommentState::Answered(Reply {
            text: ReplyText::new(reply).ok_or_else(|| {
                anywhere(CommentFault::EmptyText(FieldKey::new(
                    CommentKey::Reply.name().as_str(),
                )))
            })?,
            author: replier,
        }),
        (Some(_), None) => return Err(missing(CommentKey::ReplyAuthor)),
        (None, Some(_)) => return Err(missing(CommentKey::Reply)),
    };
    Ok(Comment::new(id, target, author, text, state))
}

pub(crate) fn comment_to_wire(comment: &Comment) -> WireComment {
    let mut wire = WireComment {
        target: None,
        tour: None,
        step: None,
        file: None,
        symbol: String::new(),
        lines: None,
        hash: None,
        author: Some(comment.author()),
        text: comment.text().as_str().to_owned(),
        reply: comment.reply().map(|reply| reply.text.as_str().to_owned()),
        reply_author: comment.reply().map(|reply| reply.author),
    };
    match comment.target() {
        CommentTarget::Tour(name) => {
            wire.target = Some(TargetKind::Tour);
            wire.tour = Some(name.as_str().to_owned());
        }
        CommentTarget::Step(address) => {
            wire.target = Some(TargetKind::Step);
            wire.tour = Some(address.tour.as_str().to_owned());
            wire.step = Some(address.step.as_str().to_owned());
        }
        CommentTarget::Code(anchor) => {
            wire.target = Some(TargetKind::Code);
            wire.file = Some(anchor.file().as_str().to_owned());
            wire.symbol = anchor
                .symbol()
                .map_or_else(String::new, |symbol| symbol.as_str().to_owned());
            wire.lines = Some((anchor.start().value(), anchor.end().value()));
            wire.hash = Some(anchor.hash().value());
        }
    }
    wire
}

pub(crate) fn comment_from_text(
    id: CommentId,
    file: &Path,
    text: &CommentFileText,
) -> Result<Comment, CommentParseError> {
    let located = |fault: WireFault| CommentParseError {
        file: file.to_path_buf(),
        line: fault.line.map(Line::new),
        fault: fault.fault,
    };
    let parsed = wire::parse(text.as_str()).map_err(located)?;
    comment_from_wire(id, &parsed).map_err(located)
}
