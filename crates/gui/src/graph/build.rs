use std::collections::{BTreeMap, BTreeSet};

use domain::{Index, Line, Span, SymbolId, SymbolName};
use ui::{Count, Extent, Icon, Label, Point, Rect, Run};

use crate::graph::place::Siblings;
use crate::graph::{Around, Button, GraphState, Node, Reveal, Side};
use crate::model::{Model, StepKey, StepSlot, TourSlot};
use crate::panels::Direction;
use crate::theme::{Cells, GRAPH_LEAST_COLUMNS, GRAPH_MOST_COLUMNS, GREEN, TEXT, WEAK};

pub(crate) const PREVIEW_LINES: Count = Count::new(12);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CellPoint {
    pub(crate) across: Cells,
    pub(crate) down: Cells,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CellSize {
    pub(crate) wide: Cells,
    pub(crate) tall: Cells,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Rank(i32);

impl Rank {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    pub(crate) const fn previous(self) -> Self {
        Self(self.0 - 1)
    }

    fn of_depth(depth: domain::Depth) -> Self {
        Self(i32::try_from(depth.value()).unwrap_or(0))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StepInfo {
    pub(crate) order: Count,
    pub(crate) slot: StepSlot,
    pub(crate) number: Label,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Origin {
    pub(crate) from: Node,
    pub(crate) side: Side,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Built {
    pub(crate) nodes: Vec<Node>,
    pub(crate) rank: BTreeMap<Node, Rank>,
    pub(crate) position: BTreeMap<Node, CellPoint>,
    pub(crate) size: BTreeMap<Node, CellSize>,
    pub(crate) range: BTreeMap<Node, Span>,
    pub(crate) view: BTreeMap<Node, Span>,
    pub(crate) by_symbol: BTreeMap<SymbolId, Node>,
    pub(crate) tour: Option<TourSlot>,
    pub(crate) step: BTreeMap<Node, StepInfo>,
    pub(crate) step_parent: BTreeMap<Node, Node>,
    pub(crate) origin: BTreeMap<Node, Origin>,
    pub(crate) has_note: BTreeSet<Node>,
    pub(crate) auto_open: Option<Node>,
    pub(crate) direction: Direction,
    pub(crate) siblings: Vec<Siblings>,
}

pub(crate) struct Shown {
    pub(crate) shown: Count,
    pub(crate) total: Count,
}

pub(crate) struct Header {
    pub(crate) runs: Vec<Run>,
    pub(crate) buttons: Vec<Labelled>,
}

pub(crate) struct Labelled {
    pub(crate) button: Button,
    pub(crate) label: Label,
}

fn line_count(span: Span) -> Count {
    Count::new(usize::try_from(span.count().value()).unwrap_or(0))
}

fn line_at(start: Line, offset: Count) -> Line {
    Line::new(start.value() + u32::try_from(offset.get()).unwrap_or(0))
}

impl Built {
    pub(crate) fn rect_at(&self, node: Node, origin: Point, cell: Extent) -> Option<Rect> {
        if !self.rank.contains_key(&node) {
            return None;
        }
        let at = self.position.get(&node).copied().unwrap_or_default();
        let size = self.size.get(&node).copied().unwrap_or_default();
        Some(Rect::new(
            origin.horizontal + at.across.of(cell.width),
            origin.vertical + at.down.of(cell.height),
            size.wide.of(cell.width),
            size.tall.of(cell.height),
        ))
    }

    pub(crate) fn position(&self, node: Node) -> Option<CellPoint> {
        self.position.get(&node).copied()
    }

    pub(crate) fn focus_node(
        &self,
        focus: Option<SymbolId>,
        step: Option<StepSlot>,
    ) -> Option<Node> {
        let symbol = focus?;
        let stepped = Node { symbol, step };
        if self.rank.contains_key(&stepped) {
            Some(stepped)
        } else {
            self.by_symbol.get(&symbol).copied()
        }
    }

    pub(crate) fn range(&self, node: Node) -> Option<Span> {
        self.range.get(&node).copied()
    }

    pub(crate) fn view(&self, node: Node) -> Option<Span> {
        self.view.get(&node).copied()
    }

    pub(crate) fn shown(&self, graph: &GraphState, node: Node) -> Shown {
        let total = self.view(node).map_or(Count::ZERO, line_count);
        let shown = if graph.collapsed.contains(&node) {
            Count::new(total.get().min(PREVIEW_LINES.get()))
        } else {
            total
        };
        Shown { shown, total }
    }

    pub(crate) fn shown_span(&self, graph: &GraphState, node: Node) -> Option<Span> {
        let view = self.view(node)?;
        let shown = self.shown(graph, node).shown;
        let last = shown.get().checked_sub(1)?;
        Span::new(view.start(), line_at(view.start(), Count::new(last)))
    }

    pub(crate) fn call_line(&self, index: &Index, node: Node, word: &SymbolName) -> Option<Line> {
        let range = self.range(node)?;
        let source = index.file(node.symbol.file())?;
        let last = source.text().count().last()?;
        let span = Span::new(range.start(), range.end().min(last))?;
        span.lines().find(|line| source.call_site(*line, word))
    }

    pub(crate) fn callees_of(&self, index: &Index, node: Node) -> Vec<SymbolId> {
        let Some(symbol) = index.symbol(node.symbol) else {
            return Vec::new();
        };
        if self.range(node) == Some(symbol.span()) {
            return symbol.callees().to_vec();
        }
        symbol
            .callees()
            .iter()
            .copied()
            .filter(|callee| {
                index
                    .symbol(*callee)
                    .is_some_and(|found| self.call_line(index, node, found.name()).is_some())
            })
            .collect()
    }

    fn add(&mut self, index: &Index, node: Node, rank: Rank, range: Span) {
        if self.rank.contains_key(&node) {
            return;
        }
        let last = index
            .file(node.symbol.file())
            .and_then(|source| source.text().count().last())
            .unwrap_or(Line::new(0));
        let end = range.end().min(last);
        let start = range.start().min(end);
        self.rank.insert(node, rank);
        if let Some(span) = Span::new(start, end) {
            self.range.insert(node, span);
        }
        self.by_symbol.entry(node.symbol).or_insert(node);
        self.nodes.push(node);
    }

    pub(crate) fn header(&self, model: &Model, graph: &GraphState, node: Node) -> Header {
        let index = &model.index;
        let mut runs = Vec::new();
        let mut buttons = Vec::new();
        let (Some(symbol), Some(source)) =
            (index.symbol(node.symbol), index.file(node.symbol.file()))
        else {
            return Header { runs, buttons };
        };
        let view = self.view(node).unwrap_or(Span::line(Line::new(0)));
        let Shown { shown, total } = self.shown(graph, node);
        let off_tour = self.tour.is_some() && !self.step.contains_key(&node);
        if let Some(step) = self.step.get(&node) {
            runs.push(Run::new(format!("{} ", step.number.as_str()), GREEN));
        }
        runs.push(Run::new(symbol.name().as_str(), TEXT));
        if off_tour {
            runs.push(Run::new("  off tour", WEAK));
        }
        let range = self.range(node).unwrap_or(view);
        runs.push(Run::new(
            format!(
                "  {}:{}-{}",
                source.path(),
                range.start().number(),
                range.end().number()
            ),
            WEAK,
        ));
        let mut label = |button: Button, text: String| {
            buttons.push(Labelled {
                button,
                label: Label::new(text),
            });
        };
        if total > PREVIEW_LINES {
            label(
                Button::Preview,
                if shown < total { "more" } else { "less" }.to_owned(),
            );
        }
        if !graph.collapsed.contains(&node) {
            if view.start().value() > 0 {
                label(Button::Above, Icon::MoreAbove.glyph().get().to_string());
            }
            if view.end().value() + 1 < source.text().count().value() {
                label(Button::Below, Icon::MoreBelow.glyph().get().to_string());
            }
            if graph.context.contains_key(&node) {
                label(Button::NoContext, "no context".to_owned());
            }
        }
        label(Button::Source, "source".to_owned());
        if off_tour {
            label(Button::Add, "+ step".to_owned());
        }
        let hidden = |list: &[SymbolId]| {
            list.iter()
                .filter(|listed| !self.by_symbol.contains_key(listed))
                .count()
        };
        let outgoing = self.callees_of(index, node);
        if graph.has_reveal(node, Side::Callees) && !outgoing.is_empty() {
            label(Button::Callees, format!("hide {} callees", outgoing.len()));
        } else if hidden(&outgoing) > 0 {
            label(Button::Callees, format!("callees > {}", hidden(&outgoing)));
        }
        let incoming = symbol.callers();
        if graph.has_reveal(node, Side::Callers) && !incoming.is_empty() {
            label(Button::Callers, format!("hide {} callers", incoming.len()));
        } else if hidden(incoming) > 0 {
            label(Button::Callers, format!("{} < callers", hidden(incoming)));
        }
        Header { runs, buttons }
    }

    fn header_cells(&self, model: &Model, graph: &GraphState, node: Node) -> Cells {
        let header = self.header(model, graph, node);
        let text: usize = header.runs.iter().map(|run| run.text.columns()).sum();
        let buttons: usize = header
            .buttons
            .iter()
            .map(|labelled| labelled.label.columns() + 3)
            .sum();
        Cells::of_count(text + buttons + 2)
    }

    fn node_size(&self, model: &Model, graph: &GraphState, node: Node) -> CellSize {
        let Shown { shown, total } = self.shown(graph, node);
        let widest = self
            .shown_span(graph, node)
            .and_then(|span| {
                let text = model.index.file(node.symbol.file())?.text();
                span.lines()
                    .filter_map(|line| text.line(line))
                    .map(|line| line.as_str().chars().count())
                    .max()
            })
            .unwrap_or(0);
        let longest = Cells::of_count(widest) + Cells::new(6);
        let columns = longest
            .clamp(GRAPH_LEAST_COLUMNS, GRAPH_MOST_COLUMNS)
            .max(self.header_cells(model, graph, node));
        let note_rows = Cells::of_count(usize::from(self.has_note.contains(&node)));
        let more = Cells::of_count(usize::from(shown < total));
        let rows = Cells::new(1) + note_rows + Cells::of_count(shown.get()) + more;
        CellSize {
            wide: columns + Cells::new(1),
            tall: rows + Cells::new(1),
        }
    }

    pub(crate) fn measure(&mut self, model: &Model, graph: &GraphState) {
        let sizes: Vec<(Node, CellSize)> = self
            .nodes
            .iter()
            .map(|node| (*node, self.node_size(model, graph, *node)))
            .collect();
        self.size = sizes.into_iter().collect();
    }
}

fn step_symbol(model: &Model, key: StepKey) -> Option<SymbolId> {
    let step = model.step(key)?;
    let index = &model.index;
    let file = index.find_file(step.file())?;
    match step.resolved_symbol() {
        Some(symbol) => Some(symbol),
        None if step.symbol().is_none() => index
            .by_line(step.file(), step.span().start())
            .or_else(|| index.symbol_ids().find(|symbol| symbol.file() == file)),
        None => None,
    }
}

pub(crate) fn rebuild(model: &Model) -> Built {
    let mut built = Built {
        tour: model
            .nav
            .tour()
            .filter(|tour| tour.get() < model.tour_count().get()),
        direction: model.graph.direction,
        ..Built::default()
    };
    if let Some(tour) = built.tour {
        add_tour_nodes(&mut built, model, tour);
    }
    if let Some(root) = model.graph.root
        && !built.by_symbol.contains_key(&root.symbol)
        && let Some(symbol) = model.index.symbol(root.symbol)
    {
        built.add(&model.index, root, Rank::ZERO, symbol.span());
    }
    add_reveals(&mut built, model, model.graph.reveals.clone());
    if let Some(reveals) = add_focus(&mut built, model) {
        add_reveals(&mut built, model, reveals);
    }
    set_views(&mut built, model);
    built
}

fn add_tour_nodes(built: &mut Built, model: &Model, tour: TourSlot) {
    let index = &model.index;
    {
        let mut node_of: BTreeMap<StepSlot, Node> = BTreeMap::new();
        for (order, numbered) in model.numbered(tour).into_iter().enumerate() {
            let key = StepKey {
                tour,
                step: numbered.step,
            };
            let parent = model
                .parent_of(key)
                .and_then(|parent| node_of.get(&parent).copied());
            let Some(symbol) = step_symbol(model, key) else {
                if let Some(parent) = parent {
                    node_of.insert(numbered.step, parent);
                }
                continue;
            };
            let Some(step) = model.step(key) else {
                continue;
            };
            let node = Node {
                symbol,
                step: Some(numbered.step),
            };
            built.add(index, node, Rank::of_depth(numbered.depth), step.span());
            built.step.insert(
                node,
                StepInfo {
                    order: Count::new(order),
                    slot: numbered.step,
                    number: numbered.number,
                },
            );
            if step.note().is_some() {
                built.has_note.insert(node);
            }
            node_of.insert(numbered.step, node);
            if let Some(parent) = parent {
                built.step_parent.insert(node, parent);
            }
        }
    }
}

fn add_focus(built: &mut Built, model: &Model) -> Option<Vec<Reveal>> {
    let graph = &model.graph;
    let index = &model.index;
    let focus = model.nav.focus()?;
    if built.by_symbol.contains_key(&focus) {
        return None;
    }
    let symbol = index.symbol(focus)?;
    let node = Node::off_tour(focus);
    built.add(index, node, Rank::ZERO, symbol.span());
    let mut reveals = graph.reveals.clone();
    if graph.auto_open != Some(node) {
        built.auto_open = Some(node);
        for side in [Side::Callees, Side::Callers] {
            let reveal = Reveal { node, side };
            if !reveals.contains(&reveal) {
                reveals.push(reveal);
            }
        }
    }
    Some(reveals)
}

fn add_reveals(built: &mut Built, model: &Model, reveals: Vec<Reveal>) {
    let index = &model.index;
    for reveal in reveals {
        let node = reveal.node;
        let Some(rank) = built.rank.get(&node).copied() else {
            continue;
        };
        let list = match reveal.side {
            Side::Callees => built.callees_of(index, node),
            Side::Callers => index
                .symbol(node.symbol)
                .map(|symbol| symbol.callers().to_vec())
                .unwrap_or_default(),
        };
        let rank = match reveal.side {
            Side::Callees => rank.next(),
            Side::Callers => rank.previous(),
        };
        for found in list {
            if built.by_symbol.contains_key(&found) {
                continue;
            }
            let Some(symbol) = index.symbol(found) else {
                continue;
            };
            let revealed = Node::off_tour(found);
            built.origin.insert(
                revealed,
                Origin {
                    from: node,
                    side: reveal.side,
                },
            );
            built.add(index, revealed, rank, symbol.span());
        }
    }
}

fn set_views(built: &mut Built, model: &Model) {
    let graph = &model.graph;
    let index = &model.index;
    let mut views = Vec::new();
    for node in &built.nodes {
        let Some(range) = built.range(*node) else {
            continue;
        };
        let around = if graph.collapsed.contains(node) {
            Around::default()
        } else {
            graph.context.get(node).copied().unwrap_or_default()
        };
        let last = index
            .file(node.symbol.file())
            .and_then(|source| source.text().count().last())
            .unwrap_or(Line::new(0));
        let start = Line::new(range.start().value().saturating_sub(around.above.value()));
        let end = Line::new(range.end().value() + around.below.value()).min(last);
        if let Some(view) = Span::new(start, end) {
            views.push((*node, view));
        }
    }
    built.view.extend(views);
}
