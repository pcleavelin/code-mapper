use std::collections::BTreeSet;
use std::rc::Rc;

use ui::{Canvas, Color, Coordinate, Count, FontSize, Point, Px, Rect, Run, Size, Vector};

use crate::action::Action;
use crate::derived::Layers;
use crate::ids;
use crate::model::Model;
use crate::theme::{
    ACCENT, BACKGROUND, BORDER, GREEN, HOVER, ORANGE, PANEL, RED, SELECTED, TEXT, WEAK,
};
use crate::types::{Access, FnIx, Neighbour, TypeIx, TypeModel, TypesAction};
use crate::widgets::{Chosen, Container, Frame, Scroller};

const SIDE_CELLS: usize = 34;
const CENTER_CELLS: usize = 34;
const LIST_ROWS: usize = 4;
const PALETTE: [Color; 8] = [
    Color::rgba(110, 170, 250, 255),
    Color::rgba(120, 210, 150, 255),
    Color::rgba(230, 180, 90, 255),
    Color::rgba(200, 140, 230, 255),
    Color::rgba(240, 130, 130, 255),
    Color::rgba(110, 210, 210, 255),
    Color::rgba(210, 210, 120, 255),
    Color::rgba(170, 170, 190, 255),
];

pub(crate) fn crate_color(layers: &Layers, name: &str) -> Color {
    let rank = layers.rank(name);
    PALETTE.get(rank % PALETTE.len()).copied().unwrap_or(TEXT)
}

pub(crate) fn access_color(access: Access) -> Color {
    match access {
        Access::Construct => GREEN,
        Access::Read => WEAK,
        Access::Mutate => ORANGE,
        Access::Consume => RED,
        Access::Return => ACCENT,
        Access::Lend => Color::rgba(120, 160, 200, 255),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    Type(TypeIx),
    Fn(FnIx),
}

#[derive(Clone, Debug)]
struct Item {
    rect: Rect,
    title: String,
    krate: String,
    color: Color,
    via: String,
    hit: Hit,
    via_hit: Option<Hit>,
    on_path: bool,
}

#[derive(Clone, Debug)]
enum Edge {
    Side {
        from: Point,
        to: Point,
        width: f32,
        color: Color,
    },
    Straight {
        from: Point,
        to: Point,
        color: Color,
    },
}

#[derive(Clone, Debug, Default)]
struct Compass {
    items: Vec<Item>,
    edges: Vec<Edge>,
    labels: Vec<(Point, String, Color)>,
    focus: Option<Item>,
    focus_lines: Vec<Vec<(String, Color)>>,
}

fn via_text(types: &TypeModel, via: &[FnIx]) -> String {
    let mut names: Vec<String> = Vec::new();
    for fn_ix in via {
        if let Some(node) = types.fns.get(*fn_ix) {
            let label = node.label();
            if !names.contains(&label) {
                names.push(label);
            }
        }
    }
    let shown: Vec<&String> = names.iter().take(2).collect();
    let mut text = shown
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > 2 {
        text.push_str(&format!(" +{}", names.len() - 2));
    }
    text
}

struct Ctx<'data> {
    types: &'data TypeModel,
    layers: &'data Layers,
    cell: Px,
    row: Px,
    path_types: &'data BTreeSet<TypeIx>,
    path_fns: &'data BTreeSet<FnIx>,
}

fn side_items(
    ctx: &Ctx<'_>,
    neighbours: &[Neighbour],
    left: Px,
    top: Px,
    width: Px,
    bottom: Px,
    compass: &mut Compass,
) -> Vec<Rect> {
    let height = ctx.row * 2 + Px::new(6);
    let step = height + Px::new(4);
    let room = usize::try_from(((bottom - top).get() / step.get().max(1)).max(0)).unwrap_or(0);
    let shown = if neighbours.len() > room {
        room.saturating_sub(1)
    } else {
        neighbours.len()
    };
    let mut rects = Vec::new();
    for (at, neighbour) in neighbours.iter().take(shown).enumerate() {
        let Some(node) = ctx.types.types.get(neighbour.ty) else {
            continue;
        };
        let rect = Rect::new(
            left,
            top + step * i32::try_from(at).unwrap_or(0),
            width,
            height,
        );
        rects.push(rect);
        compass.items.push(Item {
            rect,
            title: node.name.clone(),
            krate: node.krate.clone(),
            color: crate_color(ctx.layers, &node.krate),
            via: format!("via {}", via_text(ctx.types, &neighbour.via)),
            hit: Hit::Type(neighbour.ty),
            via_hit: neighbour.via.first().map(|fn_ix| Hit::Fn(*fn_ix)),
            on_path: ctx.path_types.contains(&neighbour.ty)
                || neighbour
                    .via
                    .iter()
                    .any(|fn_ix| ctx.path_fns.contains(fn_ix)),
        });
    }
    if neighbours.len() > shown {
        compass.labels.push((
            Point::new(
                left + Px::new(4),
                top + step * i32::try_from(shown).unwrap_or(0),
            ),
            format!("+{} more", neighbours.len() - shown),
            WEAK,
        ));
    }
    rects
}

fn list_items(
    ctx: &Ctx<'_>,
    list: &[TypeIx],
    left: Px,
    top: Px,
    width: Px,
    compass: &mut Compass,
) -> Vec<Rect> {
    let height = ctx.row + Px::new(4);
    let per_row = 2usize;
    let chip = (width - Px::new(4)) / 2;
    let mut rects = Vec::new();
    let most = LIST_ROWS * per_row;
    let shown = if list.len() > most {
        most - 1
    } else {
        list.len()
    };
    for (at, ty) in list.iter().take(shown).enumerate() {
        let Some(node) = ctx.types.types.get(*ty) else {
            continue;
        };
        let column = i32::try_from(at % per_row).unwrap_or(0);
        let line = i32::try_from(at / per_row).unwrap_or(0);
        let rect = Rect::new(
            left + (chip + Px::new(4)) * column,
            top + (height + Px::new(3)) * line,
            chip,
            height,
        );
        rects.push(rect);
        compass.items.push(Item {
            rect,
            title: node.name.clone(),
            krate: node.krate.clone(),
            color: crate_color(ctx.layers, &node.krate),
            via: String::new(),
            hit: Hit::Type(*ty),
            via_hit: None,
            on_path: ctx.path_types.contains(ty),
        });
    }
    if list.len() > shown {
        let column = i32::try_from(shown % per_row).unwrap_or(0);
        let line = i32::try_from(shown / per_row).unwrap_or(0);
        compass.labels.push((
            Point::new(
                left + (chip + Px::new(4)) * column + Px::new(4),
                top + (height + Px::new(3)) * line + Px::new(2),
            ),
            format!("+{} more", list.len() - shown),
            WEAK,
        ));
    }
    rects
}

fn sorted_by_layer(ctx: &Ctx<'_>, set: &BTreeSet<TypeIx>) -> Vec<TypeIx> {
    let mut list: Vec<TypeIx> = set.iter().copied().collect();
    list.sort_by_key(|ty| {
        ctx.types
            .types
            .get(*ty)
            .map_or((usize::MAX, String::new()), |node| {
                (ctx.layers.rank(&node.krate), node.name.clone())
            })
    });
    list
}

fn compass(ctx: &Ctx<'_>, focus: TypeIx, area: Rect) -> Compass {
    let mut out = Compass::default();
    let Some(node) = ctx.types.types.get(focus) else {
        return out;
    };
    let cell = ctx.cell;
    let row = ctx.row;
    let center_width = cell * Count::new(CENTER_CELLS);
    let side_width = (cell * Count::new(SIDE_CELLS))
        .min((area.width - center_width - cell * Count::new(12)) / 2)
        .max(cell * Count::new(14));
    let center_left = area.left + (area.width - center_width) / 2;
    let list_height = (row + Px::new(7)) * i32::try_from(LIST_ROWS).unwrap_or(4);
    let focus_height = row * 6 + Px::new(10);
    let focus_top = area.top + row + Px::new(6) + list_height + Px::new(16);
    let focus_rect = Rect::new(center_left, focus_top, center_width, focus_height);
    let accesses = ctx.types.accesses(focus);
    let count = |access: Access| accesses.get(&access).copied().unwrap_or(0);
    out.focus = Some(Item {
        rect: focus_rect,
        title: node.name.clone(),
        krate: node.krate.clone(),
        color: crate_color(ctx.layers, &node.krate),
        via: String::new(),
        hit: Hit::Type(focus),
        via_hit: None,
        on_path: ctx.path_types.contains(&focus),
    });
    let traits: Vec<&String> = node.traits.iter().take(4).collect();
    out.focus_lines = vec![
        vec![
            (format!("{} ", node.kind.name()), WEAK),
            (node.name.clone(), TEXT),
            (
                format!("  {}", node.krate),
                crate_color(ctx.layers, &node.krate),
            ),
        ],
        vec![(format!("{}:{}", node.file, node.line), WEAK)],
        Access::ALL[..3]
            .iter()
            .map(|access| {
                (
                    format!("{} {}  ", access.name(), count(*access)),
                    access_color(*access),
                )
            })
            .collect(),
        Access::ALL[3..]
            .iter()
            .map(|access| {
                (
                    format!("{} {}  ", access.name(), count(*access)),
                    access_color(*access),
                )
            })
            .collect(),
        vec![(
            if traits.is_empty() {
                String::new()
            } else {
                format!(
                    "impl {}",
                    traits
                        .iter()
                        .map(|name| name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            },
            WEAK,
        )],
    ];
    let upstream = ctx.types.made_from(focus);
    let downstream = ctx.types.turned_into(focus);
    let side_top = area.top + row + Px::new(6);
    let bottom = area.bottom() - Px::new(4);
    out.labels.push((
        Point::new(area.left + Px::new(8), area.top + Px::new(2)),
        format!("made from ({})  fn(.., A) -> {}", upstream.len(), node.name),
        WEAK,
    ));
    let right_left = area.right() - side_width - Px::new(8);
    out.labels.push((
        Point::new(right_left, area.top + Px::new(2)),
        format!("turned into ({})  fn({}) -> B", downstream.len(), node.name),
        WEAK,
    ));
    let left_rects = side_items(
        ctx,
        &upstream,
        area.left + Px::new(8),
        side_top,
        side_width,
        bottom,
        &mut out,
    );
    let right_rects = side_items(
        ctx,
        &downstream,
        right_left,
        side_top,
        side_width,
        bottom,
        &mut out,
    );
    let weight = |neighbour: &Neighbour| (1.0 + neighbour.via.len() as f32 * 0.5).min(3.5);
    for (rect, neighbour) in left_rects.iter().zip(&upstream) {
        out.edges.push(Edge::Side {
            from: Point::new(rect.right(), rect.top + rect.height / 2),
            to: Point::new(focus_rect.left, focus_rect.top + focus_rect.height / 2),
            width: weight(neighbour),
            color: GREEN.with_alpha(110),
        });
    }
    for (rect, neighbour) in right_rects.iter().zip(&downstream) {
        out.edges.push(Edge::Side {
            from: Point::new(focus_rect.right(), focus_rect.top + focus_rect.height / 2),
            to: Point::new(rect.left, rect.top + rect.height / 2),
            width: weight(neighbour),
            color: ACCENT.with_alpha(120),
        });
    }
    let held_by = ctx
        .types
        .held_by
        .get(&focus)
        .map(|set| sorted_by_layer(ctx, set))
        .unwrap_or_default();
    let holds = sorted_by_layer(ctx, &node.holds);
    out.labels.push((
        Point::new(center_left, area.top + Px::new(2)),
        format!("held in a field of ({})", held_by.len()),
        WEAK,
    ));
    let top_rects = list_items(ctx, &held_by, center_left, side_top, center_width, &mut out);
    let holds_top = focus_rect.bottom() + row + Px::new(14);
    out.labels.push((
        Point::new(center_left, focus_rect.bottom() + Px::new(8)),
        format!("its fields hold ({})", holds.len()),
        WEAK,
    ));
    let bottom_rects = list_items(ctx, &holds, center_left, holds_top, center_width, &mut out);
    for rect in top_rects {
        out.edges.push(Edge::Straight {
            from: Point::new(rect.left + rect.width / 2, rect.bottom()),
            to: Point::new(rect.left + rect.width / 2, focus_rect.top),
            color: ORANGE.with_alpha(70),
        });
    }
    for rect in bottom_rects {
        out.edges.push(Edge::Straight {
            from: Point::new(rect.left + rect.width / 2, focus_rect.bottom()),
            to: Point::new(rect.left + rect.width / 2, rect.top),
            color: ORANGE.with_alpha(70),
        });
    }
    out
}

fn vector(point: Point) -> Vector {
    Vector::new(
        Coordinate::of_px(point.horizontal),
        Coordinate::of_px(point.vertical),
    )
}

fn shifted(rect: Rect, by: Point) -> Rect {
    Rect::new(
        rect.left + by.horizontal,
        rect.top + by.vertical,
        rect.width,
        rect.height,
    )
}

fn moved(point: Point, by: Point) -> Point {
    Point::new(
        point.horizontal + by.horizontal,
        point.vertical + by.vertical,
    )
}

fn clipped(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_owned();
    }
    let kept: String = text.chars().take(room.saturating_sub(1)).collect();
    format!("{kept}\u{2026}")
}

fn draw_item(
    canvas: &mut Canvas<'_>,
    item: &Item,
    by: Point,
    cell: Px,
    size: FontSize,
    row: Px,
    hovered: bool,
) {
    let rect = shifted(item.rect, by);
    canvas.rect(rect, if hovered { HOVER } else { PANEL });
    canvas.rect(
        Rect::new(rect.left, rect.top, Px::new(3), rect.height),
        item.color,
    );
    canvas.outline(
        rect,
        Px::new(1),
        if item.on_path {
            GREEN.with_alpha(160)
        } else {
            BORDER
        },
    );
    let room = usize::try_from((rect.width.get() / cell.get().max(1)) - 2).unwrap_or(4);
    let crate_room = item.krate.chars().count() + 1;
    let title = clipped(&item.title, room.saturating_sub(crate_room).max(6));
    let pen = canvas.text(
        Point::new(rect.left + Px::new(8), rect.top + Px::new(3)),
        size,
        title,
        TEXT,
    );
    canvas.text(
        Point::new(pen + cell, rect.top + Px::new(3)),
        size,
        item.krate.clone(),
        item.color,
    );
    if !item.via.is_empty() {
        canvas.text(
            Point::new(rect.left + Px::new(8), rect.top + row + Px::new(3)),
            size,
            clipped(&item.via, room),
            WEAK,
        );
    }
}

fn draw_compass(
    canvas: &mut Canvas<'_>,
    rect: Rect,
    compass: &Compass,
    origin: Point,
    cell: Px,
    row: Px,
    size: FontSize,
    hover: Option<Point>,
) {
    canvas.rect(rect, BACKGROUND);
    let by = Point::new(rect.left - origin.horizontal, rect.top - origin.vertical);
    for edge in &compass.edges {
        match edge {
            Edge::Side {
                from,
                to,
                width,
                color,
            } => {
                let from = moved(*from, by);
                let to = moved(*to, by);
                let middle = (from.horizontal + to.horizontal) / 2;
                canvas.curve(
                    [
                        vector(from),
                        vector(Point::new(middle, from.vertical)),
                        vector(Point::new(middle, to.vertical)),
                        vector(to),
                    ],
                    Coordinate::new(*width),
                    *color,
                );
            }
            Edge::Straight { from, to, color } => {
                canvas.line(
                    vector(moved(*from, by)),
                    vector(moved(*to, by)),
                    Coordinate::new(1.0),
                    *color,
                );
            }
        }
    }
    for (at, text, color) in &compass.labels {
        canvas.text(moved(*at, by), size, text.clone(), *color);
    }
    let hovered = |item: &Item| hover.is_some_and(|point| item.rect.contains(point));
    for item in &compass.items {
        draw_item(canvas, item, by, cell, size, row, hovered(item));
    }
    if let Some(focus) = &compass.focus {
        let body = shifted(focus.rect, by);
        canvas.rect(body, SELECTED);
        canvas.rect(
            Rect::new(body.left, body.top, Px::new(4), body.height),
            focus.color,
        );
        canvas.outline(body, Px::new(1), ACCENT);
        for (line, runs) in compass.focus_lines.iter().enumerate() {
            let mut pen = body.left + Px::new(10);
            let top = body.top + Px::new(5) + row * i32::try_from(line).unwrap_or(0);
            for (text, color) in runs {
                pen = canvas.text(Point::new(pen, top), size, text.clone(), *color);
            }
        }
    }
}

fn focus_of(model: &Model, types: &TypeModel) -> Option<TypeIx> {
    let remembered = model
        .types
        .focus
        .and_then(|symbol| types.type_of.get(&symbol).copied());
    let selected = model.nav.focus()?;
    if let Some(ty) = types.type_of.get(&selected) {
        return Some(*ty);
    }
    if let Some(fn_ix) = types.fn_of.get(&selected)
        && let Some(node) = types.fns.get(*fn_ix)
    {
        if let Some(kept) = remembered.filter(|ty| node.uses.contains_key(ty)) {
            return Some(kept);
        }
        return node
            .owner
            .or_else(|| node.outputs().next())
            .or_else(|| {
                node.uses
                    .iter()
                    .find(|(_, set)| set.contains(&Access::Mutate))
                    .map(|(ty, _)| *ty)
            })
            .or_else(|| node.uses.keys().next().copied());
    }
    remembered
}

pub(crate) fn path_sets(model: &Model, types: &TypeModel) -> (BTreeSet<TypeIx>, BTreeSet<FnIx>) {
    let mut path_types = BTreeSet::new();
    let mut path_fns = BTreeSet::new();
    let Some(path) = model.nav.tour().and_then(|slot| model.tour(slot)) else {
        return (path_types, path_fns);
    };
    for step in path.steps() {
        let Some(file) = model.index.find_file(step.file()) else {
            continue;
        };
        for fn_ix in types.fns_within(file, step.span()) {
            path_fns.insert(fn_ix);
        }
        if let Some(symbol) = step.resolved_symbol()
            && let Some(ty) = types.type_of.get(&symbol)
        {
            path_types.insert(*ty);
        }
    }
    (path_types, path_fns)
}

fn fn_table(
    model: &Model,
    frame: &mut Frame<'_>,
    types: &TypeModel,
    layers: &Layers,
    focus: TypeIx,
    path_fns: &BTreeSet<FnIx>,
) {
    let mut rows: Vec<FnIx> = types.touching.get(&focus).cloned().unwrap_or_default();
    rows.sort_by_key(|fn_ix| {
        types
            .fns
            .get(*fn_ix)
            .map_or((usize::MAX, String::new()), |node| {
                (layers.rank(&node.krate), node.label())
            })
    });
    let selected = model.nav.focus();
    let id = ids::types();
    frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let mut last_crate = String::new();
    for fn_ix in rows {
        let Some(node) = types.fns.get(fn_ix) else {
            continue;
        };
        if node.krate != last_crate {
            last_crate = node.krate.clone();
            frame.row_text(vec![Run::new(
                format!("{}", node.krate),
                crate_color(layers, &node.krate),
            )]);
        }
        let accesses = node.uses.get(&focus).cloned().unwrap_or_default();
        let mut runs = vec![Run::new(
            if path_fns.contains(&fn_ix) {
                "  \u{25cf} "
            } else {
                "    "
            },
            GREEN,
        )];
        for access in Access::ALL {
            let on = accesses.contains(&access);
            runs.push(Run::new(
                format!("{:<5}", if on { access.name() } else { "\u{b7}" }),
                if on { access_color(access) } else { BORDER },
            ));
        }
        runs.push(Run::new(format!(" {}", node.label()), TEXT));
        let others: Vec<String> = node
            .uses
            .keys()
            .filter(|ty| **ty != focus)
            .filter_map(|ty| types.types.get(*ty).map(|other| other.name.clone()))
            .take(5)
            .collect();
        if !others.is_empty() {
            runs.push(Run::new(format!("   with {}", others.join(", ")), WEAK));
        }
        let chosen = if selected == Some(node.symbol) {
            Chosen::Chosen
        } else {
            Chosen::Plain
        };
        if frame
            .row(runs, ids::TYPES_ROW.nth(Count::new(fn_ix)), chosen)
            .clicked()
        {
            frame.push(Action::Focus(node.symbol));
        }
    }
    frame.finish();
}

pub(super) fn types_view(model: &Model, frame: &mut Frame<'_>) {
    let types: Rc<TypeModel> = model.derived.types(&model.index, &model.map);
    let layers = model.derived.layers(&model.index, &model.map);
    let Some(focus) = focus_of(model, &types) else {
        frame.label(
            "select a type (a struct, enum or trait) or a function to see how its data moves",
            WEAK,
        );
        return;
    };
    let Some(node) = types.types.get(focus) else {
        return;
    };
    if model.types.focus != Some(node.symbol) {
        frame.push(Action::Types(TypesAction::Focus(node.symbol)));
    }
    let (path_types, path_fns) = path_sets(model, &types);
    if model.types.trail.len() > 1 {
        frame.start(Container::Toolbar);
        frame.label("trail", WEAK);
        for (at, symbol) in model.types.trail.iter().enumerate() {
            let Some(step) = types
                .type_of
                .get(symbol)
                .and_then(|ty| types.types.get(*ty))
            else {
                continue;
            };
            let label = if *symbol == node.symbol {
                format!("[{}]", step.name)
            } else {
                step.name.clone()
            };
            if frame
                .small_button(label, ids::TYPES_TRAIL.nth(Count::new(at)))
                .clicked()
            {
                frame.push(Action::Types(TypesAction::Focus(step.symbol)));
                frame.push(Action::Focus(step.symbol));
            }
            if at + 1 < model.types.trail.len() {
                frame.label("\u{203a}", WEAK);
            }
        }
        frame.finish();
    }
    frame.start(Container::Toolbar);
    let selected_fn = model
        .nav
        .focus()
        .and_then(|symbol| types.fn_of.get(&symbol))
        .and_then(|fn_ix| types.fns.get(*fn_ix));
    if let Some(chosen) = selected_fn {
        frame.label(format!("{} touches", chosen.label()), WEAK);
        for (at, ty) in chosen.uses.keys().enumerate() {
            let Some(other) = types.types.get(*ty) else {
                continue;
            };
            let label = if *ty == focus {
                format!("[{}]", other.name)
            } else {
                other.name.clone()
            };
            if frame
                .small_button(label, ids::TYPES_CHIP.nth(Count::new(at)))
                .clicked()
            {
                frame.push(Action::Types(TypesAction::Focus(other.symbol)));
            }
        }
    } else {
        let mutated: BTreeSet<String> = types
            .touching
            .get(&focus)
            .into_iter()
            .flatten()
            .filter_map(|fn_ix| types.fns.get(*fn_ix))
            .filter(|other| {
                other.uses.get(&focus).is_some_and(|set| {
                    set.contains(&Access::Mutate) || set.contains(&Access::Construct)
                })
            })
            .map(|other| other.krate.clone())
            .collect();
        frame.caption(vec![
            Run::new("created or changed in: ", WEAK),
            Run::new(mutated.into_iter().collect::<Vec<_>>().join(", "), ORANGE),
        ]);
    }
    frame.caption(vec![
        Run::new("  \u{2500} made from  ", GREEN),
        Run::new("\u{2500} turned into  ", ACCENT),
        Run::new("\u{2500} field  ", ORANGE),
        Run::new(
            "green border: on the tour being read; click a type to follow it",
            WEAK,
        ),
    ]);
    frame.finish();
    let canvas_id = ids::TYPES_CANVAS.id();
    let previous = frame.ui.interaction(canvas_id);
    let height = frame.row_height() * 22;
    let area = previous
        .rect()
        .unwrap_or_else(|| Rect::new(Px::ZERO, Px::ZERO, Px::new(900), height));
    let ctx = Ctx {
        types: &types,
        layers: &layers,
        cell: frame.cell_width(),
        row: frame.row_height(),
        path_types: &path_types,
        path_fns: &path_fns,
    };
    let laid = compass(&ctx, focus, area);
    let pointer = frame.ui.pointer();
    if previous.clicked() {
        let point = pointer.mouse;
        let mut hit = None;
        for item in &laid.items {
            if item.rect.contains(point) {
                let via_zone = point.vertical > item.rect.top + ctx.row + Px::new(3);
                hit = if via_zone {
                    item.via_hit.or(Some(item.hit))
                } else {
                    Some(item.hit)
                };
            }
        }
        match hit {
            Some(Hit::Type(ty)) => {
                if let Some(other) = types.types.get(ty) {
                    frame.push(Action::Types(TypesAction::Focus(other.symbol)));
                    frame.push(Action::Focus(other.symbol));
                }
            }
            Some(Hit::Fn(fn_ix)) => {
                if let Some(other) = types.fns.get(fn_ix) {
                    frame.push(Action::Focus(other.symbol));
                }
            }
            None => {
                if laid
                    .focus
                    .as_ref()
                    .is_some_and(|item| item.rect.contains(point))
                {
                    frame.push(Action::Focus(node.symbol));
                }
            }
        }
    }
    let hover = previous.hovered().then_some(pointer.mouse);
    let origin = area.origin();
    let cell = ctx.cell;
    let row = ctx.row;
    let size = frame.metrics.font;
    frame.custom(
        move |canvas, rect| draw_compass(canvas, rect, &laid, origin, cell, row, size, hover),
        Size::Grow,
        height,
        Some(canvas_id),
    );
    frame.start(Container::Toolbar);
    frame.caption(vec![
        Run::new(format!("functions that touch {}: ", node.name), TEXT),
        Run::new("\u{25cf} on the tour   ", GREEN),
        Run::new("new ", GREEN),
        Run::new("read ", WEAK),
        Run::new("mut ", ORANGE),
        Run::new("take ", RED),
        Run::new("ret ", ACCENT),
        Run::new("ref", access_color(Access::Lend)),
    ]);
    frame.finish();
    fn_table(model, frame, &types, &layers, focus, &path_fns);
}
