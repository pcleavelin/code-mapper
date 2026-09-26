use std::rc::Rc;

use crate::color::Color;
use crate::geometry::{Coordinate, Extent, FontSize, Point, Px, Rect, Vector};
use crate::text::{Grid, Label};

pub trait Measure {
    fn cell(&mut self, size: FontSize) -> Extent;
}

#[derive(Clone, Debug)]
pub enum Command {
    Clip(Rect),
    Fill {
        rect: Rect,
        color: Color,
    },
    Quad {
        corners: [Vector; 4],
        color: Color,
    },
    Circle {
        center: Vector,
        radius: Coordinate,
        color: Color,
    },
    Text {
        at: Point,
        size: FontSize,
        text: Label,
        color: Color,
    },
    Grid {
        at: Point,
        size: FontSize,
        grid: Rc<Grid>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct DrawList {
    commands: Vec<Command>,
}

impl DrawList {
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

pub struct Canvas<'frame> {
    list: DrawList,
    screen: Rect,
    clips: Vec<Rect>,
    measure: &'frame mut dyn Measure,
}

impl<'frame> Canvas<'frame> {
    pub fn new(screen: Extent, measure: &'frame mut dyn Measure) -> Self {
        let screen = Rect::at(Point::default(), screen);
        Self {
            list: DrawList::default(),
            screen,
            clips: vec![screen],
            measure,
        }
    }

    pub fn finish(self) -> DrawList {
        self.list
    }

    pub fn cell(&mut self, size: FontSize) -> Extent {
        self.measure.cell(size)
    }

    pub fn clip(&self) -> Rect {
        self.clips.last().copied().unwrap_or(self.screen)
    }

    pub fn push_clip(&mut self, rect: Rect) {
        let clip = self.clip().intersect(rect);
        self.clips.push(clip);
        self.list.commands.push(Command::Clip(clip));
    }

    pub fn pop_clip(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
            let clip = self.clip();
            self.list.commands.push(Command::Clip(clip));
        }
    }

    pub fn rect(&mut self, rect: Rect, color: Color) {
        if rect.is_empty() || color.is_invisible() {
            return;
        }
        self.list.commands.push(Command::Fill { rect, color });
    }

    pub fn outline(&mut self, rect: Rect, width: Px, color: Color) {
        self.rect(Rect::new(rect.left, rect.top, rect.width, width), color);
        self.rect(
            Rect::new(rect.left, rect.bottom() - width, rect.width, width),
            color,
        );
        self.rect(Rect::new(rect.left, rect.top, width, rect.height), color);
        self.rect(
            Rect::new(rect.right() - width, rect.top, width, rect.height),
            color,
        );
    }

    pub fn line(&mut self, from: Vector, to: Vector, width: Coordinate, color: Color) {
        let across = to.horizontal.get() - from.horizontal.get();
        let down = to.vertical.get() - from.vertical.get();
        let length = (across * across + down * down).sqrt();
        if length < 0.01 {
            return;
        }
        let width = width.get();
        let normal_across = -down / length * width / 2.0;
        let normal_down = across / length * width / 2.0;
        let corner = |base: Vector, sign: f32| {
            Vector::new(
                Coordinate::new(base.horizontal.get() + sign * normal_across),
                Coordinate::new(base.vertical.get() + sign * normal_down),
            )
        };
        let corners = [
            corner(from, 1.0),
            corner(to, 1.0),
            corner(to, -1.0),
            corner(from, -1.0),
        ];
        self.list.commands.push(Command::Quad { corners, color });
    }

    pub fn curve(&mut self, points: [Vector; 4], width: Coordinate, color: Color) {
        let [first, second, third, fourth] = points;
        let steps: u8 = 24;
        let mut previous = first;
        for step in 1..=steps {
            let along = f32::from(step) / f32::from(steps);
            let rest = 1.0 - along;
            let weights = [
                rest.powi(3),
                3.0 * along * rest.powi(2),
                3.0 * along * along * rest,
                along.powi(3),
            ];
            let [weight_first, weight_second, weight_third, weight_fourth] = weights;
            let blend = |pick: fn(Vector) -> f32| {
                weight_first * pick(first)
                    + weight_second * pick(second)
                    + weight_third * pick(third)
                    + weight_fourth * pick(fourth)
            };
            let next = Vector::new(
                Coordinate::new(blend(|point| point.horizontal.get())),
                Coordinate::new(blend(|point| point.vertical.get())),
            );
            self.line(previous, next, width, color);
            previous = next;
        }
    }

    pub fn circle(&mut self, center: Vector, radius: Coordinate, color: Color) {
        self.list.commands.push(Command::Circle {
            center,
            radius,
            color,
        });
    }

    pub fn text(&mut self, at: Point, size: FontSize, text: impl Into<Label>, color: Color) -> Px {
        let text = text.into();
        let cell = self.measure.cell(size);
        let right = self.clip().right();
        let mut pen = at.horizontal;
        for _ in text.as_str().chars() {
            if pen >= right {
                break;
            }
            pen += cell.width;
        }
        self.list.commands.push(Command::Text {
            at,
            size,
            text,
            color,
        });
        pen
    }

    pub fn grid(&mut self, at: Point, size: FontSize, grid: &Rc<Grid>) {
        self.list.commands.push(Command::Grid {
            at,
            size,
            grid: Rc::clone(grid),
        });
    }
}
