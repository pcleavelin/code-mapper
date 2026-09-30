use std::collections::{BTreeMap, BTreeSet, VecDeque};

use domain::Index;

use crate::graph::build::{Built, CellPoint, CellSize, Rank};
use crate::graph::{Node, Side};
use crate::panels::Direction;
use crate::theme::{
    Cells, GRAPH_BOX_GAP_ACROSS, GRAPH_BOX_GAP_DOWN, GRAPH_BOX_HEADER, GRAPH_PAD_ACROSS,
    GRAPH_PAD_DOWN, GRAPH_RANK_GAP_ACROSS, GRAPH_RANK_GAP_DOWN, GRAPH_SIBLING_GAP_ACROSS,
    GRAPH_SIBLING_GAP_DOWN,
};

struct Trees {
    right: BTreeMap<Node, Vec<Node>>,
    left: BTreeMap<Node, Vec<Node>>,
}

struct Forest {
    trees: Trees,
    roots: Vec<Node>,
}

impl Trees {
    fn children(&self, node: Node, side: Side) -> &[Node] {
        let map = match side {
            Side::Callees => &self.right,
            Side::Callers => &self.left,
        };
        map.get(&node).map_or(&[], Vec::as_slice)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Spacing {
    rank_gap: Cells,
    sibling_gap: Cells,
    box_gap: Cells,
    lead_main: Cells,
    lead_cross: Cells,
    pad_main: Cells,
    pad_cross: Cells,
}

impl Spacing {
    const fn of(direction: Direction) -> Self {
        match direction {
            Direction::Right => Self {
                rank_gap: GRAPH_RANK_GAP_ACROSS,
                sibling_gap: GRAPH_SIBLING_GAP_DOWN,
                box_gap: GRAPH_BOX_GAP_DOWN,
                lead_main: GRAPH_PAD_ACROSS,
                lead_cross: GRAPH_BOX_HEADER,
                pad_main: GRAPH_PAD_ACROSS,
                pad_cross: GRAPH_PAD_DOWN,
            },
            Direction::Down => Self {
                rank_gap: GRAPH_RANK_GAP_DOWN,
                sibling_gap: GRAPH_SIBLING_GAP_ACROSS,
                box_gap: GRAPH_BOX_GAP_ACROSS,
                lead_main: GRAPH_BOX_HEADER,
                lead_cross: GRAPH_PAD_ACROSS,
                pad_main: GRAPH_PAD_DOWN,
                pad_cross: GRAPH_PAD_ACROSS,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Parentage {
    Path,
    OffPath,
    Callees(Node),
    Callers(Node),
}

impl Parentage {
    pub(crate) const fn parent(self) -> Option<Node> {
        match self {
            Self::Path | Self::OffPath => None,
            Self::Callees(node) | Self::Callers(node) => Some(node),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Siblings {
    pub(crate) parentage: Parentage,
    pub(crate) members: Vec<Node>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CellBounds {
    pub(crate) left: Cells,
    pub(crate) top: Cells,
    pub(crate) right: Cells,
    pub(crate) bottom: Cells,
}

impl CellBounds {
    fn joined(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }
}

const fn main_size(direction: Direction, size: CellSize) -> Cells {
    match direction {
        Direction::Right => size.wide,
        Direction::Down => size.tall,
    }
}

const fn cross_size(direction: Direction, size: CellSize) -> Cells {
    match direction {
        Direction::Right => size.tall,
        Direction::Down => size.wide,
    }
}

const fn cross_of(direction: Direction, at: CellPoint) -> Cells {
    match direction {
        Direction::Right => at.down,
        Direction::Down => at.across,
    }
}

const fn point(direction: Direction, main: Cells, cross: Cells) -> CellPoint {
    match direction {
        Direction::Right => CellPoint {
            across: main,
            down: cross,
        },
        Direction::Down => CellPoint {
            across: cross,
            down: main,
        },
    }
}

struct Lanes {
    start: BTreeMap<Rank, Cells>,
    free: BTreeMap<Rank, Cells>,
}

impl Lanes {
    fn free(&self, rank: Rank) -> Cells {
        self.free.get(&rank).copied().unwrap_or(Cells::ZERO)
    }

    fn start(&self, rank: Rank) -> Cells {
        self.start.get(&rank).copied().unwrap_or(Cells::ZERO)
    }
}

impl Built {
    fn size_of(&self, node: Node) -> CellSize {
        self.size.get(&node).copied().unwrap_or_default()
    }

    fn rank_of(&self, node: Node) -> Rank {
        self.rank.get(&node).copied().unwrap_or(Rank::ZERO)
    }

    fn lane_starts(
        &self,
        ranks: &BTreeMap<Rank, Vec<Node>>,
        spacing: Spacing,
    ) -> BTreeMap<Rank, Cells> {
        let mut start: BTreeMap<Rank, Cells> = BTreeMap::new();
        let (Some(first), Some(last)) = (
            ranks.keys().next().copied(),
            ranks.keys().next_back().copied(),
        ) else {
            return start;
        };
        let anchor = Rank::ZERO.clamp(first, last);
        let width_of = |rank: Rank| {
            ranks.get(&rank).map_or(Cells::ZERO, |nodes| {
                nodes
                    .iter()
                    .map(|node| main_size(self.direction, self.size_of(*node)))
                    .max()
                    .unwrap_or(Cells::ZERO)
            }) + spacing.lead_main
                + spacing.pad_main
        };
        start.insert(anchor, Cells::ZERO);
        let mut outward = anchor;
        while outward < last {
            let before = start.get(&outward).copied().unwrap_or(Cells::ZERO);
            start.insert(
                outward.next(),
                before + width_of(outward) + spacing.rank_gap,
            );
            outward = outward.next();
        }
        let mut inward = anchor;
        while inward > first {
            let after = start.get(&inward).copied().unwrap_or(Cells::ZERO);
            let previous = inward.previous();
            start.insert(previous, after - width_of(previous) - spacing.rank_gap);
            inward = previous;
        }
        start
    }

    fn trees(&self, index: &Index) -> Forest {
        let mut trees = Trees {
            right: BTreeMap::new(),
            left: BTreeMap::new(),
        };
        let mut has_parent = BTreeSet::new();
        for node in &self.nodes {
            if let Some(parent) = self.step_parent.get(node) {
                trees.right.entry(*parent).or_default().push(*node);
                has_parent.insert(*node);
            } else if let Some(origin) = self.origin.get(node) {
                let side = match origin.side {
                    Side::Callees => &mut trees.right,
                    Side::Callers => &mut trees.left,
                };
                side.entry(origin.from).or_default().push(*node);
                has_parent.insert(*node);
            }
        }
        let order_of = |node: &Node| self.step.get(node).map(|info| info.order);
        for (parent, children) in &mut trees.right {
            children.sort_by_key(|child| {
                let call = index
                    .symbol(child.symbol)
                    .and_then(|symbol| self.call_line(index, *parent, symbol.name()));
                let order = order_of(child);
                (call.is_none(), call, order.is_none(), order)
            });
        }
        let mut roots: Vec<Node> = self
            .nodes
            .iter()
            .copied()
            .filter(|node| !has_parent.contains(node))
            .collect();
        roots.sort_by_key(|root| {
            let order = order_of(root);
            (order.is_none(), order)
        });
        Forest { trees, roots }
    }

    fn place_at(&mut self, node: Node, lanes: &mut Lanes, cross: Cells, spacing: Spacing) -> Cells {
        let rank = self.rank_of(node);
        let size = self.size_of(node);
        self.position.insert(
            node,
            point(self.direction, lanes.start(rank) + spacing.lead_main, cross),
        );
        let end = cross + cross_size(self.direction, size);
        let free = lanes
            .free(rank)
            .max(end + spacing.pad_cross + spacing.box_gap);
        lanes.free.insert(rank, free);
        end
    }

    fn place_siblings(&mut self, siblings: Siblings, lanes: &mut Lanes, spacing: Spacing) {
        let Some(first) = siblings.members.first().copied() else {
            return;
        };
        let lane = self.rank_of(first);
        let parent_cross = siblings
            .parentage
            .parent()
            .and_then(|parent| self.position.get(&parent))
            .map_or(Cells::ZERO, |at| cross_of(self.direction, *at));
        let top = (parent_cross - spacing.lead_cross).max(lanes.free(lane));
        let mut cursor = top + spacing.lead_cross;
        for member in &siblings.members {
            let end = self.place_at(*member, lanes, cursor, spacing);
            cursor = end + spacing.sibling_gap;
        }
        self.siblings.push(siblings);
    }

    fn top_siblings(&self, roots: Vec<Node>) -> Vec<Siblings> {
        let mut grouped: BTreeMap<(Parentage, Rank), Vec<Node>> = BTreeMap::new();
        for root in roots {
            let parentage = if self.step.contains_key(&root) {
                Parentage::Path
            } else {
                Parentage::OffPath
            };
            grouped
                .entry((parentage, self.rank_of(root)))
                .or_default()
                .push(root);
        }
        grouped
            .into_iter()
            .map(|((parentage, _), members)| Siblings { parentage, members })
            .collect()
    }

    pub(crate) fn place_nodes(&mut self, index: &Index) {
        let spacing = Spacing::of(self.direction);
        let mut ranks: BTreeMap<Rank, Vec<Node>> = BTreeMap::new();
        for node in &self.nodes {
            if let Some(rank) = self.rank.get(node) {
                ranks.entry(*rank).or_default().push(*node);
            }
        }
        self.siblings.clear();
        if ranks.is_empty() {
            return;
        }
        let mut lanes = Lanes {
            start: self.lane_starts(&ranks, spacing),
            free: BTreeMap::new(),
        };
        let Forest { trees, roots } = self.trees(index);
        let mut done: BTreeSet<Node> = roots.iter().copied().collect();
        let mut queue = VecDeque::from(roots.clone());
        for siblings in self.top_siblings(roots) {
            self.place_siblings(siblings, &mut lanes, spacing);
        }
        while let Some(parent) = queue.pop_front() {
            for side in [Side::Callees, Side::Callers] {
                let members: Vec<Node> = trees
                    .children(parent, side)
                    .iter()
                    .copied()
                    .filter(|child| done.insert(*child))
                    .collect();
                queue.extend(members.iter().copied());
                let parentage = match side {
                    Side::Callees => Parentage::Callees(parent),
                    Side::Callers => Parentage::Callers(parent),
                };
                self.place_siblings(Siblings { parentage, members }, &mut lanes, spacing);
            }
        }
    }

    fn node_bounds(&self, node: Node) -> Option<CellBounds> {
        let (at, size) = (self.position.get(&node)?, self.size.get(&node)?);
        Some(CellBounds {
            left: at.across,
            top: at.down,
            right: at.across + size.wide,
            bottom: at.down + size.tall,
        })
    }

    pub(crate) fn sibling_bounds(&self, siblings: &Siblings) -> Option<CellBounds> {
        let inner = siblings
            .members
            .iter()
            .filter_map(|member| self.node_bounds(*member))
            .reduce(CellBounds::joined)?;
        Some(CellBounds {
            left: inner.left - GRAPH_PAD_ACROSS,
            top: inner.top - GRAPH_BOX_HEADER,
            right: inner.right + GRAPH_PAD_ACROSS,
            bottom: inner.bottom + GRAPH_PAD_DOWN,
        })
    }

    pub(crate) fn bounds(&self) -> Option<CellBounds> {
        self.nodes
            .iter()
            .filter_map(|node| self.node_bounds(*node))
            .chain(
                self.siblings
                    .iter()
                    .filter_map(|siblings| self.sibling_bounds(siblings)),
            )
            .reduce(CellBounds::joined)
    }
}
