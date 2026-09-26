use std::collections::BTreeMap;

use domain::{Column, Line, LineCount};
use ui::{
    Button as MouseButton, Coordinate, Extent, FontSize, Interaction, Measure, Point, Pointer, Px,
    Rect, Vector,
};

use crate::action::Action;
use crate::app::App;
use crate::graph::build::{self, Built, CellPoint};
use crate::graph::scene::{self, Drawn, Metrics, Scene, SceneInput};
use crate::graph::{Button, Drag, Expansion, GraphState, Hit, HitRect, Node, Side, Wish};
use crate::grid::GUTTER;
use crate::ids;
use crate::keys::{self, CodeGesture, Wheeling};
use crate::model::{Context, StepKey};
use crate::nav::Scrolling;
use crate::peek::Hovering;
use crate::peek::Intent;
use crate::theme::{self, CANVAS_GUESS, Cells, GRAPH_MARGIN, PAN_ROWS, PIXEL, WHEEL_NOTCH, Zoom};
use crate::widgets::TipAt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Widen {
    Above,
    Below,
}

pub(crate) enum GraphAction {
    Camera {
        zoom: Zoom,
        pan: Point,
    },
    ToggleCollapsed(Node),
    Toggle(Node, Side),
    Widen(Node, Widen),
    Narrow(Node),
    Grab(Option<Drag>),
    Place(Node, Vector),
    Pan(Point),
    HoldLook,
    AutoOpen(Node),
    Built(Box<Built>),
    Look(Point),
    Keep(Option<Point>),
    Fitted(Option<Fit>),
    Cell(Extent),
    OneToOne,
    AutoLayout,
    WantFit,
    Drawn {
        hits: Vec<HitRect>,
        code_top: BTreeMap<Node, Px>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fit {
    zoom: Zoom,
    pan: Point,
}

pub(crate) struct GraphFrame {
    pub(crate) scene: Scene,
    pub(crate) deferred: Vec<Action>,
    pub(crate) tooltip: Option<TipAt>,
    pub(crate) zoom: Zoom,
}

impl GraphState {
    pub(crate) fn apply(&mut self, action: GraphAction) {
        match action {
            GraphAction::Camera { zoom, pan } => {
                self.zoom = zoom;
                self.pan = pan;
            }
            GraphAction::Fitted(fit) => {
                self.fit = Wish::Settled;
                if let Some(fit) = fit {
                    self.zoom = fit.zoom;
                    self.pan = fit.pan;
                }
            }
            GraphAction::Cell(cell) => self.cell = cell,
            GraphAction::ToggleCollapsed(node) => {
                self.keep_node(node);
                if !self.collapsed.remove(&node) {
                    self.collapsed.insert(node);
                }
                self.manual.remove(&node);
            }
            GraphAction::Toggle(node, side) => {
                self.keep_node(node);
                let expansion = Expansion { node, side };
                match self.expansions.iter().position(|found| *found == expansion) {
                    Some(position) => {
                        self.expansions.remove(position);
                    }
                    None => self.expansions.push(expansion),
                }
                self.manual.remove(&node);
            }
            GraphAction::Widen(node, widen) => {
                let around = self.context.entry(node).or_default();
                match widen {
                    Widen::Above => around.above = more(around.above),
                    Widen::Below => around.below = more(around.below),
                }
                self.pin(node);
            }
            GraphAction::Narrow(node) => {
                self.context.remove(&node);
                self.pin(node);
            }
            GraphAction::Grab(drag) => self.drag = drag,
            GraphAction::Place(node, at) => {
                self.manual.insert(node, at);
            }
            GraphAction::Pan(delta) => {
                self.pan = Point::new(
                    self.pan.horizontal + delta.horizontal,
                    self.pan.vertical + delta.vertical,
                );
            }
            GraphAction::HoldLook => self.hold_look = Wish::Wanted,
            GraphAction::AutoOpen(node) => {
                self.auto_open = Some(node);
                for side in [Side::Callees, Side::Callers] {
                    let expansion = Expansion { node, side };
                    if !self.expansions.contains(&expansion) {
                        self.expansions.push(expansion);
                    }
                }
            }
            GraphAction::Built(built) => {
                self.manual.retain(|node, _| built.rank.contains_key(node));
                self.context.retain(|node, _| built.rank.contains_key(node));
                self.built = *built;
            }
            GraphAction::Look(pan) => {
                self.pan = pan;
                self.look = Wish::Settled;
            }
            GraphAction::Keep(pan) => {
                self.keep = None;
                if let Some(pan) = pan {
                    self.pan = pan;
                }
            }
            GraphAction::OneToOne => {
                self.zoom = Zoom::ONE;
                self.look = Wish::Wanted;
            }
            GraphAction::AutoLayout => self.manual.clear(),
            GraphAction::WantFit => self.fit = Wish::Wanted,
            GraphAction::Drawn { hits, code_top } => {
                self.hits = hits;
                self.code_top = code_top;
            }
        }
    }

    fn line_column(&self, cell_width: Px, node: Node, line: Line, mouse: Point) -> Option<Column> {
        let rect = self
            .hits
            .iter()
            .find(|hit| hit.hit == Hit::Line(node, line))?
            .rect;
        let column = (mouse.horizontal - rect.left)
            .ratio(cell_width.max(PIXEL))
            .max(0);
        usize::try_from(column)
            .ok()?
            .checked_sub(GUTTER.get())
            .map(|column| Column::new(u32::try_from(column).unwrap_or(0)))
    }
}

fn more(count: LineCount) -> LineCount {
    LineCount::new(count.value() + Context::LINES.value())
}

#[derive(Clone, Copy)]
struct Aim {
    interaction: Interaction,
    pointer: Pointer,
    canvas: Rect,
}

fn round(value: Coordinate) -> Px {
    Coordinate::new(value.get().round()).truncate()
}

impl App {
    fn graph(&mut self, action: GraphAction) {
        self.apply(Action::Graph(action));
    }

    fn graph_font(&self) -> FontSize {
        theme::graph_font(self.model.metrics.font, self.model.graph.zoom)
    }

    fn graph_wheel(&mut self, measure: &mut dyn Measure, aim: &Aim) {
        let Aim {
            interaction,
            pointer,
            canvas,
            ..
        } = *aim;
        let mouse = pointer.mouse;
        let wheel = interaction.wheel();
        let graph = &self.model.graph;
        if wheel.vertical.get() == 0.0 || !(interaction.hovered() || graph.drag.is_some()) {
            return;
        }
        let pan = graph.pan;
        if keys::wheeling(pointer) == Wheeling::Zoom {
            let old = measure.cell(self.graph_font());
            let zoom = graph.zoom.wheeled(wheel.vertical.get() > 0.0);
            let new = measure.cell(theme::graph_font(self.model.metrics.font, zoom));
            let mouse_across = (mouse.horizontal - canvas.left).float();
            let mouse_down = (mouse.vertical - canvas.top).float();
            let unit_across = (mouse_across - pan.horizontal.float()) / old.width.float();
            let unit_down = (mouse_down - pan.vertical.float()) / old.height.float();
            self.graph(GraphAction::Camera {
                zoom,
                pan: Point::new(
                    round(Coordinate::new(
                        mouse_across - unit_across * new.width.float(),
                    )),
                    round(Coordinate::new(mouse_down - unit_down * new.height.float())),
                ),
            });
            return;
        }
        let per_notch = PAN_ROWS.of(measure.cell(self.graph_font()).height).float();
        let step = |delta: f32| round(Coordinate::new(delta / WHEEL_NOTCH.get() * per_notch));
        let pan = if keys::wheeling(pointer) == Wheeling::Across {
            Point::new(pan.horizontal + step(wheel.vertical.get()), pan.vertical)
        } else {
            Point::new(
                pan.horizontal + step(wheel.horizontal.get()),
                pan.vertical + step(wheel.vertical.get()),
            )
        };
        self.graph(GraphAction::Camera {
            zoom: self.model.graph.zoom,
            pan,
        });
    }

    fn graph_click(
        &mut self,
        measure: &mut dyn Measure,
        aim: &Aim,
        hit: Option<Hit>,
        deferred: &mut Vec<Action>,
    ) {
        let Aim {
            interaction,
            pointer,
            ..
        } = *aim;
        match hit {
            Some(Hit::Button(node, button)) => match button {
                Button::Preview => self.graph(GraphAction::ToggleCollapsed(node)),
                Button::Listing => {
                    if let Some(range) = self.model.graph.built.range(node) {
                        deferred.push(Action::GoTo(node.symbol.file(), range.start()));
                    }
                }
                Button::Callees => self.graph(GraphAction::Toggle(node, Side::Callees)),
                Button::Callers => self.graph(GraphAction::Toggle(node, Side::Callers)),
                Button::Above => self.graph(GraphAction::Widen(node, Widen::Above)),
                Button::Below => self.graph(GraphAction::Widen(node, Widen::Below)),
                Button::NoContext => self.graph(GraphAction::Narrow(node)),
            },
            Some(Hit::Header(node)) => {
                self.graph(GraphAction::Grab(Some(Drag::Node(node))));
                deferred.push(match (node.step, self.model.graph.built.path) {
                    (Some(step), Some(path)) => {
                        Action::SelectStep(StepKey { path, step }, Scrolling::Scroll)
                    }
                    _ => Action::Focus(node.symbol),
                });
                self.graph(GraphAction::HoldLook);
            }
            Some(Hit::Line(node, line)) => {
                self.graph(GraphAction::Grab(Some(Drag::Node(node))));
                let cell_width = measure.cell(self.graph_font()).width;
                let column = self
                    .model
                    .graph
                    .line_column(cell_width, node, line, pointer.mouse);
                let intent = match keys::code_gesture(interaction, pointer) {
                    Some(CodeGesture::Peek) => Some(Intent::Peek),
                    Some(CodeGesture::Jump) => Some(Intent::Jump),
                    None => None,
                };
                if let (Some(column), Some(intent)) = (column, intent) {
                    deferred.push(Action::Definition(node.symbol.file(), line, column, intent));
                }
            }
            Some(Hit::Body(node)) => self.graph(GraphAction::Grab(Some(Drag::Node(node)))),
            None => self.graph(GraphAction::Grab(Some(Drag::Pan))),
        }
    }

    fn graph_drag(&mut self, measure: &mut dyn Measure, delta: Point) {
        match self.model.graph.drag {
            Some(Drag::Pan) => self.graph(GraphAction::Pan(delta)),
            Some(Drag::Node(node)) => {
                let cell = measure.cell(self.graph_font());
                let base = self.model.graph.built.position(node).unwrap_or_default();
                let current = self
                    .model
                    .graph
                    .manual
                    .get(&node)
                    .copied()
                    .unwrap_or(Vector::new(
                        Coordinate::of_integer(base.across.get()),
                        Coordinate::of_integer(base.down.get()),
                    ));
                let across = delta.horizontal.float() / cell.width.max(PIXEL).float();
                let down = delta.vertical.float() / cell.height.max(PIXEL).float();
                self.graph(GraphAction::Place(
                    node,
                    Vector::new(
                        Coordinate::new(current.horizontal.get() + across),
                        Coordinate::new(current.vertical.get() + down),
                    ),
                ));
            }
            None => {}
        }
    }

    fn graph_hover(
        &self,
        measure: &mut dyn Measure,
        aim: &Aim,
        hit: Option<Hit>,
        deferred: &mut Vec<Action>,
    ) -> Option<TipAt> {
        let Aim {
            interaction,
            pointer,
            ..
        } = *aim;
        let Some(Hit::Line(node, line)) =
            hit.filter(|_| interaction.hovered() && !pointer.down.contains(MouseButton::Left))
        else {
            return None;
        };
        let cell_width = measure.cell(self.graph_font()).width;
        let mouse = pointer.mouse;
        let column = self
            .model
            .graph
            .line_column(cell_width, node, line, mouse)?;
        match self.model.hovering(node.symbol.file(), line, column) {
            Hovering::Tip(tip) => Some(TipAt { tip, at: mouse }),
            Hovering::Ask { language, probe } => {
                deferred.push(Action::Hover(language, probe));
                None
            }
            Hovering::Nothing => None,
        }
    }

    fn graph_layout(&mut self) {
        let mut built = build::rebuild(&self.model);
        if let Some(node) = built.auto_open {
            self.graph(GraphAction::AutoOpen(node));
            built = build::rebuild(&self.model);
        }
        built.measure(&self.model, &self.model.graph);
        built.place_nodes(&self.model.index);
        let manual: Vec<(Node, Vector)> = self
            .model
            .graph
            .manual
            .iter()
            .filter(|pair| built.rank.contains_key(pair.0))
            .map(|(node, at)| (*node, *at))
            .collect();
        for (node, at) in manual {
            built.position.insert(
                node,
                CellPoint {
                    across: Cells::new(round(at.horizontal).get()),
                    down: Cells::new(round(at.vertical).get()),
                },
            );
        }
        self.graph(GraphAction::Built(Box::new(built)));
    }

    fn graph_fit(&mut self, measure: &mut dyn Measure, canvas: Rect, first_font: FontSize) {
        let fit = self.model.graph.built.bounds().map(|bounds| {
            let base = self.model.metrics.font;
            let smallest = theme::smallest_fit_font(base);
            let mut size = first_font;
            let mut cell = measure.cell(size);
            let wide = bounds.right - bounds.left;
            let tall = bounds.bottom - bounds.top;
            let room = |extent: Px| extent - GRAPH_MARGIN * 2;
            while size > smallest
                && (wide.of(cell.width) > room(canvas.width)
                    || tall.of(cell.height) > room(canvas.height))
            {
                size = FontSize::new(size.get() - 1);
                cell = measure.cell(size);
            }
            Fit {
                zoom: Zoom::of_fonts(size, base),
                pan: Point::new(
                    GRAPH_MARGIN - bounds.left.of(cell.width),
                    GRAPH_MARGIN - bounds.top.of(cell.height),
                ),
            }
        });
        self.graph(GraphAction::Fitted(fit));
    }

    pub(crate) fn graph_phase(&mut self, measure: &mut dyn Measure) -> GraphFrame {
        let interaction = self.ui.interaction(ids::GRAPH_CANVAS.id());
        let placed = interaction.rect();
        let aim = Aim {
            interaction,
            pointer: self.ui.pointer(),
            canvas: placed.unwrap_or(Rect::new(Px::ZERO, Px::ZERO, CANVAS_GUESS, CANVAS_GUESS)),
        };
        let canvas = aim.canvas;
        let mouse = aim.pointer.mouse;
        let mut deferred = Vec::new();
        self.graph_wheel(measure, &aim);
        let hit = self
            .model
            .graph
            .hits
            .iter()
            .rev()
            .find(|hit| hit.rect.contains(mouse))
            .map(|hit| hit.hit);
        if interaction.clicked() {
            self.graph_click(measure, &aim, hit, &mut deferred);
        }
        if let Some(delta) = interaction.drag() {
            self.graph_drag(measure, delta);
        }
        if !interaction.down() {
            self.graph(GraphAction::Grab(None));
        }
        let tooltip = self.graph_hover(measure, &aim, hit, &mut deferred);
        self.graph_layout();
        let first_font = self.graph_font();
        if self.model.graph.fit == Wish::Wanted && placed.is_some() {
            self.graph_fit(measure, canvas, first_font);
        }
        let size = self.graph_font();
        let cell = measure.cell(size);
        self.graph(GraphAction::Cell(cell));
        let focus = self
            .model
            .graph
            .built
            .focus_node(self.model.nav.focus(), self.model.nav.step());
        if placed.is_some() {
            self.move_camera(canvas, focus);
        }
        let zoom = self.model.graph.zoom;
        let Drawn {
            scene,
            hits,
            code_top,
        } = scene::build_scene(
            &SceneInput {
                model: &self.model,
                built: &self.model.graph.built,
                canvas,
                focus,
                size,
                metrics: Metrics {
                    cell_width: cell.width,
                    row_height: cell.height,
                    padding: cell.width / 2,
                },
                mouse,
                cell,
            },
            &mut self.grids,
        );
        self.graph(GraphAction::Drawn { hits, code_top });
        GraphFrame {
            scene,
            deferred,
            tooltip,
            zoom,
        }
    }

    fn move_camera(&mut self, canvas: Rect, focus: Option<Node>) {
        let graph = &self.model.graph;
        if graph.look == Wish::Wanted {
            if let Some(rect) = focus.and_then(|node| graph.rect_at(node, Point::default())) {
                let down = if rect.height + GRAPH_MARGIN * 2 > canvas.height {
                    GRAPH_MARGIN - rect.top
                } else {
                    canvas.height / 2 - rect.top - rect.height / 2
                };
                let across = canvas.width / 2 - rect.left - rect.width / 2;
                self.graph(GraphAction::Look(Point::new(across, down)));
            }
            return;
        }
        let Some(kept) = graph.keep else {
            return;
        };
        let pan = graph.rect_at(kept.node, Point::default()).map(|rect| {
            Point::new(
                graph.pan.horizontal + kept.at.horizontal - rect.left,
                graph.pan.vertical + kept.at.vertical - rect.top,
            )
        });
        self.graph(GraphAction::Keep(pan));
    }
}
