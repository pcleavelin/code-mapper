use domain::{
    BaseFontSize, FontFamily, LayoutPanel, LayoutSplit, LayoutTree, Settings, Share,
    SplitDirection, Theme, ViewKey,
};

use crate::wire::{WireDirection, WireSetting, WireSettings, WireTheme, WireTree, WireView};

pub(crate) fn layout(wire: &WireTree) -> Option<LayoutTree> {
    match wire {
        WireTree::Panel { views } => {
            let keys = views
                .iter()
                .map(|view| ViewKey::new(&view.name))
                .collect::<Option<Vec<ViewKey>>>()?;
            let shown = views
                .iter()
                .find(|view| view.shown)
                .and_then(|view| ViewKey::new(&view.name));
            Some(LayoutTree::Panel(LayoutPanel::new(keys, shown)))
        }
        WireTree::Split {
            direction: split,
            share,
            first,
            second,
        } => Some(LayoutTree::Split(LayoutSplit::new(
            direction(*split),
            Share::permille(*share)?,
            layout(first)?,
            layout(second)?,
        ))),
    }
}

pub(crate) fn wire(layout: &LayoutTree) -> WireTree {
    match layout {
        LayoutTree::Panel(panel) => WireTree::Panel {
            views: panel
                .views()
                .iter()
                .map(|key| WireView {
                    name: key.as_str().to_owned(),
                    shown: panel.shown() == Some(key),
                })
                .collect(),
        },
        LayoutTree::Split(split) => WireTree::Split {
            direction: wire_direction(split.direction()),
            share: split.share().get(),
            first: Box::new(wire(split.first())),
            second: Box::new(wire(split.second())),
        },
    }
}

const fn direction(wire: WireDirection) -> SplitDirection {
    match wire {
        WireDirection::Right => SplitDirection::Right,
        WireDirection::Down => SplitDirection::Down,
    }
}

const fn wire_direction(direction: SplitDirection) -> WireDirection {
    match direction {
        SplitDirection::Right => WireDirection::Right,
        SplitDirection::Down => WireDirection::Down,
    }
}

pub(crate) fn settings(wire: &WireSettings) -> Settings {
    wire.entries
        .iter()
        .fold(Settings::default(), |settings, entry| match entry {
            WireSetting::Theme(WireTheme::Dark) => settings.with_theme(Theme::Dark),
            WireSetting::Theme(WireTheme::Light) => settings.with_theme(Theme::Light),
            WireSetting::Font(family) => match FontFamily::new(family) {
                Some(family) => settings.with_font(Some(family)),
                None => settings,
            },
            WireSetting::Size(size) => match BaseFontSize::new(*size) {
                Some(size) => settings.with_size(size),
                None => settings,
            },
            WireSetting::Graph(growth) => settings.with_graph(direction(*growth)),
        })
}

pub(crate) fn wire_settings(settings: &Settings) -> WireSettings {
    let theme = match settings.theme() {
        Theme::Dark => WireTheme::Dark,
        Theme::Light => WireTheme::Light,
    };
    let mut entries = vec![WireSetting::Theme(theme)];
    if let Some(family) = settings.font() {
        entries.push(WireSetting::Font(family.as_str().to_owned()));
    }
    entries.push(WireSetting::Size(settings.size().get()));
    entries.push(WireSetting::Graph(wire_direction(settings.graph())));
    WireSettings { entries }
}
