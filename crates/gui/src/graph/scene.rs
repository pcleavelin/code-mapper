use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use domain::{Line, Span, Step, Symbol, SymbolId, SymbolName};
use ui::{
    Canvas, Color, Coordinate, Count, FontSize, Grid, Icon, Label, Point, Px, Rect, Run, Vector,
};

use crate::graph::build::{Built, Header, Labelled, Shown};
use crate::graph::place::{Parentage, Siblings};
use crate::graph::{Hit, HitRect, Node};
use crate::grid::Grids;
use crate::model::Model;
use crate::model::StepKey;
use crate::panels::Direction;
use crate::theme::{
    ACCENT, BACK_EDGE, BORDER, CALL_TINT, FIELD, GRAPH_BEND, GRAPH_BOX_HEADER, GRAPH_BUTTON_GAP,
    GRAPH_CODE_GAP, GRAPH_EDGE, GRAPH_EDGE_END, GRAPH_RANK_GAP_ACROSS, GRAPH_RANK_GAP_DOWN,
    GRAPH_RULE, GRAPH_RULE_ABOVE, GRAPH_STEP_EDGE, GRAPH_THICK_BORDER, GRAPH_THIN_BORDER, GREEN,
    HOVER, PANEL, PIXEL, RED, REVEAL_EDGE, SIBLINGS_BORDER, SIBLINGS_FILL, SLICE, STEP_BORDER,
    STEP_EDGE, TEXT, WEAK,
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

struct SceneBox {
    rect: Rect,
    border: Color,
    header: Vec<Run>,
    header_top: Px,
    parent: Option<ParentButton>,
}

struct ParentButton {
    node: Node,
    button: SceneButton,
}

pub(crate) struct Scene {
    size: FontSize,
    metrics: Metrics,
    boxes: Vec<SceneBox>,
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
        (STEP_BORDER, GRAPH_THIN_BORDER)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bend {
    Ahead,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Router {
    direction: Direction,
    reach: Coordinate,
    header_middle: Px,
}

impl Router {
    fn of(direction: Direction, metrics: Metrics) -> Self {
        let reach = match direction {
            Direction::Right => GRAPH_RANK_GAP_ACROSS.of(metrics.cell_width),
            Direction::Down => GRAPH_RANK_GAP_DOWN.of(metrics.row_height),
        };
        Self {
            direction,
            reach: Coordinate::new(reach.float() * GRAPH_BEND.get()),
            header_middle: metrics.padding + metrics.row_height / 2,
        }
    }

    fn curve(self, from: Vector, to: Vector, bend: Bend) -> [Vector; 4] {
        let sign = match bend {
            Bend::Ahead => 1.0,
            Bend::Back => -1.0,
        };
        let span = |start: Coordinate, end: Coordinate| {
            ((end.get() - start.get()).abs() * 0.5).max(self.reach.get()) * sign
        };
        match self.direction {
            Direction::Right => {
                let pull = span(from.horizontal, to.horizontal);
                [
                    from,
                    Vector::new(Coordinate::new(from.horizontal.get() + pull), from.vertical),
                    Vector::new(Coordinate::new(to.horizontal.get() - pull), to.vertical),
                    to,
                ]
            }
            Direction::Down => {
                let pull = span(from.vertical, to.vertical);
                [
                    from,
                    Vector::new(from.horizontal, Coordinate::new(from.vertical.get() + pull)),
                    Vector::new(to.horizontal, Coordinate::new(to.vertical.get() - pull)),
                    to,
                ]
            }
        }
    }

    fn entry(self, rect: Rect) -> Vector {
        match self.direction {
            Direction::Right => vector(rect.left, rect.top + self.header_middle),
            Direction::Down => vector(rect.left + rect.width / 2, rect.top),
        }
    }

    fn ahead(self, from: Rect, to: Rect) -> bool {
        match self.direction {
            Direction::Right => to.left >= from.right(),
            Direction::Down => to.top >= from.bottom(),
        }
    }

    fn back(self, from: Rect, to: Rect) -> [Vector; 4] {
        match self.direction {
            Direction::Right => self.curve(
                vector(from.left, from.top + self.header_middle),
                vector(to.right(), to.top + self.header_middle),
                Bend::Back,
            ),
            Direction::Down => self.curve(
                vector(from.left + from.width / 2, from.top),
                vector(to.left + to.width / 2, to.bottom()),
                Bend::Back,
            ),
        }
    }
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
    let router = Router::of(built.direction, metrics);
    let edge_out = |node: Node, word: &SymbolName| -> Vector {
        let rect = built.rect_at(node, origin, cell).unwrap_or_default();
        if built.direction == Direction::Down {
            return vector(rect.left + rect.width / 2, rect.bottom());
        }
        let shown_end = built.shown_span(graph, node).map(Span::end);
        let top = code_top.get(&node).copied().unwrap_or(Px::ZERO);
        let start = built.view(node).map_or(Line::new(0), Span::start);
        match built.call_line(index, node, word) {
            Some(line) if shown_end.is_some_and(|end| line <= end) => vector(
                rect.right(),
                top + line_top(line, start, metrics.row_height) + metrics.row_height / 2,
            ),
            _ => vector(rect.right(), rect.top + router.header_middle),
        }
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
            edges.push(if router.ahead(from, to) {
                Edge {
                    points: router.curve(edge_out(*caller, name), router.entry(to), Bend::Ahead),
                    color: REVEAL_EDGE,
                    width: GRAPH_EDGE,
                }
            } else {
                Edge {
                    points: router.back(from, to),
                    color: BACK_EDGE,
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
            points: router.curve(edge_out(*parent, name), router.entry(to), Bend::Ahead),
            color: STEP_EDGE,
            width: GRAPH_STEP_EDGE,
        });
    }
    edges
}

fn toward_parent(parentage: Parentage, direction: Direction) -> Icon {
    match (parentage, direction) {
        (Parentage::Callers(_), Direction::Right) => Icon::Forward,
        (Parentage::Callers(_), Direction::Down) => Icon::Down,
        (_, Direction::Right) => Icon::Back,
        (_, Direction::Down) => Icon::Up,
    }
}

fn box_header(input: &SceneInput<'_>, parentage: Parentage) -> Vec<Run> {
    let model = input.model;
    let built = input.built;
    match parentage {
        Parentage::Path => {
            let name = built
                .path
                .and_then(|path| model.path(path))
                .map_or("", |path| path.name().as_str());
            vec![Run::new("path ", WEAK), Run::new(name, TEXT)]
        }
        Parentage::OffPath if built.path.is_some() => vec![Run::new("not in the path", WEAK)],
        Parentage::OffPath => vec![Run::new("no path open", WEAK)],
        Parentage::Callees(_) => vec![Run::new("callees of", WEAK)],
        Parentage::Callers(_) => vec![Run::new("callers of", WEAK)],
    }
}

fn parent_label(input: &SceneInput<'_>, parentage: Parentage, parent: Node) -> Label {
    let built = input.built;
    let icon = toward_parent(parentage, built.direction).glyph().get();
    let number = built
        .step
        .get(&parent)
        .map(|step| format!("{} ", step.number.as_str()))
        .unwrap_or_default();
    let name = input
        .model
        .index
        .symbol(parent.symbol)
        .map_or("", |symbol| symbol.name().as_str());
    Label::new(format!("{icon} {number}{name}"))
}

fn sibling_box(input: &SceneInput<'_>, origin: Point, siblings: &Siblings) -> Option<SceneBox> {
    let SceneInput {
        built,
        cell,
        metrics,
        ..
    } = *input;
    let bounds = built.sibling_bounds(siblings)?;
    let rect = Rect::new(
        origin.horizontal + bounds.left.of(cell.width),
        origin.vertical + bounds.top.of(cell.height),
        (bounds.right - bounds.left).of(cell.width),
        (bounds.bottom - bounds.top).of(cell.height),
    );
    let header = box_header(input, siblings.parentage);
    let header_top = rect.top + (GRAPH_BOX_HEADER.of(cell.height) - metrics.row_height) / 2;
    let columns: usize = header.iter().map(|run| run.text.columns()).sum();
    let parent = siblings.parentage.parent().map(|parent| {
        let label = parent_label(input, siblings.parentage, parent);
        let left = rect.left + metrics.padding + metrics.cell_width * Count::new(columns + 1);
        let width = metrics.cell_width * Count::new(label.columns() + 2);
        let area = Rect::new(left, header_top, width, metrics.row_height);
        let lit = if area.contains(input.mouse) {
            Lit::Hovered
        } else {
            Lit::Plain
        };
        ParentButton {
            node: parent,
            button: SceneButton {
                rect: area,
                label,
                lit,
            },
        }
    });
    Some(SceneBox {
        rect,
        border: if siblings.parentage == Parentage::Path {
            STEP_BORDER
        } else {
            SIBLINGS_BORDER
        },
        header,
        header_top,
        parent,
    })
}

pub(crate) fn build_scene(input: &SceneInput<'_>, grids: &mut Grids) -> Drawn {
    let pan = input.model.graph.pan();
    let origin = Point::new(
        input.canvas.left + pan.horizontal,
        input.canvas.top + pan.vertical,
    );
    let boxes: Vec<SceneBox> = input
        .built
        .siblings
        .iter()
        .filter_map(|siblings| sibling_box(input, origin, siblings))
        .collect();
    let mut hits: Vec<HitRect> = boxes
        .iter()
        .filter_map(|scene_box| scene_box.parent.as_ref())
        .map(|parent| HitRect {
            rect: parent.button.rect,
            hit: Hit::Parent(parent.node),
        })
        .collect();
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
            boxes,
            edges,
            nodes: nodes.into_iter().map(|pair| pair.1).collect(),
        },
        hits,
        code_top,
    }
}

fn draw_button(canvas: &mut Canvas<'_>, scene: &Scene, button: &SceneButton) {
    let hovered = button.lit == Lit::Hovered;
    canvas.rect(button.rect, if hovered { HOVER } else { PANEL });
    canvas.outline(button.rect, GRAPH_RULE, BORDER);
    canvas.text(
        Point::new(button.rect.left + scene.metrics.cell_width, button.rect.top),
        scene.size,
        button.label.clone(),
        if hovered { TEXT } else { WEAK },
    );
}

fn draw_box(canvas: &mut Canvas<'_>, scene: &Scene, scene_box: &SceneBox) {
    canvas.rect(scene_box.rect, SIBLINGS_FILL);
    canvas.outline(scene_box.rect, GRAPH_THIN_BORDER, scene_box.border);
    canvas.push_clip(scene_box.rect.shrink(PIXEL));
    let mut pen = scene_box.rect.left + scene.metrics.padding;
    for run in &scene_box.header {
        pen = canvas.text(
            Point::new(pen, scene_box.header_top),
            scene.size,
            run.text.clone(),
            run.color,
        );
    }
    if let Some(parent) = &scene_box.parent {
        draw_button(canvas, scene, &parent.button);
    }
    canvas.pop_clip();
}

pub(crate) fn draw_scene(canvas: &mut Canvas<'_>, scene: &Scene) {
    for scene_box in &scene.boxes {
        draw_box(canvas, scene, scene_box);
    }
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
            draw_button(canvas, scene, button);
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
