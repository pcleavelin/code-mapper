use domain::{LayoutPanel, LayoutSplit, LayoutTree, Share, SplitDirection, ViewKey};

use crate::wire::{WireDirection, WireTree, WireView};

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
            direction,
            share,
            first,
            second,
        } => {
            let direction = match direction {
                WireDirection::Across => SplitDirection::Across,
                WireDirection::Down => SplitDirection::Down,
            };
            Some(LayoutTree::Split(LayoutSplit::new(
                direction,
                Share::permille(*share)?,
                layout(first)?,
                layout(second)?,
            )))
        }
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
            direction: match split.direction() {
                SplitDirection::Across => WireDirection::Across,
                SplitDirection::Down => WireDirection::Down,
            },
            share: split.share().get(),
            first: Box::new(wire(split.first())),
            second: Box::new(wire(split.second())),
        },
    }
}
