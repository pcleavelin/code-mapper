use std::fmt;

use ui::{Extent, FontSize, Point, Px, Rect};

use crate::theme::{
    DOCK_BOTTOM_LEAST, DOCK_CENTER_ACROSS, DOCK_CENTER_DOWN, DOCK_LEAST_BOTTOM, DOCK_LEAST_SIDE,
    DOCK_NAV, DOCK_XREFS, GRAB_REACH, SPLIT_LEAST,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Panel {
    Nav,
    Xrefs,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PanelName(&'static str);

impl PanelName {
    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for PanelName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Panel {
    pub(crate) const fn name(self) -> PanelName {
        PanelName(match self {
            Self::Nav => "nav",
            Self::Xrefs => "xrefs",
            Self::Output => "output",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    Left,
    Right,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) panel: Panel,
    pub(crate) edge: Edge,
    size: Option<Px>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Moving {
    Stay,
    Moving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Grab {
    pub(crate) panel: Panel,
    from: Point,
    moving: Moving,
}

impl Grab {
    pub(crate) fn is_moving(self) -> bool {
        self.moving == Moving::Moving
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DropTarget {
    pub(crate) edge: Edge,
    pub(crate) at: Slot,
    pub(crate) band: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Frames {
    pub(crate) body: Rect,
    pub(crate) row: Rect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Dock {
    places: Vec<Place>,
    grab: Option<Grab>,
}

impl Default for Dock {
    fn default() -> Self {
        let place = |panel, edge| Place {
            panel,
            edge,
            size: None,
        };
        Self {
            places: vec![
                place(Panel::Nav, Edge::Left),
                place(Panel::Xrefs, Edge::Right),
                place(Panel::Output, Edge::Bottom),
            ],
            grab: None,
        }
    }
}

pub(crate) fn split_width(font: FontSize) -> Px {
    Px::of_count(usize::try_from(font.get() / 2).unwrap_or(0)).max(SPLIT_LEAST)
}

fn default_size(panel: Panel, edge: Edge, window: Extent, cell: Extent) -> Px {
    match edge {
        Edge::Bottom => (window.height / 6).max(DOCK_BOTTOM_LEAST.of(cell.height)),
        Edge::Left | Edge::Right => {
            let cells = if panel == Panel::Nav {
                DOCK_NAV
            } else {
                DOCK_XREFS
            };
            cells.of(cell.width).min(window.width / 4)
        }
    }
}

impl Dock {
    pub(crate) fn places(&self) -> &[Place] {
        &self.places
    }

    pub(crate) const fn grab(&self) -> Option<Grab> {
        self.grab
    }

    pub(crate) fn sizes(&self, window: Extent, cell: Extent) -> Vec<Px> {
        let mut out = vec![Px::ZERO; self.places.len()];
        for bottom in [false, true] {
            let smallest = if bottom {
                DOCK_LEAST_BOTTOM.of(cell.height)
            } else {
                DOCK_LEAST_SIDE.of(cell.width)
            };
            let mut budget = if bottom {
                window.height - DOCK_CENTER_DOWN.of(cell.height)
            } else {
                window.width - DOCK_CENTER_ACROSS.of(cell.width)
            };
            let edge_slots: Vec<usize> = self
                .places
                .iter()
                .enumerate()
                .filter(|pair| (pair.1.edge == Edge::Bottom) == bottom)
                .map(|pair| pair.0)
                .collect();
            for (position, slot) in edge_slots.iter().enumerate() {
                let Some(place) = self.places.get(*slot) else {
                    continue;
                };
                let want = place
                    .size
                    .unwrap_or_else(|| default_size(place.panel, place.edge, window, cell));
                let after = Px::of_count(edge_slots.len() - position - 1);
                let most = (budget - Px::new(smallest.get() * after.get())).max(smallest);
                let size = want.clamp(smallest, most);
                if let Some(entry) = out.get_mut(*slot) {
                    *entry = size;
                }
                budget -= size;
            }
        }
        out
    }

    pub(crate) fn resize(&mut self, panel: Panel, size: Px) {
        if let Some(place) = self.places.iter_mut().find(|place| place.panel == panel) {
            place.size = Some(size);
        }
    }

    pub(crate) fn take_hold(&mut self, panel: Panel, from: Point) {
        self.grab = Some(Grab {
            panel,
            from,
            moving: Moving::Stay,
        });
    }

    pub(crate) fn drag_to(&mut self, mouse: Point) {
        if let Some(grab) = self.grab.as_mut() {
            let moved = (mouse.horizontal - grab.from.horizontal).absolute()
                + (mouse.vertical - grab.from.vertical).absolute();
            if moved > GRAB_REACH {
                grab.moving = Moving::Moving;
            }
        }
    }

    pub(crate) fn release(&mut self) -> Option<Grab> {
        self.grab.take()
    }

    pub(crate) fn move_to(&mut self, panel: Panel, edge: Edge, at: Slot) {
        let Some(from) = self.places.iter().position(|place| place.panel == panel) else {
            return;
        };
        let mut moved = self.places.remove(from);
        if (moved.edge == Edge::Bottom) != (edge == Edge::Bottom) {
            moved.size = None;
        }
        moved.edge = edge;
        let position = self
            .places
            .iter()
            .enumerate()
            .filter(|pair| pair.1.edge == edge)
            .nth(at.0)
            .map_or(self.places.len(), |pair| pair.0);
        self.places.insert(position, moved);
    }

    pub(crate) fn drop_target(
        &self,
        panel: Panel,
        mouse: Point,
        frames: Frames,
        rect_of: impl Fn(Panel) -> Option<Rect>,
        window: Extent,
        cell: Extent,
    ) -> Option<DropTarget> {
        let Frames { body, row } = frames;
        if !body.contains(mouse) {
            return None;
        }
        let edge = if mouse.vertical > row.bottom() - row.height / 4 {
            Edge::Bottom
        } else if mouse.horizontal < body.left + body.width / 2 {
            Edge::Left
        } else {
            Edge::Right
        };
        let others: Vec<Rect> = self
            .places
            .iter()
            .filter(|place| place.edge == edge && place.panel != panel)
            .filter_map(|place| rect_of(place.panel))
            .collect();
        let at = others
            .iter()
            .filter(|rect| {
                if edge == Edge::Bottom {
                    rect.top + rect.height / 2 < mouse.vertical
                } else {
                    rect.left + rect.width / 2 < mouse.horizontal
                }
            })
            .count();
        let place = self.places.iter().find(|place| place.panel == panel)?;
        let kept = place
            .size
            .filter(|_| (place.edge == Edge::Bottom) == (edge == Edge::Bottom));
        let size = kept.unwrap_or_else(|| default_size(panel, edge, window, cell));
        let next = others.get(at);
        let band = match edge {
            Edge::Left => Rect::new(
                next.map_or(
                    others.last().map_or(row.left, |rect| rect.right()),
                    |rect| rect.left,
                ),
                row.top,
                size,
                row.height,
            ),
            Edge::Right => Rect::new(
                next.map_or(row.right(), |rect| rect.left) - size,
                row.top,
                size,
                row.height,
            ),
            Edge::Bottom => Rect::new(
                body.left,
                next.map_or(body.bottom(), |rect| rect.top) - size,
                body.width,
                size,
            ),
        };
        Some(DropTarget {
            edge,
            at: Slot(at),
            band,
        })
    }
}

impl fmt::Display for Dock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let places: Vec<String> = self
            .places
            .iter()
            .map(|place| format!("{}={:?}", place.panel.name(), place.edge))
            .collect();
        let sizes: Vec<String> = self
            .places
            .iter()
            .map(|place| format!("{:?}", place.size.map(Px::get)))
            .collect();
        write!(formatter, "{} sizes={}", places.join(" "), sizes.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell() -> Extent {
        Extent::new(Px::new(8), Px::new(16))
    }

    fn window() -> Extent {
        Extent::new(Px::new(1600), Px::new(1000))
    }

    #[test]
    fn default_sizes_follow_the_window_and_the_cell() {
        let dock = Dock::default();
        let sizes: Vec<i32> = dock
            .sizes(window(), cell())
            .into_iter()
            .map(Px::get)
            .collect();
        assert_eq!(sizes, [352, 320, 166]);
        assert_eq!(
            dock.to_string(),
            "nav=Left xrefs=Right output=Bottom sizes=None,None,None"
        );
    }

    #[test]
    fn sizes_leave_room_for_the_panels_after() {
        let mut dock = Dock::default();
        dock.resize(Panel::Nav, Px::new(5000));
        let sizes: Vec<i32> = dock
            .sizes(window(), cell())
            .into_iter()
            .map(Px::get)
            .collect();
        assert_eq!(sizes, [1344, 96, 166]);
    }

    #[test]
    fn moving_a_panel_across_edges_forgets_its_size() {
        let mut dock = Dock::default();
        dock.resize(Panel::Output, Px::new(200));
        dock.move_to(Panel::Output, Edge::Right, Slot(1));
        assert_eq!(
            dock.to_string(),
            "nav=Left xrefs=Right output=Right sizes=None,None,None"
        );
        dock.resize(Panel::Xrefs, Px::new(300));
        dock.move_to(Panel::Xrefs, Edge::Left, Slot(0));
        assert_eq!(
            dock.to_string(),
            "xrefs=Left nav=Left output=Right sizes=Some(300),None,None"
        );
    }

    #[test]
    fn a_drop_on_the_lower_quarter_docks_at_the_bottom() {
        let dock = Dock::default();
        let frames = Frames {
            body: Rect::new(Px::new(0), Px::new(30), Px::new(1600), Px::new(940)),
            row: Rect::new(Px::new(0), Px::new(30), Px::new(1600), Px::new(760)),
        };
        let target = dock
            .drop_target(
                Panel::Xrefs,
                Point::new(Px::new(300), Px::new(700)),
                frames,
                |_| None,
                window(),
                cell(),
            )
            .unwrap();
        assert_eq!(target.edge, Edge::Bottom);
        assert_eq!(
            target.band,
            Rect::new(Px::new(0), Px::new(804), Px::new(1600), Px::new(166))
        );
        let grabbed = dock.drop_target(
            Panel::Xrefs,
            Point::new(Px::new(300), Px::new(10)),
            frames,
            |_| None,
            window(),
            cell(),
        );
        assert_eq!(grabbed, None);
    }

    #[test]
    fn a_grab_moves_once_the_pointer_leaves_its_reach() {
        let mut dock = Dock::default();
        dock.take_hold(Panel::Nav, Point::new(Px::new(10), Px::new(10)));
        dock.drag_to(Point::new(Px::new(14), Px::new(14)));
        assert!(!dock.grab().unwrap().is_moving());
        dock.drag_to(Point::new(Px::new(15), Px::new(14)));
        assert!(dock.grab().unwrap().is_moving());
    }
}
