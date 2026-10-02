use features::Feature;
use ui::Run;

use crate::help;
use crate::ids;
use crate::model::Model;
use crate::theme::{ACCENT, TEXT, WEAK};
use crate::widgets::{Container, Frame, Padding, Scroller};

pub(super) fn help_view(model: &Model, frame: &mut Frame<'_>) {
    let id = ids::help_list();
    frame.start(Container::PanelColumn);
    frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    frame.note(
        "Making a tour by hand. Keys and gestures are in blue. The help button brings this tab back.",
        WEAK,
        Padding::Step,
    );
    for feature in help::by_hand() {
        entry(frame, *feature);
    }
    frame.note(
        "In the console. Quote a note that has spaces.",
        WEAK,
        Padding::Step,
    );
    for feature in help::console_commands() {
        entry(frame, *feature);
    }
    frame.note("Finding the rest of the window.", WEAK, Padding::Step);
    for feature in help::to_find() {
        entry(frame, *feature);
    }
    frame.note("Every other function of this window.", WEAK, Padding::Step);
    for feature in help::the_rest() {
        entry(frame, feature);
    }
    frame.finish();
    frame.finish();
}

fn entry(frame: &mut Frame<'_>, feature: Feature) {
    let spec = feature.spec();
    let mut runs = vec![Run::new(spec.name().as_str(), TEXT)];
    if let Some(words) = help::how(feature) {
        runs.push(Run::new(format!("  {}", words.as_str()), ACCENT));
    }
    frame.paragraph(runs, Padding::Step);
    frame.note(spec.summary().as_str(), WEAK, Padding::Step);
}
