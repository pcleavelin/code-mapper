pub(crate) mod build;
mod input;
mod place;
mod scene;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Duration;

use domain::{Index, LineCount, SymbolId, SymbolKey};
use ui::{Coordinate, Point, Px, Rect, Vector};

use crate::model::StepSlot;
use crate::panels::Direction;
use crate::theme::Zoom;

pub(crate) use build::Built;
pub(crate) use input::{GraphAction, GraphFrame, Heading};
pub(crate) use scene::draw_scene;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Node {
    pub(crate) symbol: SymbolId,
    pub(crate) step: Option<StepSlot>,
}

impl Node {
    const fn off_path(symbol: SymbolId) -> Self {
        Self { symbol, step: None }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Side {
    Callees,
    Callers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Expansion {
    pub(crate) node: Node,
    pub(crate) side: Side,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Around {
    pub(crate) above: LineCount,
    pub(crate) below: LineCount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    Preview,
    Listing,
    Add,
    Callees,
    Callers,
    Above,
    Below,
    NoContext,
}

impl Button {
    #[cfg(test)]
    pub(crate) const fn feature(self) -> features::Feature {
        match self {
            Self::Preview | Self::Above | Self::Below | Self::NoContext => {
                features::Feature::NodeContext
            }
            Self::Listing => features::Feature::NodeListing,
            Self::Add => features::Feature::AddStep,
            Self::Callees | Self::Callers => features::Feature::ExpandNode,
        }
    }

    #[cfg(test)]
    pub(crate) const fn element(self) -> features::Element {
        match self {
            Self::Preview => features::Element::new("node-preview"),
            Self::Listing => features::Element::new("node-listing"),
            Self::Add => features::Element::new("node-add"),
            Self::Callees | Self::Callers => features::Element::new("node-button"),
            Self::Above | Self::Below | Self::NoContext => features::Element::new("node-context"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Header(Node),
    Button(Node, Button),
    Line(Node, domain::Line),
    Body(Node),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HitRect {
    pub(crate) rect: Rect,
    pub(crate) hit: Hit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drag {
    Pan,
    Node(Node),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wish {
    Wanted,
    Settled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Steering {
    Click,
    Keys,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Keyboard {
    Graph,
    Elsewhere,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presence {
    Shown,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Glide {
    from: Point,
    to: Point,
    start: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Kept {
    node: Node,
    at: Point,
}

#[derive(Clone, Debug)]
pub(crate) struct GraphState {
    expansions: Vec<Expansion>,
    auto_open: Option<Node>,
    root: Option<Node>,
    collapsed: BTreeSet<Node>,
    context: BTreeMap<Node, Around>,
    manual: BTreeMap<Node, Vector>,
    drag: Option<Drag>,
    pan: Point,
    zoom: Zoom,
    look: Wish,
    steering: Option<Steering>,
    glide: Option<Glide>,
    direction: Direction,
    keyboard: Keyboard,
    presence: Presence,
    fit: Wish,
    keep: Option<Kept>,
    built: Built,
    hits: Vec<HitRect>,
    code_top: BTreeMap<Node, Px>,
    cell: ui::Extent,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            expansions: Vec::new(),
            auto_open: None,
            root: None,
            collapsed: BTreeSet::new(),
            context: BTreeMap::new(),
            manual: BTreeMap::new(),
            drag: None,
            pan: Point::default(),
            zoom: Zoom::ONE,
            look: Wish::Settled,
            steering: None,
            glide: None,
            direction: Direction::Across,
            keyboard: Keyboard::Elsewhere,
            presence: Presence::Hidden,
            fit: Wish::Settled,
            keep: None,
            built: Built::default(),
            hits: Vec::new(),
            code_top: BTreeMap::new(),
            cell: ui::Extent::default(),
        }
    }
}

#[derive(Clone, Debug)]
struct KeyedNode {
    key: SymbolKey,
    step: Option<StepSlot>,
}

#[derive(Clone, Debug)]
struct SavedExpansion {
    node: KeyedNode,
    side: Side,
}

#[derive(Clone, Debug)]
struct SavedAround {
    node: KeyedNode,
    around: Around,
}

#[derive(Clone, Debug)]
struct SavedPlace {
    node: KeyedNode,
    at: Vector,
}

#[derive(Clone, Debug)]
pub(crate) struct Saved {
    expansions: Vec<SavedExpansion>,
    collapsed: Vec<KeyedNode>,
    context: Vec<SavedAround>,
    manual: Vec<SavedPlace>,
    auto_open: Option<KeyedNode>,
    root: Option<KeyedNode>,
}

pub(crate) struct CameraText<'graph>(&'graph GraphState);

impl fmt::Display for CameraText<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "fit={} look={} keep={}",
            self.0.fit == Wish::Wanted,
            self.0.look == Wish::Wanted,
            self.0.keep.is_some()
        )
    }
}

pub(crate) struct DragText(Option<Drag>);

impl fmt::Display for DragText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            None => "none",
            Some(Drag::Pan) => "pan",
            Some(Drag::Node(_)) => "node",
        };
        write!(formatter, "{name:?}")
    }
}

impl GraphState {
    pub(crate) const fn zoom(&self) -> Zoom {
        self.zoom
    }

    pub(crate) const fn pan(&self) -> Point {
        self.pan
    }

    pub(crate) fn hits(&self) -> &[HitRect] {
        &self.hits
    }

    pub(crate) const fn built(&self) -> &Built {
        &self.built
    }

    pub(crate) fn focused(&mut self) {
        match self.steering.take() {
            Some(Steering::Click) => {}
            Some(Steering::Keys) => self.look = Wish::Wanted,
            None => {
                self.look = Wish::Wanted;
                self.root = None;
            }
        }
    }

    pub(crate) const fn direction(&self) -> Direction {
        self.direction
    }

    pub(crate) const fn keyboard(&self) -> Keyboard {
        self.keyboard
    }

    pub(crate) const fn gliding(&self) -> bool {
        self.glide.is_some()
    }

    pub(crate) const fn camera(&self) -> CameraText<'_> {
        CameraText(self)
    }

    pub(crate) const fn drag_text(&self) -> DragText {
        DragText(self.drag)
    }

    pub(crate) fn save(&self, index: &Index) -> Saved {
        let keyed = |node: &Node| {
            index.symbol_key(node.symbol).map(|key| KeyedNode {
                key,
                step: node.step,
            })
        };
        Saved {
            expansions: self
                .expansions
                .iter()
                .filter_map(|expansion| {
                    Some(SavedExpansion {
                        node: keyed(&expansion.node)?,
                        side: expansion.side,
                    })
                })
                .collect(),
            collapsed: self.collapsed.iter().filter_map(keyed).collect(),
            context: self
                .context
                .iter()
                .filter_map(|(node, around)| {
                    Some(SavedAround {
                        node: keyed(node)?,
                        around: *around,
                    })
                })
                .collect(),
            manual: self
                .manual
                .iter()
                .filter_map(|(node, at)| {
                    Some(SavedPlace {
                        node: keyed(node)?,
                        at: *at,
                    })
                })
                .collect(),
            auto_open: self.auto_open.as_ref().and_then(keyed),
            root: self.root.as_ref().and_then(keyed),
        }
    }

    pub(crate) fn restore(&mut self, index: &Index, saved: &Saved) {
        let node = |keyed: &KeyedNode| {
            index.by_key(&keyed.key).map(|symbol| Node {
                symbol,
                step: keyed.step,
            })
        };
        self.expansions = saved
            .expansions
            .iter()
            .filter_map(|saved| {
                Some(Expansion {
                    node: node(&saved.node)?,
                    side: saved.side,
                })
            })
            .collect();
        self.collapsed = saved.collapsed.iter().filter_map(node).collect();
        self.context = saved
            .context
            .iter()
            .filter_map(|saved| Some((node(&saved.node)?, saved.around)))
            .collect();
        self.manual = saved
            .manual
            .iter()
            .filter_map(|saved| Some((node(&saved.node)?, saved.at)))
            .collect();
        self.auto_open = saved.auto_open.as_ref().and_then(node);
        self.root = saved.root.as_ref().and_then(node);
    }

    fn rect_at(&self, node: Node, origin: Point) -> Option<Rect> {
        self.built.rect_at(node, origin, self.cell)
    }

    fn keep_node(&mut self, node: Node) {
        self.keep = self.rect_at(node, Point::default()).map(|rect| Kept {
            node,
            at: rect.origin(),
        });
    }

    fn pin(&mut self, node: Node) {
        if let Some(at) = self.built.position(node) {
            self.manual.insert(
                node,
                Vector::new(
                    Coordinate::of_integer(at.across.get()),
                    Coordinate::of_integer(at.down.get()),
                ),
            );
        }
    }

    fn has_expansion(&self, node: Node, side: Side) -> bool {
        self.expansions.contains(&Expansion { node, side })
    }
}
