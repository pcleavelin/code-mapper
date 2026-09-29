use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::mem;

use crate::canvas::{Canvas, DrawList, Measure};
use crate::color::Color;
use crate::geometry::{Axis, Count, Extent, Point, Px, Rect, Vector};
use crate::id::Id;
use crate::input::{Button, Input, Pinch, Pointer};
use crate::layout::{Align, Layout, Sides, Size, Style};
use crate::text::{Label, Text, Wrap};

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

    pub fn of(rect: Rect, content_height: Px, scroll: Px) -> Option<Self> {
        if content_height <= rect.height || rect.height <= Px::ZERO {
            return None;
        }
        let track = Rect::new(
            rect.right() - Self::WIDTH,
            rect.top,
            Self::WIDTH,
            rect.height,
        );
        let thumb_height =
            Px::of_wide(rect.height.wide() * rect.height.wide() / content_height.wide())
                .max(Self::SHORTEST_THUMB);
        let most = (content_height - rect.height).max(Px::new(1));
        let thumb_top = rect.top
            + Px::of_wide(
                scroll.clamp(Px::ZERO, most).wide() * (rect.height - thumb_height).wide()
                    / most.wide(),
            );
        Some(Self {
            track,
            thumb: Rect::new(track.left, thumb_top, Self::WIDTH, thumb_height),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Placement {
    pub rect: Rect,
    pub clip: Rect,
    pub content: Extent,
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
        let value = match self.layout.size_along(axis) {
            Size::Exact(value) => value,
            Size::Grow => Px::ZERO,
            Size::Fit => {
                let sizes: Vec<Px> = self
                    .laid_out()
                    .map(|child| child.size.along(axis))
                    .collect();
                if sizes.is_empty() {
                    self.fit_leaf(axis, measure, padding)
                } else if self.along(axis) {
                    sizes.iter().fold(Px::ZERO, |sum, size| sum + *size)
                        + self.layout.gap * (Px::of_count(sizes.len()).get() - 1)
                        + padding
                } else {
                    sizes.into_iter().max().unwrap_or(Px::ZERO) + padding
                }
            }
        };
        self.size = self.size.with(axis, value);
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
                            child.size = child.size.with(axis, each);
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
        }
        for child in &mut self.children {
            child.grow(axis, window, Level::Nested);
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
        for child in self.laid_out_mut() {
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
        self.content = size.with(main, (cursor - gap).max(Px::ZERO) + padding * 2);
        let inner = if self.layout.clips() {
            self.clip.intersect(self.rect)
        } else {
            self.clip
        };
        for child in &mut self.children {
            match child.layout.floating {
                Some(at) => child.place(at, screen, screen),
                None => child.place(child.position, inner, screen),
            }
        }
    }

    fn record(&self, placements: &mut BTreeMap<Id, Placement>) {
        if let Some(id) = self.id {
            placements.insert(
                id,
                Placement {
                    rect: self.rect,
                    clip: self.clip,
                    content: self.content,
                },
            );
        }
        for child in &self.children {
            child.record(placements);
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
            Wrap::None => {
                let mut pen = at.horizontal;
                for run in text.runs {
                    pen = canvas.text(Point::new(pen, at.vertical), text.size, run.text, run.color);
                }
            }
            Wrap::Clip => {
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
        if own == layer
            && self.layout.clips()
            && let Some(bar) = Scrollbar::of(
                self.rect,
                self.content.height,
                self.layout.scroll_offset().vertical,
            )
        {
            canvas.push_clip(self.clip);
            canvas.rect(bar.track, Scrollbar::TRACK_COLOR);
            canvas.rect(bar.thumb.shrink(Px::new(1)), Scrollbar::THUMB_COLOR);
            canvas.pop_clip();
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
enum Layer {
    Base,
    Floating,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ScrollGrab {
    id: Id,
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
}

impl Ui {
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
                placement.rect.contains(mouse) && placement.clip.contains(mouse)
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

    pub fn scroll_by_wheel(&mut self, id: Id, offset: &mut Px) -> Px {
        let mouse = self.pointer.mouse;
        let inside = self.previous.get(&id).is_some_and(|placement| {
            placement.rect.contains(mouse) && placement.clip.contains(mouse)
        });
        if self.hot == Some(id) || inside {
            *offset -= self.pointer.wheel.vertical.truncate();
        }
        let left_down = self.pointer.down.contains(Button::Left);
        if let Some(placement) = self.previous.get(&id).copied() {
            let rect = placement.rect;
            let content = placement.content.height;
            if let Some(bar) = Scrollbar::of(rect, content, *offset) {
                let most = (content - rect.height).max(Px::ZERO);
                if self.pointer.pressed.contains(Button::Left)
                    && inside
                    && bar.track.contains(mouse)
                {
                    if bar.thumb.contains(mouse) {
                        self.scroll_drag = Some(ScrollGrab {
                            id,
                            grab: mouse.vertical - bar.thumb.top,
                        });
                    } else {
                        *offset = Px::of_wide(
                            (mouse.vertical - rect.top).wide() * content.wide()
                                / rect.height.max(Px::new(1)).wide(),
                        ) - rect.height / 2;
                    }
                }
                if let Some(grab) = self.scroll_drag
                    && grab.id == id
                    && left_down
                {
                    let span = (rect.height - bar.thumb.height).max(Px::new(1));
                    *offset = Px::of_wide(
                        (mouse.vertical - grab.grab - rect.top).wide() * most.wide() / span.wide(),
                    );
                }
            }
            *offset = (*offset)
                .min((content - rect.height).max(Px::ZERO))
                .max(Px::ZERO);
        } else {
            *offset = (*offset).max(Px::ZERO);
        }
        if !left_down {
            self.scroll_drag = None;
        }
        *offset
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
            root.record(&mut self.previous);
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
