use ui::{Count, Px};

use crate::action::Action;
use crate::ids;
use crate::model::Model;
use crate::theme::TEXT;
use crate::widgets::{Container, Frame, Scroller};

pub(super) fn output_panel(model: &Model, frame: &mut Frame<'_>) {
    let id = ids::output();
    frame.start(Container::OutputColumn);
    let mut offset = model.scrolls.get(id);
    if model.output_bottom.get() > 0 {
        frame.push(Action::OutputScrolled);
        offset = Px::LARGEST;
    }
    let scrolled = frame.scroll_column(id, offset, Scroller::Plain, None);
    let row_height = frame.row_height();
    let count = model.output.line_count();
    let window = frame.rows_window(
        scrolled.offset,
        scrolled.interaction.rect(),
        count,
        row_height,
        Count::new(20),
    );
    for line in model
        .output
        .lines()
        .skip(window.first.get())
        .take(window.visible.get())
    {
        frame.plain_line(line, TEXT);
    }
    frame.rows_after(count, &window, row_height, Px::ZERO);
    frame.finish();
    frame.command_row(&model.fields);
    frame.finish();
}
