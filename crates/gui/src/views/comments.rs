use domain::{Comment, CommentState, CommentTarget};
use ui::{Count, Id, Label, Point, Px, Run};

use crate::action::Action;
use crate::comments::{CommentAct, DraftOn};
use crate::field::Which;
use crate::ids;
use crate::model::Gone;
use crate::model::Model;
use crate::text::{Counted, Noun, Tag};
use crate::theme::{
    COMMENT_BUTTON_GAP, COMMENT_BUTTONS_ROOM, COMMENT_FIELD_GUESS, COMMENT_FRAME,
    COMMENT_LEAST_FIELD, COMMENT_POPUP, Cells, FAINT, GREEN, NOTE, ORANGE, TEXT, WEAK,
};
use crate::widgets::{Chosen, CommentLook, Container, Frame, Padding};

pub(super) fn box_id(comment: &Comment) -> Id {
    ids::comment_box().with(comment.id().as_str())
}

pub(super) fn draft_id() -> Id {
    ids::comment_box().with("draft")
}

pub(super) fn comment_box(frame: &mut Frame<'_>, comment: &Comment, gone: Gone) {
    let (look, state, color) = match comment.state() {
        CommentState::Open => (CommentLook::Open, "open comment", ORANGE),
        CommentState::Answered(_) => (CommentLook::Answered, "answered comment", GREEN),
    };
    frame.start(Container::Comment {
        look,
        id: box_id(comment),
    });
    frame.start(Container::FillRow);
    let mut runs = vec![
        Run::new(state, color),
        Run::new(format!(" by {}", Tag::author_word(comment.author())), WEAK),
        Run::new(format!("  {}", comment.id()), FAINT),
    ];
    if gone != Gone::Nothing {
        runs.push(Run::new(format!("  {}", Tag::gone(gone)), ORANGE));
    }
    frame.label_runs(runs);
    frame.grow();
    let id = Label::new(comment.id().as_str());
    if comment.is_open() {
        frame.label("waiting for the agent", WEAK);
        frame.cells_gap(COMMENT_BUTTON_GAP);
        let withdraw = ids::WITHDRAW_COMMENT.with(&id);
        if frame.small_button("withdraw", withdraw).clicked() {
            frame.push(Action::Comment(CommentAct::Dismiss(comment.id().clone())));
        }
        frame.attach_tip(withdraw);
    } else {
        let dismiss = ids::DISMISS_COMMENT.with(&id);
        if frame.small_button("dismiss", dismiss).clicked() {
            frame.push(Action::Comment(CommentAct::Dismiss(comment.id().clone())));
        }
        frame.attach_tip(dismiss);
    }
    frame.finish();
    frame.note(comment.text().as_str(), TEXT, Padding::Step);
    if let Some(reply) = comment.reply() {
        frame.label_runs(vec![
            Run::new("\u{21b3} reply", GREEN),
            Run::new(format!(" by {}", Tag::author_word(reply.author)), WEAK),
        ]);
        frame.note(reply.text.as_str(), NOTE, Padding::Step);
    }
    frame.finish();
}

pub(super) fn draft_box(model: &Model, frame: &mut Frame<'_>, on: &DraftOn, width: Option<Px>) {
    let what = match on {
        DraftOn::Tour(name) => format!("new comment on the tour {name}"),
        DraftOn::Step(_) => "new comment on this step".to_owned(),
        DraftOn::Lines { span, .. } if span.start() == span.end() => {
            format!("new comment on line {}", span.start().number())
        }
        DraftOn::Lines { span, .. } => format!(
            "new comment on lines {}-{}",
            span.start().number(),
            span.end().number()
        ),
    };
    let cells = width.map_or(COMMENT_FIELD_GUESS, |width| {
        Count::new(usize::try_from(width.ratio(frame.cell_width())).unwrap_or(0))
    });
    let room = Count::new(
        cells
            .get()
            .saturating_sub(COMMENT_BUTTONS_ROOM.get())
            .max(COMMENT_LEAST_FIELD.get()),
    );
    frame.start(Container::Comment {
        look: CommentLook::Draft,
        id: draft_id(),
    });
    frame.label_runs(vec![
        Run::new(what, TEXT),
        Run::new("  enter adds it, escape cancels", WEAK),
    ]);
    frame.start(Container::StepRow {
        selected: Chosen::Plain,
    });
    frame.field_named(
        &model.fields,
        Which::Comment,
        ids::COMMENT_FIELD.id(),
        &Label::new("a question about the map, or a change you want"),
        Cells::of_count(room.get()),
    );
    let add = ids::COMMENT_ADD.target();
    if frame.small_button("add", add).clicked() {
        frame.push(Action::Comment(CommentAct::Submit));
    }
    frame.attach_tip(add);
    let cancel = ids::COMMENT_CANCEL.target();
    if frame.small_button("cancel", cancel).clicked() {
        frame.push(Action::Comment(CommentAct::Cancel));
    }
    frame.finish();
    frame.finish();
}

pub(super) fn comment_count(model: &Model, frame: &mut Frame<'_>) {
    let counts = model.shelf.counts();
    if counts.total.get() == 0 {
        return;
    }
    let mut runs = vec![Run::new(
        Counted::new(counts.total, Noun::Comment).to_string(),
        WEAK,
    )];
    if counts.answered.get() > 0 {
        runs.push(Run::new(format!(", {} answered", counts.answered), GREEN));
    }
    let target = ids::COMMENT_COUNT.target();
    let count = frame.runs_button(runs, target);
    if count.clicked() {
        frame.push(Action::Comment(CommentAct::Next));
    }
    frame.attach_tip(target);
    let (Some(comment), Some(under)) = (model.shelf.popup(), count.rect()) else {
        return;
    };
    let width = COMMENT_POPUP.of(frame.cell_width());
    let at = Point::new((under.right() - width).max(Px::ZERO), under.bottom());
    frame.start(Container::CommentPopup { at, width });
    comment_box(frame, comment, Gone::of(model, comment));
    frame.start(Container::FillRow);
    frame.label(format!("on {}", comment_place(comment).as_str()), WEAK);
    frame.grow();
    let close = ids::COMMENT_CLOSE.target();
    if frame.small_button("close", close).clicked() {
        frame.push(Action::Comment(CommentAct::ClosePopup));
    }
    frame.attach_tip(close);
    frame.finish();
    frame.finish();
}

fn comment_place(comment: &Comment) -> Label {
    Label::new(match comment.target() {
        CommentTarget::Tour(name) => format!("the tour {name}"),
        CommentTarget::Step(address) => {
            format!("step {} of the tour {}", address.step, address.tour)
        }
        CommentTarget::Code(anchor) => match anchor.symbol() {
            Some(symbol) => format!("{} in {}", symbol.as_str(), anchor.file()),
            None => anchor.file().to_string(),
        },
    })
}

pub(super) fn estimate(comment: &Comment, row: Px) -> Px {
    let reply = comment
        .reply()
        .map_or(0, |reply| reply.text.lines().count() + 1);
    let rows = 1 + comment.text().lines().count().max(1) + reply;
    row * Count::new(rows) + COMMENT_FRAME
}
