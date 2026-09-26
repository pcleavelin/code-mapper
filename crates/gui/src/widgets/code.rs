use domain::{Column, FileId, Language, Line, Span};
use ui::{
    Button, Canvas, Color, Coordinate, Count, Draw, Id, Interaction, Kind, Layout, Px, Rect, Style,
};

use crate::action::Action;
use crate::grid::GUTTER;
use crate::keys::{self, CodeGesture};
use crate::model::Model;
use crate::peek::{Hovering, Intent, Probe};
use crate::status::Status;
use crate::theme::{SCROLLED_MARK, SELECTED_BAR, WEAK, WHEEL_ACROSS};
use crate::widgets::{Frame, TipAt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Width {
    Wide,
    Fit,
}

pub(crate) struct Marks<'marks> {
    pub(crate) background: &'marks dyn Fn(Line) -> Option<Color>,
    pub(crate) bar: &'marks dyn Fn(Line) -> Option<Color>,
}

pub(crate) struct CodeBlock<'marks> {
    pub(crate) file: FileId,
    pub(crate) start: Line,
    pub(crate) end: Line,
    pub(crate) id: Id,
    pub(crate) width: Width,
    pub(crate) marks: Marks<'marks>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CodeSpot {
    pub(crate) line: Line,
    pub(crate) column: Option<Column>,
}

pub(crate) struct Coded {
    pub(crate) interaction: Interaction,
    pub(crate) spot: Option<CodeSpot>,
}

struct RowMarks {
    background: Option<Color>,
    bar: Option<Color>,
}

fn spot_at(mouse: ui::Point, rect: Rect, span: Span, cell: ui::Extent, across: Px) -> CodeSpot {
    let last_row = i32::try_from(span.count().value()).unwrap_or(1) - 1;
    let row = (mouse.vertical - rect.top)
        .ratio(cell.height)
        .clamp(0, last_row);
    let column = (mouse.horizontal - rect.left + across)
        .ratio(cell.width)
        .max(0);
    let column = usize::try_from(column)
        .unwrap_or(0)
        .checked_sub(GUTTER.get())
        .map(|column| Column::new(u32::try_from(column).unwrap_or(0)));
    CodeSpot {
        line: Line::new(span.start().value() + u32::try_from(row).unwrap_or(0)),
        column,
    }
}

impl Frame<'_> {
    pub(crate) fn code_block(&mut self, model: &Model, block: &CodeBlock<'_>) -> Coded {
        let nothing = Coded {
            interaction: Interaction::default(),
            spot: None,
        };
        let Some(source) = model.index.file(block.file) else {
            return nothing;
        };
        let Some(last) = source.text().count().last() else {
            return nothing;
        };
        let end = block.end.min(last);
        let Some(span) = Span::new(block.start, end) else {
            return nothing;
        };
        let grid = self.grids.get(block.file, source, span);
        let size = self.metrics.font;
        let cell = self.metrics.cell;
        let width = cell.width * grid.columns();
        let height = cell.height * grid.rows();
        let previous = self.ui.interaction(block.id);
        let pointer = self.ui.pointer();
        let mut across = model.across.get(block.id);
        let wheel = pointer.wheel.vertical.get();
        if previous.hovered() && keys::scrolls_across(pointer) && wheel != 0.0 {
            let notch = wheel * Coordinate::of_integer(WHEEL_ACROSS.get()).get();
            across -= Coordinate::new(notch * cell.width.float()).truncate();
        }
        let most = previous
            .rect()
            .map_or(Px::ZERO, |rect| (width - rect.width).max(Px::ZERO));
        let across = across.clamp(Px::ZERO, most);
        self.push(Action::ScrollAcross(block.id, across));
        let rows: Vec<RowMarks> = span
            .lines()
            .map(|line| RowMarks {
                background: (block.marks.background)(line),
                bar: (block.marks.bar)(line),
            })
            .collect();
        let draw = move |canvas: &mut Canvas<'_>, rect: Rect| {
            let mut top = rect.top;
            for marks in &rows {
                if let Some(color) = marks.background {
                    canvas.rect(Rect::new(rect.left, top, rect.width, cell.height), color);
                }
                if let Some(color) = marks.bar {
                    canvas.rect(
                        Rect::new(rect.left, top, cell.width - SELECTED_BAR, cell.height),
                        color,
                    );
                }
                top += cell.height;
            }
            canvas.grid(ui::Point::new(rect.left - across, rect.top), size, &grid);
            if across > Px::ZERO {
                canvas.rect(
                    Rect::new(rect.left, rect.top, SCROLLED_MARK, rect.height),
                    WEAK,
                );
            }
        };
        let layout = match block.width {
            Width::Wide => Layout::row().grow_width().height(height),
            Width::Fit => Layout::row().width(width).height(height),
        };
        let draw: Box<dyn Draw> = Box::new(draw);
        let interaction = self
            .ui
            .leaf(Kind::Custom(draw), layout, Style::NONE, Some(block.id));
        let spot = interaction
            .rect()
            .filter(|_| interaction.hovered())
            .map(|rect| spot_at(pointer.mouse, rect, span, cell, across));
        if let Some(CodeSpot {
            line,
            column: Some(column),
        }) = spot
        {
            match keys::code_gesture(interaction, pointer) {
                Some(CodeGesture::Peek) => {
                    self.push(Action::Definition(block.file, line, column, Intent::Peek));
                }
                Some(CodeGesture::Jump) => {
                    self.push(Action::Definition(block.file, line, column, Intent::Jump));
                }
                None if !pointer.down.contains(Button::Left) => {
                    self.hover(model, block.file, line, column);
                }
                None => {}
            }
        }
        Coded { interaction, spot }
    }

    pub(crate) fn hover(&mut self, model: &Model, file: FileId, line: Line, column: Column) {
        match model.hovering(file, line, column) {
            Hovering::Tip(tip) => {
                self.tooltip = Some(TipAt {
                    tip,
                    at: self.ui.pointer().mouse,
                });
            }
            Hovering::Ask { language, probe } => self.push_hover(model, language, probe),
            Hovering::Nothing => {}
        }
    }

    pub(crate) fn push_hover(&mut self, model: &Model, language: Language, probe: Probe) {
        if model.queries.asks_now(&probe, model.now) && !model.work.no_server(language) {
            self.overlay.asked += Count::new(1);
            if !model.work.started(language) {
                self.overlay.status = Some(Status::Starting(language.program()));
            }
        }
        self.push(Action::Hover(language, probe));
    }
}
