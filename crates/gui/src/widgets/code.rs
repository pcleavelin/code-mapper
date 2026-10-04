use domain::{Column, FileId, Language, Line, Span};
use ui::{
    Button, Canvas, Color, Count, Draw, Extent, Id, Interaction, Kind, Layout, Point, Px, Rect,
    ScrollAxes, Style,
};

use crate::action::Action;
use crate::grid::GUTTER;
use crate::keys::{self, CodeGesture};
use crate::model::Model;
use crate::peek::{Hovering, Intent, Probe};
use crate::theme::{PIXEL, SELECTED_BAR};
use crate::widgets::{Frame, Scrolled, TipAt};

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
    pub(crate) dragged_to: Option<Line>,
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
    fn line_dragged_to(
        &self,
        interaction: Interaction,
        id: Id,
        span: Span,
        across: Px,
    ) -> Option<Line> {
        interaction.drag()?;
        let placement = self.ui.placement(id)?;
        let shown = placement.rect.intersect(placement.clip);
        let mouse = self.ui.pointer().mouse;
        (!shown.is_empty()).then(|| {
            let vertical = mouse.vertical.clamp(shown.top, shown.bottom() - PIXEL);
            let inside = ui::Point::new(mouse.horizontal, vertical);
            spot_at(inside, placement.rect, span, self.metrics.cell, across).line
        })
    }

    pub(crate) fn code_block(&mut self, model: &Model, block: &CodeBlock<'_>) -> Coded {
        let nothing = Coded {
            interaction: Interaction::default(),
            spot: None,
            dragged_to: None,
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
        let extent = Extent::new(cell.width * grid.columns(), cell.height * grid.rows());
        let pointer = self.ui.pointer();
        let rows: Vec<RowMarks> = span
            .lines()
            .map(|line| RowMarks {
                background: (block.marks.background)(line),
                bar: (block.marks.bar)(line),
            })
            .collect();
        let marks = move |canvas: &mut Canvas<'_>, rect: Rect| {
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
        };
        let code = move |canvas: &mut Canvas<'_>, rect: Rect| {
            canvas.grid(rect.origin(), size, &grid);
        };
        let Scrolled {
            interaction,
            offset: across,
        } = self.scrolled_code(model, block, extent, Box::new(marks), Box::new(code));
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
        let dragged_to = self.line_dragged_to(interaction, block.id, span, across);
        Coded {
            interaction,
            spot,
            dragged_to,
        }
    }

    fn scrolled_code(
        &mut self,
        model: &Model,
        block: &CodeBlock<'_>,
        extent: Extent,
        marks: Box<dyn Draw>,
        code: Box<dyn Draw>,
    ) -> Scrolled {
        let mut offset = Point::new(model.across.get(block.id), Px::ZERO);
        let across = self.ui.scroll_by_wheel(block.id, &mut offset).horizontal;
        self.push(Action::ScrollAcross(block.id, across));
        let layout = match block.width {
            Width::Wide => Layout::row().grow_width(),
            Width::Fit => Layout::row().width(extent.width),
        };
        let interaction = self.ui.open(
            Kind::Custom(marks),
            layout
                .scroll(Point::new(across, Px::ZERO))
                .scroll_axes(ScrollAxes::HORIZONTAL),
            Style::NONE,
            Some(block.id),
        );
        self.ui.leaf(
            Kind::Custom(code),
            Layout::row().width(extent.width).height(extent.height),
            Style::NONE,
            None,
        );
        self.ui.close();
        let interaction = if self.ui.on_scrollbar(block.id) || self.ui.dragging() {
            Interaction::default()
        } else {
            interaction
        };
        Scrolled {
            interaction,
            offset: across,
        }
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
            Hovering::Reopen => self.push(Action::ReopenHover),
            Hovering::Nothing => {}
        }
    }

    pub(crate) fn push_hover(&mut self, model: &Model, language: Language, probe: Probe) {
        if model.queries.asks_now(&probe, model.now) && !model.work.no_server(language) {
            self.overlay.asked += Count::new(1);
        }
        self.push(Action::Hover(language, probe));
    }
}
