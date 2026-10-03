use strum::VariantArray;
use ui::{Point, Px, Run};

use crate::action::Action;
use crate::ids;
use crate::model::Model;
use crate::theme::{ACCENT, TEXT, WEAK};
use crate::welcome::{BoardAction, GuideStep, WelcomeAct};
use crate::widgets::{Chosen, Container, Frame, Padding};

pub(crate) fn surface(model: &Model, frame: &mut Frame<'_>) {
    if model.welcome.fills_panel() {
        board(frame);
    }
}

pub(crate) fn coach(frame: &mut Frame<'_>, step: GuideStep) {
    let row = frame.row_height();
    let at = Point::new(
        Px::ZERO,
        frame.ui.size().height - (row + row + row + row + row),
    );
    frame.start(Container::Coach {
        at,
        width: frame.ui.size().width,
    });
    frame.start(Container::Stack);
    chord_line(frame, BoardAction::Palette);
    frame.note(step.sentence(), TEXT, Padding::Step);
    frame.start(Container::ToolbarSmall);
    if frame
        .button("skip", ids::GUIDE_SKIP.target(), Chosen::Plain)
        .clicked()
    {
        frame.push(Action::Welcome(WelcomeAct::Skip));
    }
    if frame
        .button("close", ids::GUIDE_CLOSE.target(), Chosen::Plain)
        .clicked()
    {
        frame.push(Action::Welcome(WelcomeAct::Leave));
    }
    frame.finish();
    frame.finish();
    frame.finish();
}

pub(crate) fn ring(model: &Model, frame: &mut Frame<'_>) {
    let Some(step) = model.welcome.guide_step() else {
        return;
    };
    let Some(rect) = frame.ui.interaction(step.spotlight()).rect() else {
        return;
    };
    frame.mark_rect(rect);
}

fn board(frame: &mut Frame<'_>) {
    frame.spring();
    frame.start(Container::Stack);
    frame.title("codemap");
    frame.note(
        "Start from an action. A chord beside a row is the key that runs it.",
        WEAK,
        Padding::Tour,
    );
    for action in BoardAction::VARIANTS {
        row(frame, *action);
    }
    frame.note(
        "A note is still a Console command, tour-note or step-note. The index it takes is the one the tours command prints, not the 1.1 numbers in the window.",
        WEAK,
        Padding::Tour,
    );
    frame.finish();
    frame.spring();
}

fn row(frame: &mut Frame<'_>, action: BoardAction) {
    let mut runs = vec![
        Run::new(action.title().as_str(), TEXT),
        Run::new("   ", WEAK),
    ];
    if let Some(chord) = action.chord() {
        runs.push(Run::new(chord, ACCENT));
        runs.push(Run::new("   ", WEAK));
    }
    runs.push(Run::new(action.detail().as_str(), WEAK));
    if frame
        .row(runs, action.control().target(), Chosen::Plain)
        .clicked()
    {
        frame.push(action.press());
    }
}

fn chord_line(frame: &mut Frame<'_>, action: BoardAction) {
    let mut runs = vec![
        Run::new(action.title().as_str(), TEXT),
        Run::new("  ", WEAK),
    ];
    if let Some(chord) = action.chord() {
        runs.push(Run::new(chord, ACCENT));
    }
    frame.row_text(runs);
}
