use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use domain::{Line, Span, Step, Symbol, SymbolId, SymbolName};
use ui::{Canvas, Color, Coordinate, Count, FontSize, Grid, Label, Point, Px, Rect, Run, Vector};

use crate::graph::build::{Built, Header, Labelled, Shown};
use crate::graph::{Hit, HitRect, Node};
use crate::grid::Grids;
use crate::model::Model;
use crate::model::StepKey;
use crate::theme::{
    ACCENT, BORDER, CALL_TINT, FIELD, GRAPH_BEND, GRAPH_BUTTON_GAP, GRAPH_CODE_GAP, GRAPH_EDGE,
    GRAPH_EDGE_END, GRAPH_GAP_ACROSS, GRAPH_RULE, GRAPH_RULE_ABOVE, GRAPH_STEP_EDGE,
    GRAPH_THICK_BORDER, GRAPH_THIN_BORDER, GREEN, HOVER, ORANGE, PANEL, PIXEL, RED, SLICE, TEXT,
    WEAK,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Metrics {
    pub(crate) cell_width: Px,
    pub(crate) row_height: Px,
    pub(crate) padding: Px,
}

struct Edge {
    points: [Vector; 4],
    color: Color,
    width: Coordinate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lit {
    Hovered,
    Plain,
}

struct SceneButton {
    rect: Rect,
    label: ui::Label,
    lit: Lit,
}

struct SceneNode {
    rect: Rect,
    fill: Color,
    border: Color,
    border_width: Px,
    header: Vec<Run>,
    buttons: Vec<SceneButton>,
    note: Option<ui::Label>,
    code_top: Px,
    code: Rc<Grid>,
    start: Line,
    tinted: BTreeSet<Line>,
    slice: Option<Span>,
    more: Option<ui::Count>,
}

pub(crate) struct Scene {
    size: FontSize,
    metrics: Metrics,
    edges: Vec<Edge>,
    nodes: Vec<SceneNode>,
}

pub(crate) struct Drawn {
    pub(crate) scene: Scene,
    pub(crate) hits: Vec<HitRect>,
    pub(crate) code_top: BTreeMap<Node, Px>,
}

fn vector(horizontal: Px, vertical: Px) -> Vector {
    Vector::new(Coordinate::of_px(horizontal), Coordinate::of_px(vertical))
}

pub(crate) struct SceneInput<'input> {
    pub(crate) model: &'input Model,
    pub(crate) built: &'input Built,
    pub(crate) canvas: Rect,
    pub(crate) focus: Option<Node>,
    pub(crate) size: FontSize,
    pub(crate) metrics: Metrics,
    pub(crate) mouse: Point,
    pub(crate) cell: ui::Extent,
}

fn line_top(line: Line, start: Line, row_height: Px) -> Px {
    let offset =
        i32::try_from(line.value()).unwrap_or(0) - i32::try_from(start.value()).unwrap_or(0);
    row_height * offset
}

struct Placed {
    buttons: Vec<SceneButton>,
    hits: Vec<HitRect>,
    right: Px,
}

fn buttons(input: &SceneInput<'_>, node: Node, rect: Rect, labels: Vec<Labelled>) -> Placed {
    let metrics = input.metrics;
    let mut buttons = Vec::new();
    let mut hits = Vec::new();
    let mut right = rect.right() - metrics.padding;
    for labelled in labels.into_iter().rev() {
        let width = Px::of_count(labelled.label.columns() + 2);
        let width = Px::new(width.get() * metrics.cell_width.get());
        right -= width + GRAPH_BUTTON_GAP;
        let area = Rect::new(right, rect.top + metrics.padding, width, metrics.row_height);
        hits.push(HitRect {
            rect: area,
            hit: Hit::Button(node, labelled.button),
        });
        buttons.push(SceneButton {
            rect: area,
            label: labelled.label,
            lit: if area.contains(input.mouse) {
                Lit::Hovered
            } else {
                Lit::Plain
            },
        });
    }
    Placed {
        buttons,
        hits,
        right,
    }
}

struct Outline {
    color: Color,
    width: Px,
}

fn border(input: &SceneInput<'_>, node: Node) -> Outline {
    let built = input.built;
    let on_path = built.step.contains_key(&node);
    let stale = node.step.zip(built.path).is_some_and(|(step, path)| {
        input
            .model
            .step(StepKey { path, step })
            .is_some_and(Step::is_stale)
    });
    let (color, width) = if stale {
        (RED, GRAPH_THICK_BORDER)
    } else if input.focus == Some(node) {
        (ACCENT, GRAPH_THICK_BORDER)
    } else if on_path {
        (GREEN, GRAPH_THICK_BORDER)
    } else {
        (BORDER, GRAPH_THIN_BORDER)
    };
    Outline { color, width }
}

fn tinted(input: &SceneInput<'_>, node: Node, shown: Option<Span>) -> BTreeSet<Line> {
    let built = input.built;
    let index = &input.model.index;
    let mut targets: Vec<SymbolId> = built
        .callees_of(index, node)
        .into_iter()
        .filter(|callee| *callee != node.symbol && built.by_symbol.contains_key(callee))
        .collect();
    targets.extend(
        built
            .step_parent
            .iter()
            .filter(|pair| *pair.1 == node)
            .map(|pair| pair.0.symbol),
    );
    let end_shown = shown.map(Span::end);
    targets
        .iter()
        .filter_map(|target| {
            let name = index.symbol(*target)?.name();
            built.call_line(index, node, name)
        })
        .filter(|line| end_shown.is_some_and(|end| *line <= end))
        .collect()
}

fn scene_node(
    input: &SceneInput<'_>,
    origin: Point,
    node: Node,
    grids: &mut Grids,
    hits: &mut Vec<HitRect>,
) -> Option<SceneNode> {
    let SceneInput {
        model,
        built,
        metrics,
        cell,
        ..
    } = *input;
    let graph = &model.graph;
    let rect = built.rect_at(node, origin, cell)?;
    let view = built.view(node)?;
    let source = model.index.file(node.symbol.file())?;
    let Shown { shown, total } = built.shown(graph, node);
    let shown_span = built.shown_span(graph, node);
    let code = shown_span.map_or_else(
        || Rc::new(Grid::default()),
        |span| grids.get(node.symbol.file(), source, span),
    );
    let Outline {
        color: border,
        width: border_width,
    } = border(input, node);
    let from = hits.len();
    let fill = if built.path.is_some() && !built.step.contains_key(&node) {
        PANEL
    } else {
        FIELD
    };
    let Header {
        runs,
        buttons: labels,
    } = built.header(model, graph, node);
    let Placed {
        buttons,
        hits: button_hits,
        right,
    } = buttons(input, node, rect, labels);
    let header_rect = Rect::new(
        rect.left,
        rect.top,
        (right - rect.left).max(Px::ZERO),
        metrics.row_height + metrics.padding,
    );
    let step_note = node.step.zip(built.path).and_then(|(step, path)| {
        model
            .step(StepKey { path, step })
            .and_then(Step::note)
            .map(|found| Label::new(found.as_str()))
    });
    let note_room = if step_note.is_some() {
        metrics.row_height
    } else {
        Px::ZERO
    };
    let top = rect.top + metrics.padding + metrics.row_height + note_room + GRAPH_CODE_GAP;
    if let Some(span) = shown_span {
        for (row, line) in span.lines().enumerate() {
            hits.push(HitRect {
                rect: Rect::new(
                    rect.left + metrics.padding,
                    top + metrics.row_height * i32::try_from(row).unwrap_or(0),
                    rect.width - metrics.padding * 2,
                    metrics.row_height,
                ),
                hit: Hit::Line(node, line),
            });
        }
    }
    hits.extend(button_hits);
    let mut ordered = vec![
        HitRect {
            rect,
            hit: Hit::Body(node),
        },
        HitRect {
            rect: header_rect,
            hit: Hit::Header(node),
        },
    ];
    ordered.extend(hits.drain(from..));
    hits.extend(ordered);
    Some(SceneNode {
        rect,
        fill,
        border,
        border_width,
        header: runs,
        buttons,
        note: step_note,
        code_top: top,
        code,
        start: view.start(),
        tinted: tinted(input, node, shown_span),
        slice: built.range(node).filter(|range| *range != view),
        more: (shown < total).then(|| Count::new(total.get() - shown.get())),
    })
}

fn edges(input: &SceneInput<'_>, origin: Point, code_top: &BTreeMap<Node, Px>) -> Vec<Edge> {
    let SceneInput {
        model,
        built,
        metrics,
        cell,
        ..
    } = *input;
    let graph = &model.graph;
    let index = &model.index;
    let edge_out = |node: Node, word: &SymbolName| -> Vector {
        let rect = built.rect_at(node, origin, cell).unwrap_or_default();
        let shown_end = built.shown_span(graph, node).map(Span::end);
        let top = code_top.get(&node).copied().unwrap_or(Px::ZERO);
        let start = built.view(node).map_or(Line::new(0), Span::start);
        match built.call_line(index, node, word) {
            Some(line) if shown_end.is_some_and(|end| line <= end) => vector(
                rect.right(),
                top + line_top(line, start, metrics.row_height) + metrics.row_height / 2,
            ),
            _ => vector(
                rect.right(),
                rect.top + metrics.padding + metrics.row_height / 2,
            ),
        }
    };
    let curve = |from: Vector, to: Vector, direction: f32| -> [Vector; 4] {
        let reach = GRAPH_GAP_ACROSS.of(metrics.cell_width).float() * GRAPH_BEND.get();
        let bend =
            ((to.horizontal.get() - from.horizontal.get()).abs() * 0.5).max(reach) * direction;
        [
            from,
            Vector::new(Coordinate::new(from.horizontal.get() + bend), from.vertical),
            Vector::new(Coordinate::new(to.horizontal.get() - bend), to.vertical),
            to,
        ]
    };
    let header_middle = (metrics.padding + metrics.row_height / 2).float();
    let beside = |rect: Rect, across: Px| {
        Vector::new(
            Coordinate::of_px(across),
            Coordinate::new(rect.top.float() + header_middle),
        )
    };
    let mut edges = Vec::new();
    for caller in &built.nodes {
        for callee in built.callees_of(index, *caller) {
            let Some(target) = built.by_symbol.get(&callee).copied() else {
                continue;
            };
            if caller.symbol == callee || built.step_parent.get(&target) == Some(caller) {
                continue;
            }
            let (Some(from), Some(to), Some(name)) = (
                built.rect_at(*caller, origin, cell),
                built.rect_at(target, origin, cell),
                index.symbol(callee).map(Symbol::name),
            ) else {
                continue;
            };
            edges.push(if to.left >= from.right() {
                Edge {
                    points: curve(edge_out(*caller, name), beside(to, to.left), 1.0),
                    color: WEAK,
                    width: GRAPH_EDGE,
                }
            } else {
                Edge {
                    points: curve(beside(from, from.left), beside(to, to.right()), -1.0),
                    color: ORANGE,
                    width: GRAPH_EDGE,
                }
            });
        }
    }
    for (child, parent) in &built.step_parent {
        let (Some(to), Some(name)) = (
            built.rect_at(*child, origin, cell),
            index.symbol(child.symbol).map(Symbol::name),
        ) else {
            continue;
        };
        edges.push(Edge {
            points: curve(edge_out(*parent, name), beside(to, to.left), 1.0),
            color: GREEN,
            width: GRAPH_STEP_EDGE,
        });
    }
    edges
}

pub(crate) fn build_scene(input: &SceneInput<'_>, grids: &mut Grids) -> Drawn {
    let pan = input.model.graph.pan();
    let origin = Point::new(
        input.canvas.left + pan.horizontal,
        input.canvas.top + pan.vertical,
    );
    let mut hits: Vec<HitRect> = Vec::new();
    let mut nodes = Vec::new();
    for node in &input.built.nodes {
        if let Some(scene_node) = scene_node(input, origin, *node, grids, &mut hits) {
            nodes.push((*node, scene_node));
        }
    }
    let code_top: BTreeMap<Node, Px> = nodes
        .iter()
        .map(|(node, scene_node)| (*node, scene_node.code_top))
        .collect();
    let edges = edges(input, origin, &code_top);
    let hits = hits
        .into_iter()
        .map(|hit| HitRect {
            rect: hit.rect.intersect(input.canvas),
            hit: hit.hit,
        })
        .collect();
    Drawn {
        scene: Scene {
            size: input.size,
            metrics: input.metrics,
            edges,
            nodes: nodes.into_iter().map(|pair| pair.1).collect(),
        },
        hits,
        code_top,
    }
}

pub(crate) fn draw_scene(canvas: &mut Canvas<'_>, scene: &Scene) {
    let metrics = scene.metrics;
    for edge in &scene.edges {
        canvas.curve(edge.points, edge.width, edge.color);
        let [_, _, _, end] = edge.points;
        canvas.circle(end, GRAPH_EDGE_END, edge.color);
    }
    for node in &scene.nodes {
        canvas.rect(node.rect, node.fill);
        canvas.outline(node.rect, node.border_width, node.border);
        canvas.push_clip(node.rect.shrink(PIXEL));
        let mut pen = node.rect.left + metrics.padding;
        let top = node.rect.top + metrics.padding;
        for run in &node.header {
            pen = canvas.text(
                Point::new(pen, top),
                scene.size,
                run.text.clone(),
                run.color,
            );
        }
        for button in &node.buttons {
            let hovered = button.lit == Lit::Hovered;
            canvas.rect(button.rect, if hovered { HOVER } else { PANEL });
            canvas.outline(button.rect, GRAPH_RULE, BORDER);
            canvas.text(
                Point::new(button.rect.left + metrics.cell_width, button.rect.top),
                scene.size,
                button.label.clone(),
                if hovered { TEXT } else { WEAK },
            );
        }
        if let Some(note) = &node.note {
            canvas.text(
                Point::new(node.rect.left + metrics.padding, top + metrics.row_height),
                scene.size,
                note.clone(),
                GREEN,
            );
        }
        let inner = node.rect.width - metrics.padding * 2;
        canvas.rect(
            Rect::new(
                node.rect.left + metrics.padding,
                node.code_top - GRAPH_RULE_ABOVE,
                inner,
                GRAPH_RULE,
            ),
            BORDER,
        );
        let mut line_top = node.code_top;
        for row in 0..node.code.rows().get() {
            let line = Line::new(node.start.value() + u32::try_from(row).unwrap_or(0));
            let band = Rect::new(
                node.rect.left + metrics.padding,
                line_top,
                inner,
                metrics.row_height,
            );
            if node.slice.is_some_and(|slice| slice.contains(line)) {
                canvas.rect(band, SLICE);
            }
            if node.tinted.contains(&line) {
                canvas.rect(band, CALL_TINT);
            }
            line_top += metrics.row_height;
        }
        canvas.grid(
            Point::new(node.rect.left + metrics.padding, node.code_top),
            scene.size,
            &node.code,
        );
        if let Some(more) = node.more {
            canvas.text(
                Point::new(
                    node.rect.left + metrics.padding,
                    node.code_top + metrics.row_height * node.code.rows(),
                ),
                scene.size,
                format!("      \u{2026} {more} more lines"),
                WEAK,
            );
        }
        canvas.pop_clip();
    }
}
