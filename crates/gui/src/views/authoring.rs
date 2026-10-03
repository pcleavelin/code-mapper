use domain::SymbolName;
use platform::Cursor;
use ui::{Button, Icon, Label, Run, Ui};

use crate::action::Action;
use crate::authoring::{AddOffer, Authoring, StepDrop, StepGrab, Zone};
use crate::ids;
use crate::model::{Model, StepKey, StepSlot};
use crate::panels::View;
use crate::status::Status;
use crate::theme::{ACCENT, TEXT, WEAK};
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
    frame.start(Container::ToolbarSmall);
    match model
        .nav
        .tour()
        .and_then(|tour| Some((tour, model.tour(tour)?)))
    {
        None => frame.label("open a tour to add steps to it", WEAK),
        Some((tour, found)) => {
            frame.label("adds to", WEAK);
            frame.label(found.name().as_str(), TEXT);
            match model.target_under() {
                Some(step) => {
                    let key = StepKey { tour, step };
                    let symbol = model
                        .step(key)
                        .and_then(domain::Step::symbol)
                        .map_or("(lines)", SymbolName::as_str);
                    frame.label(
                        format!("under {} {symbol}", model.number_of(key).as_str()),
                        ACCENT,
                    );
                    if frame
                        .small_button("top level", ids::TARGET_TOP.with(&place))
                        .clicked()
                    {
                        frame.push(Action::Authoring(Authoring::AddAtTopLevel));
                    }
                }
                None => frame.label("at the top level", ACCENT),
            }
            let offer = match model.add_offer() {
                Some(AddOffer::Lines(label) | AddOffer::Symbol(_, label)) => Some(label),
                None => None,
            };
            if let Some(label) = offer {
                if frame
                    .small_button(label, ids::ADD_OFFER.with(&place))
                    .clicked()
                {
                    frame.push(Action::Authoring(Authoring::AddOffered));
                }
            } else {
                frame.caption(vec![Run::new(Status::select_symbol_or_lines(), WEAK)]);
            }
        }
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
