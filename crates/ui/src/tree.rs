use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::mem;

use crate::canvas::{Canvas, DrawList, Measure};
use crate::color::Color;
use crate::geometry::{Axis, Count, Extent, Point, Px, Rect, Vector};
use crate::id::Id;
use crate::input::{Button, Input, Pinch, Pointer};
use crate::layout::{Align, Layout, ScrollAxes, Sides, Size, Style};
use crate::text::{Label, Text, Wrap};

const LEAST_WRAP: Count = Count::new(12);

pub trait Draw {
    fn draw(self: Box<Self>, canvas: &mut Canvas<'_>, rect: Rect);
}

impl<Function: FnOnce(&mut Canvas<'_>, Rect)> Draw for Function {
    fn draw(self: Box<Self>, canvas: &mut Canvas<'_>, rect: Rect) {
        self(canvas, rect);
    }
}

pub enum Kind {
    None,
    Text(Text),
    Custom(Box<dyn Draw>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scrollbar {
    pub track: Rect,
    pub thumb: Rect,
}

impl Scrollbar {
    pub const WIDTH: Px = Px::new(8);
    const SHORTEST_THUMB: Px = Px::new(20);
    const TRACK_COLOR: Color = Color::rgba(0, 0, 0, 60);
    const THUMB_COLOR: Color = Color::rgba(140, 140, 148, 150);

    pub fn of(axis: Axis, rect: Rect, visible: Rect, content: Px, scroll: Px) -> Option<Self> {
        let length = rect.extent().along(axis);
        if content <= length || length <= Px::ZERO {
            return None;
        }
        let track = match axis {
            Axis::Vertical => Rect::new(
                rect.right() - Self::WIDTH,
                rect.top,
                Self::WIDTH,
                rect.height,
            ),
            Axis::Horizontal => Rect::new(
                rect.left,
                (visible.bottom() - Self::WIDTH).max(rect.top),
                rect.width,
                Self::WIDTH,
            ),
        };
        let thumb_length =
            Px::of_wide(length.wide() * length.wide() / content.wide()).max(Self::SHORTEST_THUMB);
        let most = (content - length).max(Px::new(1));
        let thumb_start = rect.origin().along(axis)
            + Px::of_wide(
                scroll.clamp(Px::ZERO, most).wide() * (length - thumb_length).wide() / most.wide(),
            );
        Some(Self {
            track,
            thumb: Rect::at(
                track.origin().with(axis, thumb_start),
                track.extent().with(axis, thumb_length),
            ),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Placement {
    pub rect: Rect,
    pub clip: Rect,
    pub content: Extent,
    pub scroll_offset: Point,
    pub scroll_axes: ScrollAxes,
    layer: Layer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Capture {
    #[default]
    Everything,
    Floating,
}

impl Capture {
    fn reaches(self, placement: &Placement) -> bool {
        self == Self::Everything || placement.layer == Layer::Floating
    }
}

impl Placement {
    pub fn visible_center(self) -> Option<Point> {
        let center = self.rect.center();
        self.clip.contains(center).then_some(center)
    }

    pub fn overflow(self, axis: Axis) -> Px {
        if self.scroll_axes.has(axis) {
            (self.content.along(axis) - self.rect.extent().along(axis)).max(Px::ZERO)
        } else {
            Px::ZERO
        }
    }

    pub fn scrollbar(self, axis: Axis, scroll: Px) -> Option<Scrollbar> {
        if !self.scroll_axes.has(axis) {
            return None;
        }
        Scrollbar::of(
            axis,
            self.rect,
            self.rect.intersect(self.clip),
            self.content.along(axis),
            scroll,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Signals(u8);

impl Signals {
    const HOVERED: Self = Self(1);
    const CLICKED: Self = Self(2);
    const DOUBLE_CLICKED: Self = Self(4);
    const DOWN: Self = Self(8);

    #[must_use]
    const fn when(self, on: bool, bit: Self) -> Self {
        if on { Self(self.0 | bit.0) } else { self }
    }

    const fn contains(self, bit: Self) -> bool {
        self.0 & bit.0 != 0
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Interaction {
    signals: Signals,
    drag: Option<Point>,
    wheel: Vector,
    pinch: Pinch,
    rect: Option<Rect>,
}

impl Interaction {
    pub const fn hovered(self) -> bool {
        self.signals.contains(Signals::HOVERED)
    }

    pub const fn clicked(self) -> bool {
        self.signals.contains(Signals::CLICKED)
    }

    pub const fn double_clicked(self) -> bool {
        self.signals.contains(Signals::DOUBLE_CLICKED)
    }

    pub const fn down(self) -> bool {
        self.signals.contains(Signals::DOWN)
    }

    pub const fn drag(self) -> Option<Point> {
        self.drag
    }

    pub const fn wheel(self) -> Vector {
        self.wheel
    }

    pub const fn pinch(self) -> Pinch {
        self.pinch
    }

    pub const fn rect(self) -> Option<Rect> {
        self.rect
    }
}

struct Element {
    kind: Kind,
    layout: Layout,
    style: Style,
    id: Option<Id>,
    children: Vec<Element>,
    size: Extent,
    least: Extent,
    unsqueezed: Extent,
    position: Point,
    content: Extent,
    lines: Vec<Label>,
    rect: Rect,
    clip: Rect,
}

impl Element {
    fn laid_out(&self) -> impl Iterator<Item = &Self> {
        self.children
            .iter()
            .filter(|child| child.layout.floating.is_none())
    }

    fn laid_out_mut(&mut self) -> impl Iterator<Item = &mut Self> {
        self.children
            .iter_mut()
            .filter(|child| child.layout.floating.is_none())
    }

    fn along(&self, axis: Axis) -> bool {
        self.layout.direction.main() == axis
    }

    fn fit(&mut self, axis: Axis, measure: &mut dyn Measure) {
        for child in &mut self.children {
            child.fit(axis, measure);
        }
        let padding = self.layout.padding * 2;
        let sizes: Vec<(Px, Px)> = self
            .laid_out()
            .map(|child| (child.size.along(axis), child.least.along(axis)))
            .collect();
        let (fit, least) = if sizes.is_empty() {
            let fit = self.fit_leaf(axis, measure, padding);
            (fit, self.least_leaf(axis, measure, padding))
        } else if self.along(axis) {
            let gaps = self.layout.gap * (Px::of_count(sizes.len()).get() - 1) + padding;
            let (sum, least) = sizes
                .iter()
                .fold((Px::ZERO, Px::ZERO), |(sum, least), (size, low)| {
                    (sum + *size, least + *low)
                });
            (sum + gaps, least + gaps)
        } else {
            let widest = sizes.iter().map(|(size, _)| *size).max();
            let least = sizes.iter().map(|(_, low)| *low).max();
            (
                widest.unwrap_or(Px::ZERO) + padding,
                least.unwrap_or(Px::ZERO) + padding,
            )
        };
        let (value, least) = match self.layout.size_along(axis) {
            Size::Exact(value) => (value, value),
            Size::Grow => (Px::ZERO, least),
            Size::Fit => (fit, least.min(fit)),
        };
        let room = match (axis, self.layout.size_along(axis)) {
            (Axis::Vertical, Size::Fit) => self.scrollbar_room(),
            _ => Px::ZERO,
        };
        self.size = self.size.with(axis, value + room);
        self.least = self.least.with(axis, least);
    }

    fn scrollbar_room(&self) -> Px {
        let widest = self
            .laid_out()
            .map(|child| child.size.width)
            .max()
            .unwrap_or(Px::ZERO);
        let overflows = widest + self.layout.padding * 2 > self.size.width;
        if self.layout.clips() && self.layout.scroll_axes.has(Axis::Horizontal) && overflows {
            Scrollbar::WIDTH
        } else {
            Px::ZERO
        }
    }

    fn least_leaf(&self, axis: Axis, measure: &mut dyn Measure, padding: Px) -> Px {
        let Kind::Text(text) = &self.kind else {
            return padding;
        };
        let cell = measure.cell(text.size);
        match (axis, text.wrap) {
            (Axis::Horizontal, Wrap::None) => {
                cell.width * Count::new(text.first_glyph_columns() + 1) + padding
            }
            (Axis::Horizontal, Wrap::Words) => {
                let word = text
                    .runs
                    .first()
                    .map_or(0, |run| run.text.longest_word())
                    .min(LEAST_WRAP.get());
                cell.width * Count::new(word) + padding
            }
            _ => self.fit_leaf(axis, measure, padding),
        }
    }

    fn fit_leaf(&self, axis: Axis, measure: &mut dyn Measure, padding: Px) -> Px {
        let Kind::Text(text) = &self.kind else {
            return padding;
        };
        let cell = measure.cell(text.size);
        match (axis, text.wrap) {
            (Axis::Horizontal, Wrap::Words) => Px::ZERO,
            (Axis::Horizontal, Wrap::Clip) => padding,
            (Axis::Horizontal, Wrap::None) => cell.width * text.columns() + padding,
            (Axis::Vertical, Wrap::Words) => {
                cell.height * Count::new(self.lines.len().max(1)) + padding
            }
            (Axis::Vertical, Wrap::None | Wrap::Clip) => cell.height + padding,
        }
    }

    fn grow(&mut self, axis: Axis, window: Extent, level: Level) {
        if (level == Level::Top || self.layout.floating.is_some())
            && self.layout.size_along(axis) == Size::Grow
        {
            self.size = self.size.with(axis, window.along(axis));
        }
        if axis == Axis::Horizontal
            && let Some(at) = self.layout.floating
            && self.layout.size_along(axis) == Size::Fit
        {
            let room = (window.along(axis) - at.along(axis)).max(self.least.along(axis));
            self.size = self.size.with(axis, self.size.along(axis).min(room));
        }
        let inner = self.size.along(axis) - self.layout.padding * 2;
        let count = self.laid_out().count();
        if count > 0 {
            if self.along(axis) {
                let growing = self
                    .laid_out()
                    .filter(|child| child.layout.size_along(axis) == Size::Grow)
                    .count();
                let used = self
                    .laid_out()
                    .fold(Px::ZERO, |sum, child| sum + child.size.along(axis))
                    + self.layout.gap * (Px::of_count(count).get() - 1);
                if growing > 0 {
                    let each = (inner - used).max(Px::ZERO) / Px::of_count(growing).get();
                    for child in self.laid_out_mut() {
                        if child.layout.size_along(axis) == Size::Grow {
                            let least = match axis {
                                Axis::Horizontal => child.least.along(axis),
                                Axis::Vertical => Px::ZERO,
                            };
                            child.size = child.size.with(axis, each.max(least));
                        }
                    }
                }
            } else {
                for child in self.laid_out_mut() {
                    if child.layout.size_along(axis) == Size::Grow {
                        child.size = child.size.with(axis, inner.max(Px::ZERO));
                    }
                }
            }
            if axis == Axis::Horizontal {
                self.squeeze(axis, inner);
            }
        }
        for child in &mut self.children {
            child.grow(axis, window, Level::Nested);
        }
    }

    fn squeezes(&self, axis: Axis) -> bool {
        self.layout.size_along(axis) == Size::Fit && self.size.along(axis) > self.least.along(axis)
    }

    fn squeeze(&mut self, axis: Axis, inner: Px) {
        for child in self.laid_out_mut() {
            child.unsqueezed = child.unsqueezed.with(axis, child.size.along(axis));
        }
        if !self.along(axis) {
            for child in self.laid_out_mut() {
                if child.squeezes(axis) && child.size.along(axis) > inner {
                    let to = inner.max(child.least.along(axis));
                    child.size = child.size.with(axis, to);
                }
            }
            return;
        }
        let count = self.laid_out().count();
        let used = self
            .laid_out()
            .fold(Px::ZERO, |sum, child| sum + child.size.along(axis))
            + self.layout.gap * (Px::of_count(count).get() - 1);
        let mut over = used - inner;
        while over > Px::ZERO {
            let widths: Vec<Px> = self
                .laid_out()
                .filter(|child| child.squeezes(axis))
                .map(|child| child.size.along(axis))
                .collect();
            let Some(widest) = widths.iter().copied().max() else {
                break;
            };
            let next = widths
                .iter()
                .copied()
                .filter(|width| *width < widest)
                .max()
                .unwrap_or(Px::ZERO);
            let tied = Px::of_count(widths.iter().filter(|width| **width == widest).count()).get();
            let step = (widest - next).min((over + Px::new(tied - 1)) / tied);
            for child in self.laid_out_mut() {
                if over > Px::ZERO && child.squeezes(axis) && child.size.along(axis) == widest {
                    let to = (widest - step).max(child.least.along(axis));
                    over -= widest - to;
                    child.size = child.size.with(axis, to);
                }
            }
        }
    }

    fn wrap(&mut self, measure: &mut dyn Measure) {
        if let Kind::Text(text) = &self.kind
            && text.wrap == Wrap::Words
        {
            let cell = measure.cell(text.size);
            let columns = (self.size.width - self.layout.padding * 2)
                .ratio(cell.width.max(Px::new(1)))
                .max(1);
            let columns = usize::try_from(columns).unwrap_or(1);
            self.lines = match text.runs.first() {
                Some(run) => run.text.wrap(columns),
                None => Label::default().wrap(columns),
            };
        }
        for child in &mut self.children {
            child.wrap(measure);
        }
    }

    fn place(&mut self, position: Point, clip: Rect, screen: Rect) {
        self.position = position;
        self.rect = Rect::at(position, self.size);
        self.clip = clip;
        let padding = self.layout.padding;
        let gap = self.layout.gap;
        let cross = self.layout.cross;
        let size = self.size;
        let main = self.layout.direction.main();
        let other = main.other();
        let origin =
            self.rect.origin() + Point::new(padding, padding) - self.layout.scroll_offset();
        let mut cursor = Px::ZERO;
        let mut unsqueezed = Px::ZERO;
        for child in self.laid_out_mut() {
            unsqueezed += child.unsqueezed.along(main).max(child.size.along(main)) + gap;
            let child_size = child.size;
            let mut spot = origin.with(main, origin.along(main) + cursor);
            let shift = match cross {
                Align::Start => Px::ZERO,
                Align::Center => (size.along(other) - padding * 2 - child_size.along(other)) / 2,
            };
            spot = spot.with(other, spot.along(other) + shift);
            cursor += child_size.along(main) + gap;
            child.position = spot;
        }
        let widest = self
            .laid_out()
            .map(|child| child.size.along(other))
            .max()
            .unwrap_or(Px::ZERO);
        self.content = Extent::default()
            .with(
                main,
                (cursor.max(unsqueezed) - gap).max(Px::ZERO) + padding * 2,
            )
            .with(other, (widest + padding * 2).max(size.along(other)));
        let inner = self.clip.intersect(self.rect);
        for child in &mut self.children {
            match child.layout.floating {
                Some(at) => child.place(at, screen, screen),
                None => child.place(child.position, inner, screen),
            }
        }
    }

    const fn placement(&self, layer: Layer) -> Placement {
        Placement {
            rect: self.rect,
            clip: self.clip,
            content: self.content,
            scroll_offset: self.layout.scroll_offset(),
            scroll_axes: self.layout.scroll_axes,
            layer,
        }
    }

    fn record(&self, placements: &mut BTreeMap<Id, Placement>, inherited: Layer) {
        let layer = if self.layout.floating.is_some() {
            Layer::Floating
        } else {
            inherited
        };
        if let Some(id) = self.id {
            placements.insert(id, self.placement(layer));
        }
        for child in &self.children {
            child.record(placements, layer);
        }
    }

    fn paint(&mut self, canvas: &mut Canvas<'_>, text_color: Color) {
        canvas.push_clip(self.clip);
        if let Some(background) = self.style.background {
            canvas.rect(self.rect, background);
        }
        let rect = self.rect;
        match mem::replace(&mut self.kind, Kind::None) {
            Kind::None => {}
            Kind::Text(text) => self.paint_text(canvas, text, text_color),
            Kind::Custom(draw) => {
                canvas.push_clip(rect);
                draw.draw(canvas, rect);
                canvas.pop_clip();
            }
        }
        let border = self.style.border;
        let color = self.style.border_color;
        let one = Px::new(1);
        for (side, edge) in [
            (
                Sides::LEFT,
                Rect::new(rect.left, rect.top, one, rect.height),
            ),
            (
                Sides::RIGHT,
                Rect::new(rect.right() - one, rect.top, one, rect.height),
            ),
            (Sides::TOP, Rect::new(rect.left, rect.top, rect.width, one)),
            (
                Sides::BOTTOM,
                Rect::new(rect.left, rect.bottom() - one, rect.width, one),
            ),
        ] {
            if border.contains(side) {
                canvas.rect(edge, color);
            }
        }
        canvas.pop_clip();
    }

    fn paint_text(&mut self, canvas: &mut Canvas<'_>, text: Text, text_color: Color) {
        let row_height = canvas.cell(text.size).height;
        let at = self.rect.origin() + Point::new(self.layout.padding, self.layout.padding);
        match text.wrap {
            Wrap::Words => {
                let color = text.runs.first().map_or(text_color, |run| run.color);
                let mut top = at.vertical;
                for line in mem::take(&mut self.lines) {
                    canvas.text(Point::new(at.horizontal, top), text.size, line, color);
                    top += row_height;
                }
            }
            Wrap::None | Wrap::Clip => {
                let cell = canvas.cell(text.size).width.max(Px::new(1));
                let room = (self.rect.width - self.layout.padding * 2)
                    .ratio(cell)
                    .max(0);
                let mut pen = at.horizontal;
                for run in Text::clipped(text.runs, usize::try_from(room).unwrap_or(0)) {
                    pen = canvas.text(Point::new(pen, at.vertical), text.size, run.text, run.color);
                }
            }
        }
    }

    fn draw_layer(
        &mut self,
        canvas: &mut Canvas<'_>,
        text_color: Color,
        layer: Layer,
        inherited: Layer,
    ) {
        let own = if self.layout.floating.is_some() {
            Layer::Floating
        } else {
            inherited
        };
        if own == layer && !self.rect.intersect(self.clip).is_empty() {
            self.paint(canvas, text_color);
        }
        for child in &mut self.children {
            child.draw_layer(canvas, text_color, layer, own);
        }
    }

    fn draw_scrollbars(&self, canvas: &mut Canvas<'_>, layer: Layer, inherited: Layer) {
        let own = if self.layout.floating.is_some() {
            Layer::Floating
        } else {
            inherited
        };
        if own == layer && self.layout.clips() {
            let placement = self.placement(own);
            for axis in Axis::BOTH {
                if let Some(bar) = placement.scrollbar(axis, placement.scroll_offset.along(axis)) {
                    canvas.push_clip(self.clip);
                    canvas.rect(bar.track, Scrollbar::TRACK_COLOR);
                    canvas.rect(bar.thumb.shrink(Px::new(1)), Scrollbar::THUMB_COLOR);
                    canvas.pop_clip();
                }
            }
        }
        for child in &self.children {
            child.draw_scrollbars(canvas, layer, own);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Level {
    Top,
    Nested,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Layer {
    Base,
    Floating,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ScrollGrab {
    id: Id,
    axis: Axis,
    grab: Px,
}

#[derive(Default)]
pub struct Ui {
    roots: Vec<Element>,
    stack: Vec<Element>,
    previous: BTreeMap<Id, Placement>,
    hot: Option<Id>,
    active: Option<Id>,
    last_mouse: Point,
    scroll_drag: Option<ScrollGrab>,
    pointer: Pointer,
    size: Extent,
    capture: Capture,
}

impl Ui {
    pub const fn capture(&mut self, capture: Capture) {
        self.capture = capture;
    }

    pub fn begin(&mut self, input: &Input) {
        self.roots.clear();
        self.stack.clear();
        self.size = input.size;
        self.pointer = input.pointer;
        let mouse = input.pointer.mouse;
        self.hot = self
            .previous
            .iter()
            .filter(|(_, placement)| {
                self.capture.reaches(placement)
                    && placement.rect.contains(mouse)
                    && placement.clip.contains(mouse)
            })
            .max_by_key(|(id, placement)| {
                let visible = placement.rect.intersect(placement.clip);
                (Reverse(visible.width.wide() * visible.height.wide()), **id)
            })
            .map(|(id, _)| *id);
        let pressed = input.pointer.pressed.contains(Button::Left);
        if pressed {
            self.active = self.hot;
        }
        if !input.pointer.down.contains(Button::Left) && !pressed {
            self.active = None;
        }
    }

    pub const fn hot(&self) -> Option<Id> {
        self.hot
    }

    pub const fn active(&self) -> Option<Id> {
        self.active
    }

    pub const fn pointer(&self) -> Pointer {
        self.pointer
    }

    pub const fn size(&self) -> Extent {
        self.size
    }

    pub fn interaction(&self, id: Id) -> Interaction {
        let hovered = self.hot == Some(id);
        let pointer = &self.pointer;
        let left_down = pointer.down.contains(Button::Left);
        let left_pressed = pointer.pressed.contains(Button::Left);
        let active = self.active == Some(id);
        Interaction {
            signals: Signals::default()
                .when(hovered, Signals::HOVERED)
                .when(hovered && left_pressed, Signals::CLICKED)
                .when(
                    hovered && pointer.clicks.is_double(Button::Left),
                    Signals::DOUBLE_CLICKED,
                )
                .when(active && left_down, Signals::DOWN),
            drag: (active && left_down && !left_pressed).then(|| pointer.mouse - self.last_mouse),
            wheel: if hovered { pointer.wheel } else { Vector::ZERO },
            pinch: if hovered { pointer.pinch } else { Pinch::ZERO },
            rect: self.previous.get(&id).map(|placement| placement.rect),
        }
    }

    pub fn open(
        &mut self,
        kind: Kind,
        layout: Layout,
        style: Style,
        id: Option<Id>,
    ) -> Interaction {
        self.stack.push(Element {
            kind,
            layout,
            style,
            id,
            children: Vec::new(),
            size: Extent::default(),
            least: Extent::default(),
            unsqueezed: Extent::default(),
            position: Point::default(),
            content: Extent::default(),
            lines: Vec::new(),
            rect: Rect::default(),
            clip: Rect::default(),
        });
        id.map(|id| self.interaction(id)).unwrap_or_default()
    }

    pub fn close(&mut self) {
        if let Some(element) = self.stack.pop() {
            match self.stack.last_mut() {
                Some(parent) => parent.children.push(element),
                None => self.roots.push(element),
            }
        }
    }

    pub fn leaf(
        &mut self,
        kind: Kind,
        layout: Layout,
        style: Style,
        id: Option<Id>,
    ) -> Interaction {
        let interaction = self.open(kind, layout, style, id);
        self.close();
        interaction
    }

    pub const fn dragging(&self) -> bool {
        self.scroll_drag.is_some()
    }

    pub fn placement(&self, id: Id) -> Option<Placement> {
        self.previous.get(&id).copied()
    }

    pub fn on_scrollbar(&self, id: Id) -> bool {
        let mouse = self.pointer.mouse;
        self.previous.get(&id).is_some_and(|placement| {
            placement.clip.contains(mouse)
                && Axis::BOTH.into_iter().any(|axis| {
                    placement
                        .scrollbar(axis, placement.scroll_offset.along(axis))
                        .is_some_and(|bar| bar.track.contains(mouse))
                })
        })
    }

    pub fn scroll_by_wheel(&mut self, id: Id, offset: &mut Point) -> Point {
        let mouse = self.pointer.mouse;
        let inside = self.previous.get(&id).is_some_and(|placement| {
            self.capture.reaches(placement)
                && placement.rect.contains(mouse)
                && placement.clip.contains(mouse)
        });
        if self.hot == Some(id) || inside {
            let wheel = self.pointer.wheel;
            *offset = *offset - Point::new(wheel.horizontal.truncate(), wheel.vertical.truncate());
        }
        let left_down = self.pointer.down.contains(Button::Left);
        let press = (inside && self.pointer.pressed.contains(Button::Left)).then_some(mouse);
        for axis in Axis::BOTH {
            let value = match self.previous.get(&id).copied() {
                Some(placement) => {
                    self.drag_scrollbar(id, placement, axis, offset.along(axis), press)
                }
                None => offset.along(axis).max(Px::ZERO),
            };
            *offset = offset.with(axis, value);
        }
        if !left_down {
            self.scroll_drag = None;
        }
        *offset
    }

    fn drag_scrollbar(
        &mut self,
        id: Id,
        placement: Placement,
        axis: Axis,
        offset: Px,
        press: Option<Point>,
    ) -> Px {
        let mouse = self.pointer.mouse.along(axis);
        let start = placement.rect.origin().along(axis);
        let length = placement.rect.extent().along(axis);
        let content = placement.content.along(axis);
        let most = placement.overflow(axis);
        let mut offset = offset;
        if let Some(bar) = placement.scrollbar(axis, offset) {
            if let Some(press) = press
                && bar.track.contains(press)
            {
                if bar.thumb.contains(press) {
                    self.scroll_drag = Some(ScrollGrab {
                        id,
                        axis,
                        grab: mouse - bar.thumb.origin().along(axis),
                    });
                } else {
                    offset = Px::of_wide(
                        (mouse - start).wide() * content.wide() / length.max(Px::new(1)).wide(),
                    ) - length / 2;
                }
            }
            if let Some(grab) = self.scroll_drag
                && grab.id == id
                && grab.axis == axis
                && self.pointer.down.contains(Button::Left)
            {
                let span = (length - bar.thumb.extent().along(axis)).max(Px::new(1));
                offset =
                    Px::of_wide((mouse - grab.grab - start).wide() * most.wide() / span.wide());
            }
        }
        offset.min(most).max(Px::ZERO)
    }

    pub fn end(&mut self, measure: &mut dyn Measure) {
        while !self.stack.is_empty() {
            self.close();
        }
        let window = self.size;
        for axis in Axis::BOTH {
            for root in &mut self.roots {
                root.fit(axis, measure);
            }
            for root in &mut self.roots {
                root.grow(axis, window, Level::Top);
            }
            if axis == Axis::Horizontal {
                for root in &mut self.roots {
                    root.wrap(measure);
                }
            }
        }
        let screen = Rect::at(Point::default(), window);
        for root in &mut self.roots {
            let at = root.layout.floating.unwrap_or_default();
            root.place(at, screen, screen);
        }
        self.previous.clear();
        for root in &self.roots {
            root.record(&mut self.previous, Layer::Base);
        }
        self.last_mouse = self.pointer.mouse;
    }

    pub fn draw(&mut self, measure: &mut dyn Measure, text_color: Color) -> DrawList {
        let mut roots = mem::take(&mut self.roots);
        let mut canvas = Canvas::new(self.size, measure);
        for layer in [Layer::Base, Layer::Floating] {
            for root in &mut roots {
                root.draw_layer(&mut canvas, text_color, layer, Layer::Base);
            }
            for root in &roots {
                root.draw_scrollbars(&mut canvas, layer, Layer::Base);
            }
        }
        canvas.finish()
    }
}
