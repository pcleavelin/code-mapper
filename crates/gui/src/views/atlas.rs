use domain::TourKind;
use ui::{Canvas, Color, Count, FontSize, Point, Px, Rect, Run, Size};

use crate::action::Action;
use crate::atlas::{Atlas, AtlasAction, Coverage, Entry, Line, Order, Relation, atlas};
use crate::ids;
use crate::keys;
use crate::model::{Model, Tab};
use crate::nav::Scrolling;
use crate::theme::{
    ACCENT, BACKGROUND, BORDER, GREEN, HOVER, ORANGE, PANEL, RED, SELECTED, TEXT, WEAK,
};
use crate::widgets::{Container, Frame, Scroller};

const NAME_CELLS: usize = 34;
const UNIT_CELLS: usize = 5;
const HEADER_LINES: usize = 3;
const SHARE_CELLS: usize = 18;
const LINKED: Color = Color::rgba(170, 130, 230, 255);

#[derive(Clone, Copy, Debug)]
struct Grid {
    cell: Px,
    size: FontSize,
    across: Px,
    units: usize,
}

impl Grid {
    fn name_width(self) -> Px {
        self.cell * Count::new(NAME_CELLS)
    }

    fn unit_width(self) -> Px {
        self.cell * Count::new(UNIT_CELLS)
    }

    fn fixed_width(self) -> Px {
        self.cell * Count::new(NAME_CELLS + SHARE_CELLS)
    }

    fn unit_left(self, origin: Px, at: usize) -> Px {
        origin + self.fixed_width() + self.unit_width() * Count::new(at) - self.across
    }

    fn share_left(self, origin: Px) -> Px {
        origin + self.name_width()
    }

    fn total(self) -> Px {
        self.fixed_width() + self.unit_width() * Count::new(self.units)
    }

    fn column_at(self, origin: Px, horizontal: Px) -> Option<usize> {
        let offset = horizontal - (origin + self.fixed_width() - self.across);
        if offset < Px::ZERO {
            return None;
        }
        let at = usize::try_from(offset.get() / self.unit_width().get().max(1)).ok()?;
        (at < self.units).then_some(at)
    }
}

fn clipped(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_owned();
    }
    let kept: String = text.chars().take(room.saturating_sub(1)).collect();
    format!("{kept}\u{2026}")
}

fn centered(
    canvas: &mut Canvas<'_>,
    grid: Grid,
    left: Px,
    width: Px,
    top: Px,
    text: &str,
    color: Color,
) {
    let text_width = grid.cell * Count::new(text.chars().count());
    canvas.text(
        Point::new(left + (width - text_width) / 2, top),
        grid.size,
        text.to_owned(),
        color,
    );
}

fn heat(count: usize) -> Color {
    let alpha = (30 + count * 22).min(210);
    GREEN.with_alpha(u8::try_from(alpha).unwrap_or(210))
}

fn coverage_color(covered: usize, total: usize) -> Color {
    if total == 0 {
        return WEAK;
    }
    let share = covered * 100 / total;
    if share >= 75 {
        GREEN
    } else if share >= 40 {
        ORANGE
    } else {
        RED
    }
}

fn paint_header(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    grid: Grid,
    units: &[String],
    levels: &[usize],
    chosen: Option<usize>,
    row: Px,
) {
    canvas.rect(rect, BACKGROUND);
    canvas.push_clip(Rect::new(
        rect.left + grid.fixed_width(),
        rect.top,
        (rect.width - grid.fixed_width()).max(Px::ZERO),
        rect.height,
    ));
    for (at, name) in units.iter().enumerate() {
        let left = grid.unit_left(rect.left, at);
        if chosen == Some(at) {
            canvas.rect(
                Rect::new(left, rect.top, grid.unit_width(), rect.height),
                SELECTED,
            );
        }
        if at > 0 && levels.get(at) != levels.get(at - 1) {
            canvas.rect(
                Rect::new(left, rect.top, Px::new(1), rect.height),
                ACCENT.with_alpha(120),
            );
        }
        let line = row * i32::try_from(at % HEADER_LINES).unwrap_or(0) + Px::new(4);
        let shown = clipped(name, UNIT_CELLS * HEADER_LINES - 1);
        let text_width = grid.cell * Count::new(shown.chars().count());
        let center = left + grid.unit_width() / 2;
        canvas.text(
            Point::new(
                (center - text_width / 2).max(rect.left + grid.fixed_width() + Px::new(2)),
                rect.top + line,
            ),
            grid.size,
            shown,
            if chosen == Some(at) { ACCENT } else { TEXT },
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
    canvas.text(
        Point::new(rect.left + Px::new(4), rect.top + row * 2 + Px::new(4)),
        grid.size,
        "tour",
        WEAK,
    );
    let share_left = grid.share_left(rect.left);
    canvas.text(
        Point::new(share_left + Px::new(6), rect.top + row * 2 + Px::new(4)),
        grid.size,
        "vs selected",
        WEAK,
    );
}

fn kind_name(kind: TourKind) -> &'static str {
    match kind {
        TourKind::Flow => "flow",
        TourKind::Layer => "layer",
        TourKind::Data => "data",
    }
}

fn paint_line(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    grid: Grid,
    line: &Line,
    chosen: bool,
    hovered: bool,
) {
    if chosen {
        canvas.rect(rect, SELECTED);
    } else if hovered {
        canvas.rect(rect, HOVER);
    }
    let top = rect.top + Px::new(2);
    let indent = grid.cell * Count::new(2 * line.depth + 1);
    let kind = kind_name(line.kind);
    let stale = if line.stale > 0 {
        format!(" !{}", line.stale)
    } else {
        String::new()
    };
    let share_left = grid.share_left(rect.left) + Px::new(6);
    let arrow = match line.relation {
        Relation::Itself => "selected",
        Relation::LinksTo => "caller",
        Relation::LinkedFrom => "callee",
        Relation::Both => "both ways",
        Relation::None => "",
    };
    let mark = match (arrow.is_empty(), line.shared) {
        (_, 0) => arrow.to_owned(),
        (true, count) => shared(count),
        (false, count) => format!("{arrow}, {}", shared(count)),
    };
    let color = match line.relation {
        Relation::Itself => ACCENT,
        Relation::None => ORANGE,
        _ => LINKED,
    };
    canvas.text(Point::new(share_left, top), grid.size, mark, color);
    let room = NAME_CELLS.saturating_sub(2 * line.depth + 2 + kind.len() + 1 + stale.len());
    let name = clipped(&line.name, room);
    let mut pen = canvas.text(Point::new(rect.left + indent, top), grid.size, name, TEXT);
    pen = canvas.text(Point::new(pen + grid.cell, top), grid.size, kind, WEAK);
    if !stale.is_empty() {
        canvas.text(Point::new(pen, top), grid.size, stale, RED);
    }
    canvas.push_clip(Rect::new(
        rect.left + grid.fixed_width(),
        rect.top,
        (rect.width - grid.fixed_width()).max(Px::ZERO),
        rect.height,
    ));
    for (at, count) in line.counts.iter().enumerate() {
        let left = grid.unit_left(rect.left, at);
        let cell = Rect::new(
            left + Px::new(1),
            rect.top + Px::new(1),
            grid.unit_width() - Px::new(2),
            rect.height - Px::new(2),
        );
        if *count == 0 {
            canvas.rect(
                Rect::new(
                    cell.left + cell.width / 2,
                    cell.top + cell.height / 2,
                    Px::new(1),
                    Px::new(1),
                ),
                BORDER,
            );
            continue;
        }
        canvas.rect(cell, heat(*count));
        centered(
            canvas,
            grid,
            left,
            grid.unit_width(),
            top,
            &count.to_string(),
            TEXT,
        );
    }
    canvas.pop_clip();
}

fn shared(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        format!("{count} shared")
    }
}

fn paint_group(canvas: &mut Canvas<'_>, rect: Rect, grid: Grid, name: &str, depth: usize) {
    canvas.rect(
        Rect::new(
            rect.left,
            rect.bottom() - Px::new(1),
            rect.width,
            Px::new(1),
        ),
        BORDER,
    );
    let indent = grid.cell * Count::new(2 * depth);
    canvas.text(
        Point::new(rect.left + indent, rect.top + Px::new(3)),
        grid.size,
        format!("{name}/"),
        ACCENT,
    );
}

fn paint_footer(canvas: &mut Canvas<'_>, rect: Rect, grid: Grid, atlas: &Atlas, row: Px) {
    canvas.rect(rect, PANEL);
    canvas.rect(
        Rect::new(rect.left, rect.top, rect.width, Px::new(1)),
        BORDER,
    );
    let lines = [
        (Px::new(4), "symbols in a tour"),
        (row + Px::new(6), "tours through it"),
    ];
    for (offset, label) in lines {
        canvas.text(
            Point::new(rect.left + grid.cell, rect.top + offset),
            grid.size,
            label,
            WEAK,
        );
    }
    canvas.push_clip(Rect::new(
        rect.left + grid.fixed_width(),
        rect.top,
        (rect.width - grid.fixed_width()).max(Px::ZERO),
        rect.height,
    ));
    for (at, Coverage { covered, total }) in atlas.coverage.iter().enumerate() {
        let left = grid.unit_left(rect.left, at);
        let text = if *total == 0 {
            "-".to_owned()
        } else {
            format!("{}%", covered * 100 / total)
        };
        centered(
            canvas,
            grid,
            left,
            grid.unit_width(),
            rect.top + Px::new(4),
            &text,
            coverage_color(*covered, *total),
        );
        let paths = atlas.paths_per_unit.get(at).copied().unwrap_or(0);
        centered(
            canvas,
            grid,
            left,
            grid.unit_width(),
            rect.top + row + Px::new(6),
            &paths.to_string(),
            if paths == 0 { RED } else { TEXT },
        );
    }
    canvas.pop_clip();
}

pub(super) fn atlas_view(model: &Model, frame: &mut Frame<'_>) {
    let atlas = model.derived.atlas(
        &model.index,
        &model.map,
        model.nav.tour(),
        model.atlas.clone(),
        |layers| atlas(model, &model.atlas, layers),
    );
    let cell = frame.cell_width();
    let row = frame.row_height();
    let size = frame.metrics.font;
    let scroll_id = ids::atlas();
    let pointer = frame.ui.pointer();
    let previous = frame.ui.interaction(scroll_id);
    let mut grid = Grid {
        cell,
        size,
        across: Px::ZERO,
        units: atlas.units.len(),
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

    frame.start(Container::Toolbar);
    let order = match model.atlas.order {
        Order::List => "order: tours list",
        Order::Shared => "order: shared with selected",
    };
    if frame
        .small_button(order, ids::ATLAS_ORDER.target())
        .clicked()
    {
        frame.push(Action::Atlas(AtlasAction::TurnOrder));
    }
    if let Some(column) = &model.atlas.column {
        if frame
            .small_button(
                format!("only through {column} x"),
                ids::ATLAS_HEADER.target(),
            )
            .clicked()
        {
            frame.push(Action::Atlas(AtlasAction::Column(column.clone())));
        }
    }
    frame.caption(vec![
        Run::new(
            "crates in dependency order, top layer left; click a crate to keep its tours  ",
            WEAK,
        ),
        Run::new("shared: same symbols as the selected tour  ", ORANGE),
        Run::new(
            "click a cell: first step there; a name: select; double-click: read",
            WEAK,
        ),
    ]);
    frame.finish();

    let units = atlas.units.clone();
    let levels = atlas.levels.clone();
    let chosen = model
        .atlas
        .column
        .as_ref()
        .and_then(|name| units.iter().position(|unit| unit == name));
    let header = frame.custom(
        move |canvas, rect| paint_header(canvas, rect, grid, &units, &levels, chosen, row),
        Size::Grow,
        row * i32::try_from(HEADER_LINES).unwrap_or(2) + Px::new(8),
        Some(ids::ATLAS_HEADER.id()),
    );
    if header.clicked() {
        let origin = header.rect().map_or(Px::ZERO, |rect| rect.left);
        if let Some(name) = grid
            .column_at(origin, pointer.mouse.horizontal)
            .and_then(|column| atlas.units.get(column))
        {
            frame.push(Action::Atlas(AtlasAction::Column(name.clone())));
        }
    }
    frame.scroll_column(
        scroll_id,
        model.scrolls.get(scroll_id),
        Scroller::Plain,
        Some(BACKGROUND),
    );
    let selected = model.nav.tour();
    for entry in &atlas.entries {
        match entry {
            Entry::Group { name, depth } => {
                let name = name.clone();
                let depth = *depth;
                frame.custom(
                    move |canvas, rect| paint_group(canvas, rect, grid, &name, depth),
                    Size::Grow,
                    row + Px::new(6),
                    None,
                );
            }
            Entry::Path(line) => {
                let target = ids::ATLAS_ROW.nth(Count::new(line.slot.get()));
                let previous = frame.ui.interaction(target.id());
                let hovered = previous.hovered();
                let chosen = selected == Some(line.slot);
                let painted = line.clone();
                let interaction = frame.custom(
                    move |canvas, rect| paint_line(canvas, rect, grid, &painted, chosen, hovered),
                    Size::Grow,
                    row + Px::new(4),
                    Some(target.id()),
                );
                if interaction.double_clicked() {
                    frame.push(Action::OpenTour(line.slot, Tab::Tour));
                } else if interaction.clicked() {
                    let origin = interaction.rect().map_or(Px::ZERO, |rect| rect.left);
                    let key = grid
                        .column_at(origin, pointer.mouse.horizontal)
                        .and_then(|column| line.first.get(column).copied().flatten());
                    match key {
                        Some(key) => frame.push(Action::SelectStep(key, Scrolling::Scroll)),
                        None => frame.push(Action::OpenTour(line.slot, Tab::Atlas)),
                    }
                }
            }
        }
    }
    frame.spacer(row);
    frame.finish();
    frame.custom(
        move |canvas, rect| paint_footer(canvas, rect, grid, &atlas, row),
        Size::Grow,
        row * 2 + Px::new(10),
        None,
    );
}
