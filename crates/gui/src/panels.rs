use std::fmt;
use std::mem;

use domain::{LayoutPanel, LayoutSplit, LayoutTree, Share, SplitDirection, ViewKey};
use strum::VariantArray;
use ui::{Axis, Count, Extent, FontSize, Label, Point, Px, Rect};

use crate::model::Tab;
use crate::nav::Ticket;
use crate::theme::{
    DEFAULT_BOTTOM, DEFAULT_LEFT, DEFAULT_RIGHT, EDGE_ZONE, GRAB_REACH, HALF, RATIO_WHOLE,
    SPLIT_LEAST,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, VariantArray)]
pub(crate) enum View {
    Tours,
    Symbols,
    Files,
    Tour,
    Diff,
    Graph,
    Source,
    Search,
    References,
    Console,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ViewName(&'static str);

impl ViewName {
    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for ViewName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl View {
    pub(crate) const fn name(self) -> ViewName {
        ViewName(match self {
            Self::Tours => "Tours",
            Self::Symbols => "Symbols",
            Self::Files => "Files",
            Self::Tour => "Tour",
            Self::Diff => "Diff",
            Self::Graph => "Graph",
            Self::Source => "Source",
            Self::Search => "Search",
            Self::References => "References",
            Self::Console => "Console",
        })
    }

    pub(crate) const fn tab(self) -> Option<Tab> {
        match self {
            Self::Tour => Some(Tab::Tour),
            Self::Diff => Some(Tab::Diff),
            Self::Graph => Some(Tab::Graph),
            Self::Source => Some(Tab::Source),
            Self::Search => Some(Tab::Search),
            Self::Tours | Self::Symbols | Self::Files | Self::References | Self::Console => None,
        }
    }

    pub(crate) const fn of_tab(tab: Tab) -> Self {
        match tab {
            Tab::Tour => Self::Tour,
            Tab::Diff => Self::Diff,
            Tab::Graph => Self::Graph,
            Tab::Source => Self::Source,
            Tab::Search => Self::Search,
        }
    }

    pub(crate) fn named(name: &Label) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|view| view.name().as_str().eq_ignore_ascii_case(name.as_str()))
    }

    pub(crate) fn matching(search: &Label) -> Vec<Self> {
        let search = search.as_str().to_lowercase();
        Self::VARIANTS
            .iter()
            .copied()
            .filter(|view| view.name().as_str().to_lowercase().contains(&search))
            .collect()
    }

    pub(crate) fn position(self) -> Count {
        Count::new(
            Self::VARIANTS
                .iter()
                .position(|view| *view == self)
                .unwrap_or_default(),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct BranchId(u32);

impl BranchId {
    pub(crate) fn number(self) -> usize {
        usize::try_from(self.0).unwrap_or_default()
    }

    const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for BranchId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Ratio(u16);

impl Ratio {
    pub(crate) const fn permille(value: u16) -> Self {
        Self(value)
    }

    pub(crate) fn of(part: Px, whole: Px) -> Self {
        if whole <= Px::ZERO {
            return HALF;
        }
        let value = (part.wide() * i64::from(RATIO_WHOLE.0) / whole.wide())
            .clamp(0, i64::from(RATIO_WHOLE.0));
        Self(u16::try_from(value).unwrap_or(HALF.0))
    }

    fn apply(self, whole: Px) -> Px {
        Px::of_wide(whole.wide() * i64::from(self.0) / i64::from(RATIO_WHOLE.0))
    }
}

impl fmt::Display for Ratio {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Panel {
    id: BranchId,
    views: Vec<View>,
    active: Option<View>,
}

impl Panel {
    fn new(id: BranchId, views: Vec<View>) -> Self {
        let active = views.first().copied();
        Self { id, views, active }
    }

    pub(crate) const fn id(&self) -> BranchId {
        self.id
    }

    pub(crate) fn views(&self) -> &[View] {
        &self.views
    }

    pub(crate) const fn active(&self) -> Option<View> {
        self.active
    }

    fn holds(&self, view: View) -> bool {
        self.views.contains(&view)
    }

    fn remove(&mut self, view: View) {
        let Some(at) = self.views.iter().position(|held| *held == view) else {
            return;
        };
        self.views.remove(at);
        if self.active == Some(view) {
            self.active = self.views.get(at).or_else(|| self.views.last()).copied();
        }
    }

    fn add(&mut self, view: View) {
        if !self.holds(view) {
            self.views.push(view);
        }
        self.active = Some(view);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Direction {
    #[default]
    Right,
    Down,
}

impl Direction {
    pub(crate) const fn turned(self) -> Self {
        match self {
            Self::Right => Self::Down,
            Self::Down => Self::Right,
        }
    }

    const fn axis(self) -> Axis {
        match self {
            Self::Right => Axis::Horizontal,
            Self::Down => Axis::Vertical,
        }
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Right => "right",
            Self::Down => "down",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Split {
    id: BranchId,
    direction: Direction,
    ratio: Ratio,
    first: Box<Branch>,
    second: Box<Branch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Divided {
    pub(crate) first: Rect,
    pub(crate) divider: Rect,
    pub(crate) second: Rect,
}

impl Split {
    pub(crate) const fn id(&self) -> BranchId {
        self.id
    }

    pub(crate) const fn direction(&self) -> Direction {
        self.direction
    }

    pub(crate) fn first(&self) -> &Branch {
        &self.first
    }

    pub(crate) fn second(&self) -> &Branch {
        &self.second
    }

    pub(crate) fn divide(&self, rect: Rect, divider: Px, least: Extent) -> Divided {
        let axis = self.direction.axis();
        let whole = rect.extent().along(axis);
        let room = (whole - divider).max(Px::ZERO);
        let floor = least.along(axis).min(room / 2);
        let first = self.ratio.apply(room).clamp(floor, room - floor);
        let second = room - first;
        let origin = rect.origin();
        let extent = rect.extent();
        let at = |offset: Px, size: Px| {
            Rect::at(
                origin.with(axis, origin.along(axis) + offset),
                extent.with(axis, size),
            )
        };
        Divided {
            first: at(Px::ZERO, first),
            divider: at(first, divider.min(whole)),
            second: at(first + divider, second),
        }
    }

    pub(crate) fn ratio_at(&self, rect: Rect, divider: Px, mouse: Point) -> Ratio {
        let axis = self.direction.axis();
        let room = rect.extent().along(axis) - divider;
        Ratio::of(
            mouse.along(axis) - rect.origin().along(axis) - divider / 2,
            room,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Branch {
    Panel(Panel),
    Split(Split),
}

impl Branch {
    const fn id(&self) -> BranchId {
        match self {
            Self::Panel(panel) => panel.id,
            Self::Split(split) => split.id,
        }
    }

    fn placeholder() -> Self {
        Self::Panel(Panel::new(BranchId(0), Vec::new()))
    }

    fn panels<'node>(&'node self, out: &mut Vec<&'node Panel>) {
        match self {
            Self::Panel(panel) => out.push(panel),
            Self::Split(split) => {
                split.first.panels(out);
                split.second.panels(out);
            }
        }
    }

    fn splits<'node>(&'node self, out: &mut Vec<&'node Split>) {
        if let Self::Split(split) = self {
            out.push(split);
            split.first.splits(out);
            split.second.splits(out);
        }
    }

    fn panel_mut(&mut self, id: BranchId) -> Option<&mut Panel> {
        match self {
            Self::Panel(panel) => (panel.id == id).then_some(panel),
            Self::Split(split) => split
                .first
                .panel_mut(id)
                .or_else(|| split.second.panel_mut(id)),
        }
    }

    fn split_mut(&mut self, id: BranchId) -> Option<&mut Split> {
        let Self::Split(split) = self else {
            return None;
        };
        if split.id == id {
            return Some(split);
        }
        if let Some(found) = split.first.split_mut(id) {
            return Some(found);
        }
        split.second.split_mut(id)
    }

    fn replace(&mut self, id: BranchId, change: &mut dyn FnMut(Self) -> Self) -> bool {
        if self.id() == id {
            let taken = mem::replace(self, Self::placeholder());
            *self = change(taken);
            return true;
        }
        match self {
            Self::Panel(_) => false,
            Self::Split(split) => {
                split.first.replace(id, change) || split.second.replace(id, change)
            }
        }
    }

    fn close(&mut self, id: BranchId) -> bool {
        let Self::Split(split) = self else {
            return false;
        };
        let first = split.first.id() == id;
        if first || split.second.id() == id {
            let taken = mem::replace(self, Self::placeholder());
            if let Self::Split(owned) = taken {
                *self = if first { *owned.second } else { *owned.first };
            }
            return true;
        }
        split.first.close(id) || split.second.close(id)
    }
}

impl fmt::Display for Branch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Panel(panel) => {
                let views: Vec<String> = panel
                    .views
                    .iter()
                    .map(|view| {
                        let star = if panel.active == Some(*view) { "*" } else { "" };
                        format!("{}{star}", view.name())
                    })
                    .collect();
                write!(formatter, "{}[{}]", panel.id, views.join(" "))
            }
            Self::Split(split) => write!(
                formatter,
                "{}{}({} {} {})",
                split.direction, split.id, split.ratio, split.first, split.second
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    Middle,
    Edge(Edge),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DropTarget {
    pub(crate) panel: BranchId,
    pub(crate) zone: Zone,
    pub(crate) band: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Moving {
    Stay,
    Moving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Grab {
    pub(crate) view: View,
    from: Point,
    moving: Moving,
}

impl Grab {
    pub(crate) fn is_moving(self) -> bool {
        self.moving == Moving::Moving
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Panels {
    root: Branch,
    next: BranchId,
    grab: Option<Grab>,
    picker: Option<BranchId>,
    revealed: Tab,
    answered: Ticket,
}

impl Default for Panels {
    fn default() -> Self {
        let mut panels = Self {
            root: Branch::placeholder(),
            next: BranchId(0),
            grab: None,
            picker: None,
            revealed: Tab::Tour,
            answered: Ticket::default(),
        };
        let nav = panels.panel(vec![View::Tours, View::Symbols, View::Files]);
        let centre = panels.panel(vec![
            View::Tour,
            View::Diff,
            View::Graph,
            View::Source,
            View::Search,
        ]);
        let references = panels.panel(vec![View::References]);
        let console = panels.panel(vec![View::Console]);
        let right = panels.split(Direction::Right, DEFAULT_RIGHT, centre, references);
        let row = panels.split(Direction::Right, DEFAULT_LEFT, nav, right);
        panels.root = panels.split(Direction::Down, DEFAULT_BOTTOM, row, console);
        panels
    }
}

pub(crate) fn divider_width(font: FontSize) -> Px {
    Px::of_count(usize::try_from(font.get() / 2).unwrap_or(0)).max(SPLIT_LEAST)
}

fn layout_of(branch: &Branch) -> LayoutTree {
    match branch {
        Branch::Panel(panel) => {
            let key = |view: View| ViewKey::new(view.name().as_str());
            LayoutTree::Panel(LayoutPanel::new(
                panel.views.iter().filter_map(|view| key(*view)).collect(),
                panel.active.and_then(key),
            ))
        }
        Branch::Split(split) => LayoutTree::Split(LayoutSplit::new(
            match split.direction {
                Direction::Right => SplitDirection::Right,
                Direction::Down => SplitDirection::Down,
            },
            Share::permille(split.ratio.0).unwrap_or(Share::WHOLE),
            layout_of(&split.first),
            layout_of(&split.second),
        )),
    }
}

impl Panels {
    pub(crate) fn from_layout(layout: &LayoutTree) -> Self {
        let mut panels = Self::default();
        let mut placed = Vec::new();
        panels.next = BranchId(0);
        panels.root = panels.restore(layout, &mut placed);
        panels
    }

    fn restore(&mut self, layout: &LayoutTree, placed: &mut Vec<View>) -> Branch {
        match layout {
            LayoutTree::Panel(saved) => {
                let named = |key: &ViewKey| View::named(&Label::new(key.as_str()));
                let views: Vec<View> = saved
                    .views()
                    .iter()
                    .filter_map(named)
                    .filter(|view| !placed.contains(view))
                    .collect();
                placed.extend(views.iter().copied());
                let id = self.fresh();
                let mut panel = Panel::new(id, views);
                if let Some(shown) = saved.shown().and_then(named)
                    && panel.holds(shown)
                {
                    panel.active = Some(shown);
                }
                Branch::Panel(panel)
            }
            LayoutTree::Split(saved) => {
                let first = self.restore(saved.first(), placed);
                let second = self.restore(saved.second(), placed);
                let direction = match saved.direction() {
                    SplitDirection::Right => Direction::Right,
                    SplitDirection::Down => Direction::Down,
                };
                self.split(direction, Ratio(saved.share().get()), first, second)
            }
        }
    }

    pub(crate) fn layout(&self) -> LayoutTree {
        layout_of(&self.root)
    }

    fn fresh(&mut self) -> BranchId {
        let id = self.next;
        self.next = id.next();
        id
    }

    fn panel(&mut self, views: Vec<View>) -> Branch {
        let id = self.fresh();
        Branch::Panel(Panel::new(id, views))
    }

    fn split(
        &mut self,
        direction: Direction,
        ratio: Ratio,
        first: Branch,
        second: Branch,
    ) -> Branch {
        Branch::Split(Split {
            id: self.fresh(),
            direction,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        })
    }

    pub(crate) const fn root(&self) -> &Branch {
        &self.root
    }

    pub(crate) const fn grab(&self) -> Option<Grab> {
        self.grab
    }

    pub(crate) const fn picker(&self) -> Option<BranchId> {
        self.picker
    }

    pub(crate) fn panels(&self) -> Vec<&Panel> {
        let mut out = Vec::new();
        self.root.panels(&mut out);
        out
    }

    pub(crate) fn splits(&self) -> Vec<&Split> {
        let mut out = Vec::new();
        self.root.splits(&mut out);
        out
    }

    pub(crate) fn holder(&self, view: View) -> Option<BranchId> {
        self.panels()
            .into_iter()
            .find(|panel| panel.holds(view))
            .map(Panel::id)
    }

    pub(crate) fn is_shown(&self, view: View) -> bool {
        self.panels().iter().any(|panel| panel.active == Some(view))
    }

    pub(crate) fn is_single(&self) -> bool {
        matches!(self.root, Branch::Panel(_))
    }

    pub(crate) fn activate(&mut self, view: View) {
        if let Some(id) = self.holder(view)
            && let Some(panel) = self.root.panel_mut(id)
        {
            panel.active = Some(view);
        }
    }

    pub(crate) fn reveal(&mut self, tab: Tab, asked: Ticket) {
        if self.answered == asked {
            return;
        }
        self.answered = asked;
        let before = View::of_tab(self.revealed);
        self.revealed = tab;
        let view = View::of_tab(tab);
        if self.holder(view).is_some() {
            self.activate(view);
            return;
        }
        let home = self
            .holder(before)
            .or_else(|| {
                View::VARIANTS
                    .iter()
                    .copied()
                    .filter(|other| other.tab().is_some())
                    .find_map(|other| self.holder(other))
            })
            .or_else(|| self.panels().first().map(|panel| panel.id));
        if let Some(home) = home {
            self.put(view, home);
        }
    }

    fn take(&mut self, view: View, keep: Option<BranchId>) {
        let Some(from) = self.holder(view) else {
            return;
        };
        let emptied = self.root.panel_mut(from).is_some_and(|panel| {
            panel.remove(view);
            panel.views.is_empty()
        });
        if emptied && keep != Some(from) {
            self.close(from);
        }
    }

    pub(crate) fn close_view(&mut self, view: View) {
        self.take(view, None);
    }

    pub(crate) fn put(&mut self, view: View, panel: BranchId) {
        self.take(view, Some(panel));
        if let Some(found) = self.root.panel_mut(panel) {
            found.add(view);
        }
    }

    pub(crate) fn split_panel(&mut self, panel: BranchId, direction: Direction) {
        let fresh = self.fresh();
        let split = self.fresh();
        let replaced = self.root.replace(panel, &mut |old| {
            Branch::Split(Split {
                id: split,
                direction,
                ratio: HALF,
                first: Box::new(old),
                second: Box::new(Branch::Panel(Panel::new(fresh, Vec::new()))),
            })
        });
        if replaced {
            self.picker = Some(fresh);
        }
    }

    pub(crate) fn close(&mut self, panel: BranchId) {
        if self.root.close(panel) && self.picker == Some(panel) {
            self.picker = None;
        }
    }

    pub(crate) fn resize(&mut self, split: BranchId, ratio: Ratio) {
        if let Some(found) = self.root.split_mut(split) {
            found.ratio = ratio.clamp(Ratio(0), RATIO_WHOLE);
        }
    }

    pub(crate) fn toggle_picker(&mut self, panel: BranchId) {
        self.picker = if self.picker == Some(panel) {
            None
        } else {
            Some(panel)
        };
    }

    pub(crate) fn close_picker(&mut self) {
        self.picker = None;
    }

    pub(crate) fn pick(&mut self, panel: BranchId, view: View) {
        self.put(view, panel);
        self.picker = None;
    }

    pub(crate) fn take_hold(&mut self, view: View, from: Point) {
        self.grab = Some(Grab {
            view,
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

    pub(crate) fn drop_view(&mut self, view: View, target: DropTarget) {
        let Some(from) = self.holder(view) else {
            return;
        };
        let alone = self
            .panels()
            .iter()
            .any(|panel| panel.id == from && panel.views.len() == 1);
        let side = match target.zone {
            Zone::Middle => {
                self.put(view, target.panel);
                return;
            }
            Zone::Edge(side) => side,
        };
        if alone && from == target.panel {
            return;
        }
        self.take(view, Some(target.panel));
        let fresh = self.fresh();
        let split = self.fresh();
        let (direction, before) = match side {
            Edge::Left => (Direction::Right, true),
            Edge::Right => (Direction::Right, false),
            Edge::Top => (Direction::Down, true),
            Edge::Bottom => (Direction::Down, false),
        };
        self.root.replace(target.panel, &mut |old| {
            let new = Branch::Panel(Panel::new(fresh, vec![view]));
            let (first, second) = if before { (new, old) } else { (old, new) };
            Branch::Split(Split {
                id: split,
                direction,
                ratio: HALF,
                first: Box::new(first),
                second: Box::new(second),
            })
        });
    }

    pub(crate) fn drop_target(
        &self,
        mouse: Point,
        rect_of: impl Fn(BranchId) -> Option<Rect>,
    ) -> Option<DropTarget> {
        self.panels().into_iter().find_map(|panel| {
            let rect = rect_of(panel.id).filter(|rect| rect.contains(mouse))?;
            let zone = zone(rect, mouse);
            Some(DropTarget {
                panel: panel.id,
                zone,
                band: band(rect, zone),
            })
        })
    }
}

fn zone(rect: Rect, mouse: Point) -> Zone {
    let reach = |size: Px| Ratio::permille(EDGE_ZONE.0).apply(size);
    let across = reach(rect.width);
    let down = reach(rect.height);
    let distances = [
        (mouse.horizontal - rect.left, across, Edge::Left),
        (rect.right() - mouse.horizontal, across, Edge::Right),
        (mouse.vertical - rect.top, down, Edge::Top),
        (rect.bottom() - mouse.vertical, down, Edge::Bottom),
    ];
    distances
        .into_iter()
        .filter(|(distance, limit, _)| distance < limit)
        .min_by_key(|(distance, limit, _)| distance.wide() * 1000 / limit.wide().max(1))
        .map_or(Zone::Middle, |(_, _, side)| Zone::Edge(side))
}

fn band(rect: Rect, zone: Zone) -> Rect {
    let half_width = rect.width / 2;
    let half_height = rect.height / 2;
    match zone {
        Zone::Middle => rect,
        Zone::Edge(Edge::Left) => Rect::new(rect.left, rect.top, half_width, rect.height),
        Zone::Edge(Edge::Right) => {
            Rect::new(rect.right() - half_width, rect.top, half_width, rect.height)
        }
        Zone::Edge(Edge::Top) => Rect::new(rect.left, rect.top, rect.width, half_height),
        Zone::Edge(Edge::Bottom) => Rect::new(
            rect.left,
            rect.bottom() - half_height,
            rect.width,
            half_height,
        ),
    }
}

impl fmt::Display for Panels {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.root)?;
        if let Some(picker) = self.picker {
            write!(formatter, " picker={picker}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
