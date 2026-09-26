use std::fs;
use std::path::PathBuf;

use crate::files::write;
use crate::source::workspace_files;
use crate::text::{Count, Hasher, Message, Root, Stamp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Light {
    Green,
    Red,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GateState {
    pub(crate) stamp: Stamp,
    pub(crate) light: Light,
    pub(crate) repeats: Count,
    pub(crate) failures: Message,
}

fn directory(root: &Root) -> PathBuf {
    root.path().join("target").join("xtask").join("gate")
}

pub(crate) fn stamp(root: &Root) -> Stamp {
    let mut hasher = Hasher::new();
    for path in workspace_files(root) {
        hasher.feed(path.as_str().as_bytes());
        hasher.feed(&[0]);
        if let Ok(bytes) = fs::read(root.join(&path)) {
            hasher.feed(&bytes);
        }
        hasher.feed(&[0]);
    }
    hasher.finish()
}

impl GateState {
    pub(crate) fn load(root: &Root) -> Option<Self> {
        let text = fs::read_to_string(directory(root).join("state")).ok()?;
        let mut stamp = None;
        let mut light = None;
        let mut repeats = Count::ZERO;
        for line in text.lines() {
            match line.split_once(' ') {
                Some(("stamp", value)) => stamp = Stamp::parse(value),
                Some(("result", "green")) => light = Some(Light::Green),
                Some(("result", "red")) => light = Some(Light::Red),
                Some(("repeats", value)) => {
                    repeats = Count::new(value.trim().parse().unwrap_or_default());
                }
                _ => {}
            }
        }
        let failures =
            Message::new(fs::read_to_string(directory(root).join("failures")).unwrap_or_default());
        Some(Self {
            stamp: stamp?,
            light: light?,
            repeats,
            failures,
        })
    }

    pub(crate) fn save(&self, root: &Root) -> Result<(), Message> {
        let folder = directory(root);
        let light = match self.light {
            Light::Green => "green",
            Light::Red => "red",
        };
        write(
            &folder.join("state"),
            &Message::new(format!(
                "stamp {}\nresult {light}\nrepeats {}\n",
                self.stamp, self.repeats
            )),
        )?;
        write(&folder.join("failures"), &self.failures)
    }
}
