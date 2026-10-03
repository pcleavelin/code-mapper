#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SplitDirection {
    #[default]
    Right,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Share(u16);

impl Share {
    pub const WHOLE: Self = Self(1000);

    pub fn permille(value: u16) -> Option<Self> {
        (value <= Self::WHOLE.0).then_some(Self(value))
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewKey(String);

impl ViewKey {
    pub fn new(name: &str) -> Option<Self> {
        (!name.is_empty() && name.chars().all(|letter| letter.is_ascii_alphanumeric()))
            .then(|| Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutPanel {
    views: Vec<ViewKey>,
    shown: Option<ViewKey>,
}

impl LayoutPanel {
    pub fn new(views: Vec<ViewKey>, shown: Option<ViewKey>) -> Self {
        let shown = shown.filter(|key| views.contains(key));
        Self { views, shown }
    }

    pub fn views(&self) -> &[ViewKey] {
        &self.views
    }

    pub const fn shown(&self) -> Option<&ViewKey> {
        self.shown.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutSplit {
    direction: SplitDirection,
    share: Share,
    first: Box<LayoutTree>,
    second: Box<LayoutTree>,
}

impl LayoutSplit {
    pub fn new(
        direction: SplitDirection,
        share: Share,
        first: LayoutTree,
        second: LayoutTree,
    ) -> Self {
        Self {
            direction,
            share,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    pub const fn direction(&self) -> SplitDirection {
        self.direction
    }

    pub const fn share(&self) -> Share {
        self.share
    }

    pub fn first(&self) -> &LayoutTree {
        &self.first
    }

    pub fn second(&self) -> &LayoutTree {
        &self.second
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutTree {
    Split(LayoutSplit),
    Panel(LayoutPanel),
}
