use platform::Cursor;
use ui::{Button, Icon, Label, Ui};

use crate::action::Action;
use crate::authoring::{Authoring, StepDrop, StepGrab, StripAct, Zone};
use crate::ids;
use crate::model::{Model, StepSlot};
use crate::panels::View;
use crate::theme::{BAR_PADDING, Cells, WEAK};
use crate::widgets::{Container, Frame};
use crate::wizard::WizardAct;

pub(super) fn target_strip(model: &Model, frame: &mut Frame<'_>, view: View) {
    if let Some(wizard) = model.wizard.as_ref()
        && matches!(view, View::Source | View::Graph)
    {
        super::wizard::wizard_strip(model, wizard, frame, view);
        return;
    }
    let place = Label::new(view.name().as_str());
    let id = ids::target_strip().with(place.as_str());
    let room = frame
        .ui
        .placement(id)
        .map(|placement| placement.rect.width - BAR_PADDING * 2);
    let strip = model.target_strip();
    frame.start(Container::Strip(id));
    for row in strip.rows(frame.cell_width(), room) {
        frame.start(Container::ToolbarSmall);
        for run in row.runs {
            frame.label(run.text.clone(), run.color);
        }
        for button in row.buttons {
            let (target, act) = match button.act {
                StripAct::AddAtTopLevel => (ids::TARGET_TOP.with(&place), Authoring::AddAtTopLevel),
                StripAct::AddOffered => (ids::ADD_OFFER.with(&place), Authoring::AddOffered),
            };
            let cells = Cells::of_count(button.text.columns());
            if frame
                .small_button_sized(button.text.clone(), Some(cells), target)
                .clicked()
            {
                frame.push(Action::Authoring(act));
            }
        }
        if let Some(aside) = row.aside {
            frame.label(aside.clone(), WEAK);
        }
        frame.finish();
    }
    frame.finish();
}

pub(super) fn new_tour_button(frame: &mut Frame<'_>) {
    let label = format!("{} new tour", Icon::Add.glyph().get());
    if frame.small_button(label, ids::NEW_TOUR.target()).clicked() {
        frame.push(Action::Wizard(WizardAct::Start));
    }
}

pub(super) fn target_mark(model: &Model, step: StepSlot) -> Option<Label> {
    (model.target_under() == Some(step)).then(|| Label::new("  + adds here"))
}

pub(crate) fn step_list_drop(model: &Model, ui: &Ui) -> Option<StepDrop> {
    let grab = model.step_grab?;
    let mouse = ui.pointer().mouse;
    (0..model.step_count(grab.key.tour).get())
        .map(StepSlot::new)
        .find_map(|step| {
            let row = ui
                .interaction(ids::STEP_LIST_ROW.id().nth(step.get()))
                .rect()?;
            row.contains(mouse).then(|| {
                let zone = Zone::of(row, mouse);
                StepDrop {
                    onto: step,
                    zone,
                    band: zone.band(row),
                }
            })
        })
}

pub(crate) fn step_list_input(model: &Model, ui: &Ui) -> Vec<Action> {
    let Some(grab) = model.step_grab else {
        return Vec::new();
    };
    let mouse = ui.pointer().mouse;
    let mut actions = vec![Action::Authoring(Authoring::DragStep(mouse))];
    if !ui.pointer().down.contains(Button::Left) {
        let moving = grab.dragged_to(mouse).is_moving();
        actions.push(Action::Authoring(Authoring::DropStep(
            step_list_drop(model, ui).filter(|_| moving),
        )));
    }
    actions
}

pub(super) fn step_list_band(model: &Model, frame: &mut Frame<'_>) {
    if !model.step_grab.is_some_and(StepGrab::is_moving) {
        return;
    }
    frame.cursor = Cursor::Grabbing;
    if let Some(drop) = step_list_drop(model, frame.ui) {
        frame.drop_band(drop.band);
    }
}
