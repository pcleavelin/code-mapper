use std::fs;
use std::path::PathBuf;

use crate::files::write;
use crate::source::workspace_files;
use crate::text::{Count, Hasher, Literal, Message, Root, Stamp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Light {
    Green,
    Red,
}

impl Light {
    const ALL: [Self; 2] = [Self::Green, Self::Red];

    const fn name(self) -> Literal {
        match self {
            Self::Green => Literal::new("green"),
            Self::Red => Literal::new("red"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Key {
    Stamp,
    Result,
    Repeats,
}

impl Key {
    const ALL: [Self; 3] = [Self::Stamp, Self::Result, Self::Repeats];

    const fn name(self) -> Literal {
        match self {
            Self::Stamp => Literal::new("stamp"),
            Self::Result => Literal::new("result"),
            Self::Repeats => Literal::new("repeats"),
        }
    }
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
            let Some((key_word, value)) = line.split_once(' ') else {
                continue;
            };
            match Key::ALL
                .into_iter()
                .find(|key| key.name().as_str() == key_word)
            {
                Some(Key::Stamp) => stamp = Stamp::parse(value),
                Some(Key::Result) => {
                    if let Some(known) = Light::ALL
                        .into_iter()
                        .find(|candidate| candidate.name().as_str() == value)
                    {
                        light = Some(known);
                    }
                }
                Some(Key::Repeats) => {
                    repeats = Count::new(value.trim().parse().unwrap_or_default());
                }
                None => {}
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
        write(
            &folder.join("state"),
            &Message::new(format!(
                "{} {}\n{} {}\n{} {}\n",
                Key::Stamp.name(),
                self.stamp,
                Key::Result.name(),
                self.light.name(),
                Key::Repeats.name(),
                self.repeats,
            )),
        )?;
        write(&folder.join("failures"), &self.failures)
    }
}
