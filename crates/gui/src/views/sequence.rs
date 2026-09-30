use ui::{Canvas, Color, Coordinate, Count, FontSize, Point, Px, Rect, Run, Size, Vector};

use crate::action::Action;
use crate::ids;
use crate::keys;
use crate::model::{Model, TourSlot};
use crate::nav::Scrolling;
use crate::sequence::{
    ACTOR, Calls, Diagram, Lifelines, Message, SequenceAction, diagram, link_keys,
};
use crate::theme::{
    ACCENT, BACKGROUND, BORDER, GREEN, HOVER, PANEL, RED, SELECTED, STEP_EDGE, TEXT, WEAK,
};
use crate::widgets::{Container, Frame, Padding, Scroller};

const BAR: i32 = 8;
const SHIFT: i32 = 5;
const HEAD: f32 = 6.0;
const MARGIN: i32 = 8;
const MIN_CELLS: usize = 16;
const MAX_CELLS: usize = 30;
const LIFELINE: Color = Color::rgba(70, 70, 78, 255);
const BAR_FILL: Color = Color::rgba(44, 58, 48, 255);
const BAR_EDGE: Color = GREEN.with_alpha(150);
const LINKED: Color = Color::rgba(170, 130, 230, 255);

#[derive(Clone, Debug)]
struct Columns {
    centers: Vec<Px>,
    widths: Vec<Px>,
    total: Px,
}

fn columns(diagram: &Diagram, cell: Px) -> Columns {
    let mut centers = Vec::new();
    let mut widths = Vec::new();
    let mut left = Px::new(MARGIN);
    for participant in &diagram.participants {
        let cells = (participant.name.chars().count() + 8).clamp(MIN_CELLS, MAX_CELLS);
        let width = cell * Count::new(cells);
        centers.push(left + width / 2);
        widths.push(width);
        left += width;
    }
    Columns {
        centers,
        widths,
        total: left + Px::new(MARGIN),
    }
}

#[derive(Clone, Copy, Debug)]
struct Bar {
    participant: usize,
    level: usize,
    starts: bool,
    ends: bool,
    chosen: bool,
}

#[derive(Clone, Debug)]
struct RowPaint {
    message: Message,
    bars: Vec<Bar>,
    centers: Vec<Px>,
    across: Px,
    cell: Px,
    row: Px,
    size: FontSize,
    chosen: bool,
    hovered: bool,
}

fn point(horizontal: Px, vertical: Px) -> Vector {
    Vector::new(Coordinate::of_px(horizontal), Coordinate::of_px(vertical))
}

fn level_px(level: usize) -> Px {
    Px::new(SHIFT * i32::try_from(level).unwrap_or(0))
}

fn clipped(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_owned();
    }
    let kept: String = text.chars().take(room.saturating_sub(1)).collect();
    format!("{kept}\u{2026}")
}

fn arrow_head(canvas: &mut Canvas<'_>, tip: Vector, leftward: bool, color: Color) {
    let back = if leftward { HEAD } else { -HEAD };
    let across = HEAD * 0.7;
    let base = |sign: f32| {
        Vector::new(
            Coordinate::new(tip.horizontal.get() + back),
            Coordinate::new(tip.vertical.get() + sign * across),
        )
    };
    canvas.line(tip, base(1.0), Coordinate::new(1.5), color);
    canvas.line(tip, base(-1.0), Coordinate::new(1.5), color);
}

fn paint_row(canvas: &mut Canvas<'_>, rect: Rect, paint: &RowPaint) {
    let left = rect.left - paint.across;
    let x = |participant: usize| left + paint.centers.get(participant).copied().unwrap_or_default();
    if paint.chosen {
        canvas.rect(rect, SELECTED);
    } else if paint.hovered {
        canvas.rect(rect, HOVER);
    }
    for (participant, _) in paint.centers.iter().enumerate() {
        let center = x(participant);
        let color = if participant == ACTOR {
            ACCENT.with_alpha(90)
        } else {
            LIFELINE
        };
        canvas.rect(Rect::new(center, rect.top, Px::new(1), rect.height), color);
    }
    let arrow_y = rect.top + paint.row + paint.row / 2 + Px::new(2);
    for bar in &paint.bars {
        let center = x(bar.participant) + level_px(bar.level);
        let top = if bar.starts {
            arrow_y - Px::new(3)
        } else {
            rect.top
        };
        let bottom = if bar.ends {
            rect.bottom() - Px::new(4)
        } else {
            rect.bottom()
        };
        let body = Rect::new(center - Px::new(BAR / 2), top, Px::new(BAR), bottom - top);
        canvas.rect(body, if bar.chosen { SELECTED } else { BAR_FILL });
        let edge = if bar.chosen { ACCENT } else { BAR_EDGE };
        canvas.rect(Rect::new(body.left, top, Px::new(1), body.height), edge);
        canvas.rect(
            Rect::new(body.right() - Px::new(1), top, Px::new(1), body.height),
            edge,
        );
        if bar.starts {
            canvas.rect(Rect::new(body.left, top, body.width, Px::new(1)), edge);
        }
        if bar.ends {
            canvas.rect(
                Rect::new(body.left, bottom - Px::new(1), body.width, Px::new(1)),
                edge,
            );
        }
    }
    let message = &paint.message;
    let color = if paint.chosen {
        ACCENT
    } else if message.stale {
        RED
    } else if message.via.is_some() {
        LINKED
    } else {
        STEP_EDGE
    };
    let half = Px::new(BAR / 2);
    let from_x = x(message.from);
    let to_x = x(message.to);
    let width = Coordinate::new(if paint.chosen { 2.0 } else { 1.5 });
    let label_left;
    let room;
    let right_end;
    if message.from == message.to {
        let start = from_x + half + level_px(message.from_level);
        let end = to_x + half + level_px(message.level);
        let reach = start.max(end) + paint.cell * Count::new(2);
        let upper = arrow_y - Px::new(7);
        canvas.line(point(start, upper), point(reach, upper), width, color);
        canvas.line(point(reach, upper), point(reach, arrow_y), width, color);
        canvas.line(point(reach, arrow_y), point(end, arrow_y), width, color);
        arrow_head(canvas, point(end, arrow_y), true, color);
        label_left = start + Px::new(4);
        right_end = reach;
        room = 28;
    } else {
        let rightward = to_x > from_x;
        let (start, end) = if message.from == ACTOR {
            let end = if rightward {
                to_x - half + level_px(message.level)
            } else {
                to_x + half + level_px(message.level)
            };
            (from_x, end)
        } else if rightward {
            (
                from_x + half + level_px(message.from_level),
                to_x - half + level_px(message.level),
            )
        } else {
            (
                from_x - half + level_px(message.from_level),
                to_x + half + level_px(message.level),
            )
        };
        canvas.line(point(start, arrow_y), point(end, arrow_y), width, color);
        arrow_head(canvas, point(end, arrow_y), !rightward, color);
        label_left = start.min(end) + Px::new(6);
        right_end = start.max(end);
        let span = (end - start).absolute().get() / paint.cell.get().max(1);
        room = usize::try_from(span - 1).unwrap_or(0).max(24);
    }
    let top = Point::new(label_left, rect.top + Px::new(3));
    if let Some(unfolded) = message.unfolded {
        let (mark, color) = if unfolded {
            ("[-]", LINKED)
        } else {
            ("[+]", LINKED)
        };
        canvas.text(
            Point::new(rect.left - paint.across + Px::new(4), top.vertical),
            paint.size,
            mark,
            color,
        );
    }
    let number = format!("{} ", message.number);
    let used = number.chars().count();
    let name_color = if message.stale {
        RED
    } else if paint.chosen {
        TEXT
    } else if message.via.is_some() {
        LINKED
    } else {
        TEXT
    };
    let name = clipped(&message.symbol, room.saturating_sub(used).max(8));
    let link_text = message
        .link
        .as_ref()
        .map_or_else(String::new, |link| format!("  \u{2192} {link}"));
    let chars = used + name.chars().count() + link_text.chars().count();
    let label_width = paint.cell * Count::new(chars);
    let label_left = if label_left + label_width > rect.right() - Px::new(4) {
        (right_end.min(rect.right()) - label_width - Px::new(10)).max(rect.left + Px::new(4))
    } else {
        label_left
    };
    let backdrop = if paint.chosen {
        SELECTED
    } else if paint.hovered {
        HOVER
    } else {
        BACKGROUND.with_alpha(200)
    };
    canvas.rect(
        Rect::new(
            label_left - Px::new(2),
            top.vertical - Px::new(1),
            paint.cell * Count::new(chars) + Px::new(4),
            paint.row + Px::new(1),
        ),
        backdrop,
    );
    let pen = canvas.text(
        Point::new(label_left, top.vertical),
        paint.size,
        number,
        WEAK,
    );
    let pen = canvas.text(Point::new(pen, top.vertical), paint.size, name, name_color);
    if !link_text.is_empty() {
        canvas.text(Point::new(pen, top.vertical), paint.size, link_text, LINKED);
    }
}

fn header_paint(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    names: &[String],
    columns: &Columns,
    across: Px,
    cell: Px,
    size: FontSize,
) {
    canvas.rect(rect, BACKGROUND);
    let left = rect.left - across;
    for (participant, name) in names.iter().enumerate() {
        let center = left
            + columns
                .centers
                .get(participant)
                .copied()
                .unwrap_or_default();
        let column = columns.widths.get(participant).copied().unwrap_or_default();
        let room = usize::try_from((column.get() / cell.get().max(1)) - 3).unwrap_or(4);
        let shown = clipped(name, room);
        let text_width = cell * Count::new(shown.chars().count());
        let width = text_width + cell * Count::new(2);
        let body = Rect::new(
            center - width / 2,
            rect.top + Px::new(4),
            width,
            rect.height - Px::new(8),
        );
        let (fill, edge, ink) = if participant == ACTOR {
            (ACCENT.with_alpha(40), ACCENT, TEXT)
        } else {
            (PANEL, BORDER, TEXT)
        };
        canvas.rect(body, fill);
        canvas.outline(body, Px::new(1), edge);
        canvas.text(
            Point::new(
                center - text_width / 2,
                body.top + (body.height - cell * Count::new(2)).max(Px::ZERO) / 2 + Px::new(1),
            ),
            size,
            shown,
            ink,
        );
        canvas.rect(
            Rect::new(
                center,
                body.bottom(),
                Px::new(1),
                rect.bottom() - body.bottom(),
            ),
            if participant == ACTOR {
                ACCENT.with_alpha(90)
            } else {
                LIFELINE
            },
        );
    }
}

fn toolbar(model: &Model, frame: &mut Frame<'_>, diagram: &Diagram, root: TourSlot) {
    frame.start(Container::Toolbar);
    let state = &model.sequence;
    if frame
        .small_button(
            match state.lifelines {
                Lifelines::Auto => format!("lifelines: auto ({})", diagram.unit.name()),
                Lifelines::Chosen(unit) => format!("lifelines: {}", unit.name()),
            },
            ids::SEQUENCE_UNIT.target(),
        )
        .clicked()
    {
        frame.push(Action::Sequence(SequenceAction::TurnUnit));
    }
    let folded = diagram
        .messages
        .iter()
        .filter(|message| message.unfolded == Some(false))
        .count();
    if frame
        .small_button(
            format!("inline links ({folded})"),
            ids::SEQUENCE_LINKS.target(),
        )
        .clicked()
    {
        frame.push(Action::Sequence(SequenceAction::UnfoldAll(link_keys(
            model, root,
        ))));
    }
    if !state.expanded.is_empty()
        && frame
            .small_button("un-inline links", ids::SEQUENCE_FOLD.target())
            .clicked()
    {
        frame.push(Action::Sequence(SequenceAction::FoldAll));
    }
    let calls = match state.calls {
        Calls::All => "calls: all",
        Calls::Crossing => "calls: crossing a lifeline",
    };
    if frame
        .small_button(calls, ids::SEQUENCE_CALLS.target())
        .clicked()
    {
        frame.push(Action::Sequence(SequenceAction::TurnCalls));
    }
    let crossings = diagram
        .messages
        .iter()
        .filter(|message| message.from != message.to && message.from != ACTOR)
        .count();
    frame.caption(vec![
        Run::new(
            format!(
                "{} lifelines, {} calls, {} cross a boundary   ",
                diagram.participants.len().saturating_sub(1),
                diagram.messages.len(),
                crossings
            ),
            WEAK,
        ),
        Run::new("\u{2500} step  ", GREEN),
        Run::new("\u{2500} linked tour  ", LINKED),
        Run::new("\u{2500} stale  ", RED),
        Run::new(
            "click a call to select its step, [+] to inline a linked tour; shift+wheel scrolls sideways",
            WEAK,
        ),
    ]);
    frame.finish();
}

fn footer(frame: &mut Frame<'_>, message: &Message) {
    frame.start(Container::Toolbar);
    let mut runs = vec![
        Run::new(format!("{}  ", message.number), WEAK),
        Run::new(format!("{}  ", message.symbol), TEXT),
        Run::new(message.place.clone(), WEAK),
    ];
    if let Some(via) = &message.via {
        runs.push(Run::new(format!("   in {via}"), LINKED));
    }
    if let Some(link) = &message.link {
        runs.push(Run::new(format!("   \u{2192} {link}"), LINKED));
    }
    frame.caption(runs);
    frame.finish();
    if !message.note.is_empty() {
        frame.note(message.note.clone(), GREEN, Padding::Tour);
    }
}

pub(super) fn sequence_view(model: &Model, frame: &mut Frame<'_>) {
    let Some(path) = model.nav.tour() else {
        frame.label("open a tour to see it as a sequence of calls", WEAK);
        return;
    };
    let step_key = model.nav.step_key();
    let held = model
        .sequence
        .root
        .filter(|root| *root != path)
        .map(|root| (root, diagram(model, root, &model.sequence)))
        .filter(|(_, held)| {
            held.messages
                .iter()
                .any(|message| Some(message.key) == step_key)
        });
    let (root, diagram) = match held {
        Some(found) => found,
        None => {
            if model.sequence.root != Some(path) {
                frame.push(Action::Sequence(SequenceAction::Root(path)));
            }
            (path, diagram(model, path, &model.sequence))
        }
    };
    toolbar(model, frame, &diagram, root);
    if root != path {
        frame.start(Container::Toolbar);
        let outer = model
            .tour(root)
            .map_or_else(String::new, |found| found.name().to_string());
        let inner = model
            .tour(path)
            .map_or_else(String::new, |found| found.name().to_string());
        frame.caption(vec![Run::new(
            format!("reading tour {inner} inside tour {outer}  "),
            LINKED,
        )]);
        if frame
            .small_button(
                format!("show tour {inner} alone"),
                ids::SEQUENCE_ALONE.target(),
            )
            .clicked()
        {
            frame.push(Action::Sequence(SequenceAction::Root(path)));
        }
        frame.finish();
    }
    if diagram.messages.is_empty() {
        frame.label("this tour has no steps", WEAK);
        return;
    }
    let cell = frame.cell_width();
    let row = frame.row_height();
    let size = frame.metrics.font;
    let columns = columns(&diagram, cell);
    let scroll_id = ids::sequence();
    let pointer = frame.ui.pointer();
    let previous = frame.ui.interaction(scroll_id);
    let visible = previous.rect().map_or(columns.total, |rect| rect.width);
    let mut across = model.across.get(scroll_id);
    if previous.hovered() {
        across -= pointer.wheel.horizontal.truncate();
        if keys::scrolls_across(pointer) {
            across -= pointer.wheel.vertical.truncate();
        }
    }
    let across = across.clamp(Px::ZERO, (columns.total - visible).max(Px::ZERO));
    frame.push(Action::ScrollAcross(scroll_id, across));
    let names: Vec<String> = diagram
        .participants
        .iter()
        .map(|participant| {
            if participant.steps > 0 {
                format!("{} \u{b7}{}", participant.name, participant.steps)
            } else {
                participant.name.clone()
            }
        })
        .collect();
    let header_columns = columns.clone();
    frame.custom(
        move |canvas, rect| header_paint(canvas, rect, &names, &header_columns, across, cell, size),
        Size::Grow,
        row + Px::new(12),
        Some(ids::SEQUENCE_HEADER.id()),
    );
    let chosen_step = model.nav.step();
    let chosen_path = model.nav.tour();
    let is_chosen = |message: &Message| {
        Some(message.key.step) == chosen_step && Some(message.key.tour) == chosen_path
    };
    let row_height = row * 2 + Px::new(6);
    let mut offset = model.scrolls.get(scroll_id);
    let chosen_key = diagram
        .messages
        .iter()
        .position(|message| is_chosen(message));
    if let Some(at) = chosen_key
        && let Some(message) = diagram.messages.get(at)
        && model.sequence.followed != Some(message.key)
    {
        frame.push(Action::Sequence(SequenceAction::Followed(message.key)));
        let height = previous.rect().map_or(Px::new(400), |rect| rect.height);
        let top = row_height * i32::try_from(at).unwrap_or(0);
        if top < offset || top + row_height > offset + height {
            offset = (top - height / 3).max(Px::ZERO);
        }
    }
    frame.scroll_column(scroll_id, offset, Scroller::Plain, Some(BACKGROUND));
    let mut hovered_row = None;
    for (at, message) in diagram.messages.iter().enumerate() {
        let target = ids::SEQUENCE_ROW.nth(Count::new(at));
        let hovered = frame.ui.interaction(target.id()).hovered();
        if hovered {
            hovered_row = Some(at);
        }
        let bars: Vec<Bar> = diagram
            .messages
            .iter()
            .enumerate()
            .filter(|(start, open)| *start <= at && open.end >= at)
            .map(|(start, open)| Bar {
                participant: open.to,
                level: open.level,
                starts: start == at,
                ends: open.end == at,
                chosen: is_chosen(open),
            })
            .collect();
        let paint = RowPaint {
            message: message.clone(),
            bars,
            centers: columns.centers.clone(),
            across,
            cell,
            row,
            size,
            chosen: is_chosen(message),
            hovered,
        };
        let clicked = frame
            .custom(
                move |canvas, rect| paint_row(canvas, rect, &paint),
                Size::Grow,
                row_height,
                Some(target.id()),
            )
            .clicked();
        if clicked {
            let gutter = frame
                .ui
                .interaction(target.id())
                .rect()
                .is_some_and(|rect| {
                    pointer.mouse.horizontal < rect.left - across + cell * Count::new(4)
                });
            if gutter && message.unfolded.is_some() {
                frame.push(Action::Sequence(SequenceAction::ToggleLink(message.key)));
            } else {
                frame.push(Action::SelectStep(message.key, Scrolling::Scroll));
            }
        }
    }
    frame.spacer(row * 2);
    frame.finish();
    let shown = hovered_row
        .and_then(|at| diagram.messages.get(at))
        .or_else(|| diagram.messages.iter().find(|message| is_chosen(message)));
    if let Some(message) = shown {
        footer(frame, message);
    }
}
