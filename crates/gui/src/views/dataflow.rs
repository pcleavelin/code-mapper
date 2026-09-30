use ui::{Canvas, Color, Count, FontSize, Point, Px, Rect, Run, Size};

use crate::action::Action;
use crate::dataflow::{Columns, DataAction, Flow, Lane, Mark, flow};
use crate::derived::Layers;
use crate::ids;
use crate::keys;
use crate::model::Model;
use crate::nav::Scrolling;
use crate::theme::{ACCENT, BACKGROUND, BORDER, HOVER, PANEL, SELECTED, TEXT, WEAK};
use crate::types::{TypeModel, TypesAction};
use crate::widgets::{Container, Frame, Scroller};

use super::types::{access_color, crate_color};

const NAME_CELLS: usize = 32;
const COLUMN_CELLS: usize = 6;
const HEADER_LINES: usize = 3;

#[derive(Clone, Copy, Debug)]
struct Grid {
    cell: Px,
    size: FontSize,
    across: Px,
    columns: usize,
}

impl Grid {
    fn name_width(self) -> Px {
        self.cell * Count::new(NAME_CELLS)
    }

    fn column_width(self) -> Px {
        self.cell * Count::new(COLUMN_CELLS)
    }

    fn column_left(self, origin: Px, at: usize) -> Px {
        origin + self.name_width() + self.column_width() * Count::new(at) - self.across
    }

    fn total(self) -> Px {
        self.name_width() + self.column_width() * Count::new(self.columns)
    }

    fn column_at(self, origin: Px, horizontal: Px) -> Option<usize> {
        let offset = horizontal - (origin + self.name_width() - self.across);
        if offset < Px::ZERO {
            return None;
        }
        let at = usize::try_from(offset.get() / self.column_width().get().max(1)).ok()?;
        (at < self.columns).then_some(at)
    }
}

fn clipped(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_owned();
    }
    let kept: String = text.chars().take(room.saturating_sub(1)).collect();
    format!("{kept}\u{2026}")
}

fn mark_color(mark: Mark) -> Color {
    match mark {
        Mark::Defines => TEXT,
        Mark::Access(access) => access_color(access),
    }
}

fn paint_header(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    grid: Grid,
    names: &[(String, Color)],
    focus: Option<usize>,
    row: Px,
) {
    canvas.rect(rect, BACKGROUND);
    canvas.text(
        Point::new(rect.left + Px::new(4), rect.top + row * 2 + Px::new(4)),
        grid.size,
        "step",
        WEAK,
    );
    canvas.push_clip(Rect::new(
        rect.left + grid.name_width(),
        rect.top,
        (rect.width - grid.name_width()).max(Px::ZERO),
        rect.height,
    ));
    for (at, (name, color)) in names.iter().enumerate() {
        let left = grid.column_left(rect.left, at);
        if focus == Some(at) {
            canvas.rect(
                Rect::new(left, rect.top, grid.column_width(), rect.height),
                SELECTED,
            );
        }
        let line = row * i32::try_from(at % HEADER_LINES).unwrap_or(0) + Px::new(4);
        let shown = clipped(name, COLUMN_CELLS * HEADER_LINES - 1);
        let text_width = grid.cell * Count::new(shown.chars().count());
        let center = left + grid.column_width() / 2;
        canvas.text(
            Point::new(
                (center - text_width / 2).max(rect.left + grid.name_width() + Px::new(2)),
                rect.top + line,
            ),
            grid.size,
            shown,
            *color,
        );
        canvas.rect(
            Rect::new(
                center,
                rect.top + line + row,
                Px::new(1),
                rect.bottom() - (rect.top + line + row),
            ),
            BORDER,
        );
    }
    canvas.pop_clip();
}

#[derive(Clone, Debug)]
struct LanePaint {
    lane: Lane,
    columns: Vec<usize>,
    spans: Vec<(usize, usize)>,
    at: usize,
    chosen: bool,
    hovered: bool,
    focus: Option<usize>,
}

fn paint_lane(canvas: &mut Canvas<'_>, rect: Rect, grid: Grid, paint: &LanePaint) {
    if paint.chosen {
        canvas.rect(rect, SELECTED);
    } else if paint.hovered {
        canvas.rect(rect, HOVER);
    }
    let top = rect.top + Px::new(2);
    let indent = grid.cell * Count::new(paint.lane.depth);
    let pen = canvas.text(
        Point::new(rect.left + Px::new(4) + indent, top),
        grid.size,
        format!("{} ", paint.lane.number),
        WEAK,
    );
    let used = paint.lane.depth + paint.lane.number.chars().count() + 2;
    canvas.text(
        Point::new(pen, top),
        grid.size,
        clipped(&paint.lane.symbol, NAME_CELLS.saturating_sub(used)),
        TEXT,
    );
    canvas.push_clip(Rect::new(
        rect.left + grid.name_width(),
        rect.top,
        (rect.width - grid.name_width()).max(Px::ZERO),
        rect.height,
    ));
    for (column, ty) in paint.columns.iter().enumerate() {
        let left = grid.column_left(rect.left, column);
        let center = left + grid.column_width() / 2;
        if paint.focus == Some(column) {
            canvas.rect(
                Rect::new(left, rect.top, grid.column_width(), rect.height),
                ACCENT.with_alpha(25),
            );
        }
        let (first, last) = paint.spans.get(column).copied().unwrap_or((0, 0));
        if first <= paint.at && paint.at <= last {
            let from = if first == paint.at {
                rect.top + rect.height / 2
            } else {
                rect.top
            };
            let to = if last == paint.at {
                rect.top + rect.height / 2
            } else {
                rect.bottom()
            };
            canvas.rect(
                Rect::new(center, from, Px::new(2), to - from),
                ACCENT.with_alpha(90),
            );
        }
        let Some(marks) = paint.lane.marks.get(ty) else {
            continue;
        };
        let Some(mark) = Mark::strongest(marks) else {
            continue;
        };
        let color = mark_color(mark);
        let cell = Rect::new(
            left + Px::new(3),
            rect.top + Px::new(2),
            grid.column_width() - Px::new(6),
            rect.height - Px::new(4),
        );
        canvas.rect(cell, PANEL);
        canvas.rect(cell, color.with_alpha(60));
        canvas.outline(cell, Px::new(1), color.with_alpha(160));
        let text = if marks.len() > 1 {
            format!("{}+", mark.name())
        } else {
            mark.name().to_owned()
        };
        let width = grid.cell * Count::new(text.chars().count());
        canvas.text(Point::new(center - width / 2, top), grid.size, text, TEXT);
    }
    canvas.pop_clip();
}

fn footer(frame: &mut Frame<'_>, types: &TypeModel, layers: &Layers, lane: &Lane) {
    frame.start(Container::Toolbar);
    let mut runs = vec![Run::new(format!("{} {}  ", lane.number, lane.symbol), TEXT)];
    for (ty, marks) in &lane.marks {
        let Some(node) = types.types.get(*ty) else {
            continue;
        };
        let names: Vec<&str> = marks.iter().map(|mark| mark.name()).collect();
        runs.push(Run::new(
            node.name.clone(),
            crate_color(layers, &node.krate),
        ));
        runs.push(Run::new(format!(" {}   ", names.join("/")), WEAK));
    }
    frame.caption(runs);
    frame.finish();
}

pub(super) fn data_view(model: &Model, frame: &mut Frame<'_>) {
    let Some(path) = model.nav.tour() else {
        frame.label(
            "open a tour to see the data its steps create, change and pass on",
            WEAK,
        );
        return;
    };
    let types = model.derived.types(&model.index, &model.map);
    let layers = model.derived.layers(&model.index, &model.map);
    let data: Flow = flow(model, &types, path, model.data.columns);
    let cell = frame.cell_width();
    let row = frame.row_height();
    let size = frame.metrics.font;
    let scroll_id = ids::data();
    let pointer = frame.ui.pointer();
    let previous = frame.ui.interaction(scroll_id);
    let mut grid = Grid {
        cell,
        size,
        across: Px::ZERO,
        columns: data.columns.len(),
    };
    let visible = previous.rect().map_or(grid.total(), |rect| rect.width);
    let mut across = model.across.get(scroll_id);
    if previous.hovered() {
        across -= pointer.wheel.horizontal.truncate();
        if keys::scrolls_across(pointer) {
            across -= pointer.wheel.vertical.truncate();
        }
    }
    let across = across.clamp(Px::ZERO, (grid.total() - visible).max(Px::ZERO));
    grid.across = across;
    frame.push(Action::ScrollAcross(scroll_id, across));
    let focus_column = model
        .types
        .focus
        .and_then(|symbol| types.type_of.get(&symbol))
        .and_then(|ty| data.columns.iter().position(|column| column == ty));

    frame.start(Container::Toolbar);
    let label = match model.data.columns {
        Columns::Shared => format!("types: in 2+ steps ({} hidden)", data.hidden),
        Columns::All => "types: all".to_owned(),
    };
    if frame
        .small_button(label, ids::DATA_SHARED.target())
        .clicked()
    {
        frame.push(Action::Data(DataAction::TurnColumns));
    }
    let mut legend = vec![Run::new("each step's signature and literals:  ", WEAK)];
    for mark in [
        Mark::Defines,
        Mark::Access(crate::types::Access::Construct),
        Mark::Access(crate::types::Access::Read),
        Mark::Access(crate::types::Access::Mutate),
        Mark::Access(crate::types::Access::Consume),
        Mark::Access(crate::types::Access::Return),
        Mark::Access(crate::types::Access::Lend),
    ] {
        legend.push(Run::new(format!("{} ", mark.name()), mark_color(mark)));
    }
    legend.push(Run::new(
        "  click a type to follow it in Types, a row to select the step",
        WEAK,
    ));
    frame.caption(legend);
    frame.finish();

    let names: Vec<(String, ui::Color)> = data
        .columns
        .iter()
        .filter_map(|ty| types.types.get(*ty))
        .map(|node| (node.name.clone(), crate_color(&layers, &node.krate)))
        .collect();
    let header = frame.custom(
        move |canvas, rect| paint_header(canvas, rect, grid, &names, focus_column, row),
        Size::Grow,
        row * i32::try_from(HEADER_LINES).unwrap_or(3) + Px::new(8),
        Some(ids::DATA_HEADER.id()),
    );
    if header.clicked() {
        let origin = header.rect().map_or(Px::ZERO, |rect| rect.left);
        if let Some(node) = grid
            .column_at(origin, pointer.mouse.horizontal)
            .and_then(|column| data.columns.get(column))
            .and_then(|ty| types.types.get(*ty))
        {
            frame.push(Action::Types(TypesAction::Focus(node.symbol)));
            frame.push(Action::Focus(node.symbol));
        }
    }
    frame.scroll_column(
        scroll_id,
        model.scrolls.get(scroll_id),
        Scroller::Plain,
        Some(BACKGROUND),
    );
    let chosen = model.nav.step_key();
    let mut hovered_lane = None;
    for (at, lane) in data.lanes.iter().enumerate() {
        let target = ids::DATA_ROW.nth(Count::new(at));
        let hovered = frame.ui.interaction(target.id()).hovered();
        if hovered {
            hovered_lane = Some(at);
        }
        let paint = LanePaint {
            lane: lane.clone(),
            columns: data.columns.clone(),
            spans: data.spans.clone(),
            at,
            chosen: chosen == Some(lane.key),
            hovered,
            focus: focus_column,
        };
        let interaction = frame.custom(
            move |canvas, rect| paint_lane(canvas, rect, grid, &paint),
            Size::Grow,
            row + Px::new(6),
            Some(target.id()),
        );
        if interaction.clicked() {
            frame.push(Action::SelectStep(lane.key, Scrolling::Scroll));
            let origin = interaction.rect().map_or(Px::ZERO, |rect| rect.left);
            if let Some(node) = grid
                .column_at(origin, pointer.mouse.horizontal)
                .and_then(|column| data.columns.get(column))
                .filter(|ty| lane.marks.contains_key(ty))
                .and_then(|ty| types.types.get(*ty))
            {
                frame.push(Action::Types(TypesAction::Focus(node.symbol)));
            }
        }
    }
    frame.spacer(row);
    frame.finish();
    let shown = hovered_lane
        .and_then(|at| data.lanes.get(at))
        .or_else(|| data.lanes.iter().find(|lane| Some(lane.key) == chosen));
    if let Some(lane) = shown {
        footer(frame, &types, &layers, lane);
    }
}
