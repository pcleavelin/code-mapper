use std::collections::{BTreeMap, BTreeSet};

use domain::Index;

use crate::graph::build::{Built, CellPoint, Rank};
use crate::graph::{Node, Side};
use crate::theme::{Cells, GRAPH_GAP_ACROSS, GRAPH_GAP_DOWN};

struct Trees {
    right: BTreeMap<Node, Vec<Node>>,
    left: BTreeMap<Node, Vec<Node>>,
    height: BTreeMap<Node, Cells>,
}

struct Forest {
    trees: Trees,
    roots: Vec<Node>,
}

impl Trees {
    fn children(map: &BTreeMap<Node, Vec<Node>>, node: Node) -> &[Node] {
        map.get(&node).map_or(&[], Vec::as_slice)
    }
}

impl Built {
    fn tall(&self, node: Node) -> Cells {
        self.size.get(&node).map_or(Cells::ZERO, |size| size.tall)
    }

    fn measure_tree(&self, trees: &mut Trees, root: Node, seen: &mut BTreeSet<Node>) -> Cells {
        if !seen.insert(root) {
            return Cells::ZERO;
        }
        let mut stacks = Vec::new();
        for side in [Side::Callees, Side::Callers] {
            let children = match side {
                Side::Callees => Trees::children(&trees.right, root).to_vec(),
                Side::Callers => Trees::children(&trees.left, root).to_vec(),
            };
            let mut height = Cells::ZERO;
            for child in children {
                let child_height = self.measure_tree(trees, child, seen);
                if child_height > Cells::ZERO {
                    height += child_height + GRAPH_GAP_DOWN;
                }
            }
            stacks.push((height - GRAPH_GAP_DOWN).max(Cells::ZERO));
        }
        let height = stacks.into_iter().fold(self.tall(root), Cells::max);
        trees.height.insert(root, height);
        height
    }

    fn place_tree(
        &mut self,
        trees: &Trees,
        root: Node,
        top: Cells,
        rank_across: &BTreeMap<Rank, Cells>,
        done: &mut BTreeSet<Node>,
    ) {
        if !done.insert(root) {
            return;
        }
        let block = trees.height.get(&root).copied().unwrap_or(Cells::ZERO);
        let own = self.tall(root);
        let across = self
            .rank
            .get(&root)
            .and_then(|rank| rank_across.get(rank))
            .copied()
            .unwrap_or(Cells::ZERO);
        self.position.insert(
            root,
            CellPoint {
                across,
                down: top + (block - own) / 2,
            },
        );
        for children in [
            Trees::children(&trees.right, root),
            Trees::children(&trees.left, root),
        ] {
            let children: Vec<Node> = children
                .iter()
                .copied()
                .filter(|child| trees.height.contains_key(child) && !done.contains(child))
                .collect();
            let stacked = children
                .iter()
                .map(|child| {
                    trees.height.get(child).copied().unwrap_or(Cells::ZERO) + GRAPH_GAP_DOWN
                })
                .fold(Cells::ZERO, |sum, height| sum + height)
                - GRAPH_GAP_DOWN;
            let mut cursor = top + (block - stacked.max(Cells::ZERO)) / 2;
            for child in children {
                self.place_tree(trees, child, cursor, rank_across, done);
                cursor += trees.height.get(&child).copied().unwrap_or(Cells::ZERO) + GRAPH_GAP_DOWN;
            }
        }
    }

    fn rank_across(&self, ranks: &BTreeMap<Rank, Vec<Node>>) -> BTreeMap<Rank, Cells> {
        let mut across: BTreeMap<Rank, Cells> = BTreeMap::new();
        let (Some(first), Some(last)) = (
            ranks.keys().next().copied(),
            ranks.keys().next_back().copied(),
        ) else {
            return across;
        };
        let anchor = Rank::ZERO.clamp(first, last);
        let widths: BTreeMap<Rank, Cells> = ranks
            .iter()
            .map(|(rank, nodes)| {
                let widest = nodes
                    .iter()
                    .filter_map(|node| self.size.get(node))
                    .map(|size| size.wide)
                    .max()
                    .unwrap_or(Cells::ZERO);
                (*rank, widest)
            })
            .collect();
        let width_of = |rank: Rank| widths.get(&rank).copied().unwrap_or(Cells::ZERO);
        across.insert(anchor, Cells::ZERO);
        let mut right_rank = anchor;
        while right_rank < last {
            let before = across.get(&right_rank).copied().unwrap_or(Cells::ZERO);
            across.insert(
                right_rank.next(),
                before + width_of(right_rank) + GRAPH_GAP_ACROSS,
            );
            right_rank = right_rank.next();
        }
        let mut left_rank = anchor;
        while left_rank > first {
            let after = across.get(&left_rank).copied().unwrap_or(Cells::ZERO);
            let previous = left_rank.previous();
            across.insert(previous, after - width_of(previous) - GRAPH_GAP_ACROSS);
            left_rank = previous;
        }
        across
    }

    fn trees(&self, index: &Index) -> Forest {
        let mut trees = Trees {
            right: BTreeMap::new(),
            left: BTreeMap::new(),
            height: BTreeMap::new(),
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

    fn compact(&mut self, ranks: &mut BTreeMap<Rank, Vec<Node>>) {
        for nodes in ranks.values_mut() {
            nodes.sort_by_key(|node| self.position.get(node).map(|at| at.down));
            let mut bottom: Option<Cells> = None;
            for node in nodes.iter() {
                let Some(at) = self.position.get_mut(node) else {
                    continue;
                };
                if let Some(bottom) = bottom {
                    at.down = at.down.max(bottom + GRAPH_GAP_DOWN);
                }
                let down = at.down;
                bottom = Some(down + self.size.get(node).map_or(Cells::ZERO, |size| size.tall));
            }
        }
    }

    pub(crate) fn place_nodes(&mut self, index: &Index) {
        let mut ranks: BTreeMap<Rank, Vec<Node>> = BTreeMap::new();
        for node in &self.nodes {
            if let Some(rank) = self.rank.get(node) {
                ranks.entry(*rank).or_default().push(*node);
            }
        }
        if ranks.is_empty() {
            return;
        }
        let across = self.rank_across(&ranks);
        let Forest { mut trees, roots } = self.trees(index);
        let mut seen = BTreeSet::new();
        for root in &roots {
            self.measure_tree(&mut trees, *root, &mut seen);
        }
        let mut done = BTreeSet::new();
        let mut cursor = Cells::ZERO;
        for root in &roots {
            if done.contains(root) {
                continue;
            }
            self.place_tree(&trees, *root, cursor, &across, &mut done);
            cursor += trees.height.get(root).copied().unwrap_or(Cells::ZERO) + GRAPH_GAP_DOWN * 2;
        }
        self.compact(&mut ranks);
    }

    pub(crate) fn bounds(&self) -> Option<CellBounds> {
        let mut bounds: Option<CellBounds> = None;
        for node in &self.nodes {
            let (Some(at), Some(size)) = (self.position.get(node), self.size.get(node)) else {
                continue;
            };
            let right = at.across + size.wide;
            let bottom = at.down + size.tall;
            bounds = Some(match bounds {
                None => CellBounds {
                    left: at.across,
                    top: at.down,
                    right,
                    bottom,
                },
                Some(known) => CellBounds {
                    left: known.left.min(at.across),
                    top: known.top.min(at.down),
                    right: known.right.max(right),
                    bottom: known.bottom.max(bottom),
                },
            });
        }
        bounds
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CellBounds {
    pub(crate) left: Cells,
    pub(crate) top: Cells,
    pub(crate) right: Cells,
    pub(crate) bottom: Cells,
}
