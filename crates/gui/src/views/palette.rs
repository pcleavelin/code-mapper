use ui::{Count, Label, Point, Px, Run};

use crate::action::Action;
use crate::field::Which;
use crate::ids;
use crate::model::Model;
use crate::palette::{EntryKind, PALETTE_ROWS, PaletteAction};
use crate::theme::{ACCENT, GREEN, ORANGE, PALETTE_FIELD, TEXT, TOOLTIP_PADDING, WEAK};
use crate::widgets::{Chosen, Container, Frame};

const fn tag_color(kind: EntryKind) -> ui::Color {
    match kind {
        EntryKind::Action => ORANGE,
        EntryKind::Path | EntryKind::Step => GREEN,
        EntryKind::Symbol => ACCENT,
        EntryKind::File | EntryKind::View => WEAK,
    }
}

pub(super) fn palette(model: &Model, frame: &mut Frame<'_>) {
    let Some(palette) = &model.palette else {
        return;
    };
    let window = frame.ui.size();
    let width = PALETTE_FIELD.of(frame.cell_width()) + TOOLTIP_PADDING * 2;
    let at = Point::new(
        ((window.width - width) / 2).max(Px::ZERO),
        window.height / 10,
    );
    frame.start(Container::Palette { at, width });
    frame.field(
        &model.fields,
        Which::Palette,
        &Label::new("go to a path, step, symbol, file or view; > for actions"),
        PALETTE_FIELD,
    );
    let total = palette.entries.len();
    if total == 0 {
        frame.label("nothing matches", WEAK);
    }
    for (row, entry) in palette
        .entries
        .iter()
        .enumerate()
        .skip(palette.top.get())
        .take(PALETTE_ROWS.get())
    {
        let chosen = if row == palette.selected.get() {
            Chosen::Chosen
        } else {
            Chosen::Plain
        };
        let runs = vec![
            Run::new(entry.name.clone(), TEXT),
            Run::new(format!("   {}", entry.detail.as_str()), WEAK),
        ];
        let tag = Run::new(entry.kind.tag(), tag_color(entry.kind));
        if frame
            .palette_row(
                tag,
                runs,
                entry.chord.clone(),
                ids::PALETTE_ROW.nth(Count::new(row)),
                chosen,
            )
            .clicked()
        {
            frame.push(Action::Palette(PaletteAction::Run(Count::new(row))));
        }
    }
    if total > 0 {
        let first = palette.top.get() + 1;
        let last = (palette.top.get() + PALETTE_ROWS.get()).min(total);
        frame.label(
            format!("{first}-{last} of {total}   up/down to move, enter to go, esc to close"),
            WEAK,
        );
    }
    frame.finish();
}
