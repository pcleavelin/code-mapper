use ui::{Canvas, Color, Coordinate, Extent, Point, Px, Rect};

use crate::graph::Node;
use crate::graph::build::Built;
use crate::theme::{
    ACCENT, BORDER, MINIMAP_CAMERA, MINIMAP_LEAST, MINIMAP_MOST, MINIMAP_NODE, MINIMAP_STEP, PANEL,
    PIXEL,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mark {
    rect: Rect,
    color: Color,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Minimap {
    world: Rect,
    camera: Rect,
    size: Extent,
    marks: Vec<Mark>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MinimapDrag {
    pub(crate) offset: Point,
    pub(crate) world: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Shrink(f32);

#[derive(Clone, Copy, Debug, PartialEq)]
struct Fold {
    scale: Shrink,
    left: Px,
    top: Px,
}

fn joined(one: Rect, other: Rect) -> Rect {
    let left = one.left.min(other.left);
    let top = one.top.min(other.top);
    Rect::new(
        left,
        top,
        one.right().max(other.right()) - left,
        one.bottom().max(other.bottom()) - top,
    )
}

fn inside(inner: Rect, outer: Rect) -> bool {
    inner.left >= outer.left
        && inner.top >= outer.top
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

fn scaled(value: Px, scale: Shrink) -> Px {
    Coordinate::new((value.float() * scale.0).round()).truncate()
}

fn room(canvas: Rect) -> Extent {
    Extent::new(
        MINIMAP_MOST.width.min(canvas.width / 4),
        MINIMAP_MOST.height.min(canvas.height / 3),
    )
}

impl Minimap {
    pub(crate) fn of(
        built: &Built,
        canvas: Rect,
        pan: Point,
        cell: Extent,
        focus: Option<Node>,
        minimap_drag: Option<MinimapDrag>,
    ) -> Option<Self> {
        let bounds = built.bounds()?;
        let graph = Rect::new(
            bounds.left.of(cell.width),
            bounds.top.of(cell.height),
            (bounds.right - bounds.left).of(cell.width),
            (bounds.bottom - bounds.top).of(cell.height),
        );
        let camera = Rect::new(-pan.horizontal, -pan.vertical, canvas.width, canvas.height);
        if minimap_drag.is_none() && inside(graph, camera) {
            return None;
        }
        let world = minimap_drag.map_or_else(|| joined(graph, camera), |drag| drag.world);
        let most = room(canvas);
        if most.width < MINIMAP_LEAST || most.height < MINIMAP_LEAST || world.is_empty() {
            return None;
        }
        let scale = Shrink(
            (most.width.float() / world.width.float())
                .min(most.height.float() / world.height.float()),
        );
        let size = Extent::new(
            scaled(world.width, scale).max(MINIMAP_LEAST),
            scaled(world.height, scale).max(MINIMAP_LEAST),
        );
        let marks = built
            .nodes
            .iter()
            .filter_map(|node| {
                let rect = built.rect_at(*node, Point::default(), cell)?;
                let color = if focus == Some(*node) {
                    ACCENT
                } else if built.step.contains_key(node) {
                    MINIMAP_STEP
                } else {
                    MINIMAP_NODE
                };
                Some(Mark { rect, color })
            })
            .collect();
        Some(Self {
            world,
            camera,
            size,
            marks,
        })
    }

    pub(crate) const fn size(&self) -> Extent {
        self.size
    }

    pub(crate) const fn world(&self) -> Rect {
        self.world
    }

    fn fold(&self, rect: Rect) -> Fold {
        let scale = Shrink(
            (rect.width.float() / self.world.width.float().max(1.0))
                .min(rect.height.float() / self.world.height.float().max(1.0)),
        );
        Fold {
            scale,
            left: rect.left + (rect.width - scaled(self.world.width, scale)) / 2,
            top: rect.top + (rect.height - scaled(self.world.height, scale)) / 2,
        }
    }

    fn to_screen(&self, rect: Rect, world: Rect) -> Rect {
        let fold = self.fold(rect);
        Rect::new(
            fold.left + scaled(world.left - self.world.left, fold.scale),
            fold.top + scaled(world.top - self.world.top, fold.scale),
            scaled(world.width, fold.scale).max(PIXEL),
            scaled(world.height, fold.scale).max(PIXEL),
        )
    }

    pub(crate) fn camera_on(&self, rect: Rect) -> Rect {
        self.to_screen(rect, self.camera)
    }

    pub(crate) fn to_world(&self, rect: Rect, at: Point) -> Point {
        let fold = self.fold(rect);
        let back = |offset: Px| scaled(offset, Shrink(1.0 / fold.scale.0.max(f32::EPSILON)));
        Point::new(
            self.world.left + back(at.horizontal - fold.left),
            self.world.top + back(at.vertical - fold.top),
        )
    }

    pub(crate) fn draw(&self, canvas: &mut Canvas<'_>, rect: Rect) {
        canvas.rect(rect, PANEL);
        canvas.push_clip(rect.shrink(PIXEL));
        for mark in &self.marks {
            canvas.rect(self.to_screen(rect, mark.rect), mark.color);
        }
        let camera = self.camera_on(rect);
        canvas.rect(camera, MINIMAP_CAMERA);
        canvas.outline(camera, PIXEL, ACCENT);
        canvas.pop_clip();
        canvas.outline(rect, PIXEL, BORDER);
    }
}
