use std::fs;
use std::path::Path;
use std::time::SystemTime;

use domain::{FileText, RelativePath, Root, TextHash};
use ignore::WalkBuilder;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ByteCount(usize);

impl ByteCount {
    const LARGEST_FILE: Self = Self(4 << 20);
    const BINARY_HEAD: Self = Self(1024);

    fn binary(raw: &[u8]) -> bool {
        raw.len() > Self::LARGEST_FILE.0
            || raw.iter().take(Self::BINARY_HEAD.0).any(|byte| *byte == 0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contents(String);

impl Contents {
    pub fn new(raw: &str) -> Self {
        Self(raw.replace('\t', "    "))
    }

    fn of_bytes(raw: &[u8]) -> Self {
        Self::new(&String::from_utf8_lossy(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn text(&self) -> FileText {
        FileText::from(self.0.as_str())
    }

    pub fn hash(&self) -> TextHash {
        TextHash::of(self.0.as_bytes())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Modified(SystemTime);

impl Modified {
    pub fn of(path: &Path) -> Option<Self> {
        fs::metadata(path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(Self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub path: RelativePath,
    pub modified: Option<Modified>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stamps(Vec<Stamp>);

impl Stamps {
    pub fn new(stamps: Vec<Stamp>) -> Self {
        Self(stamps)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Stamp> {
        self.0.iter()
    }

    pub fn changed(&self, root: &Root) -> bool {
        self.0
            .iter()
            .any(|stamp| Modified::of(&root.join(&stamp.path)) != stamp.modified)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRead {
    pub path: RelativePath,
    pub contents: Contents,
    pub modified: Option<Modified>,
}

pub fn walk(root: &Root) -> Vec<SourceRead> {
    WalkBuilder::new(root.as_path())
        .require_git(false)
        .build()
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let raw = fs::read(entry.path()).ok()?;
            if ByteCount::binary(&raw) {
                return None;
            }
            let relative = entry
                .path()
                .strip_prefix(root.as_path())
                .unwrap_or(entry.path())
                .to_string_lossy()
                .into_owned();
            Some(SourceRead {
                path: RelativePath::new(&relative),
                contents: Contents::of_bytes(&raw),
                modified: entry
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .map(Modified),
            })
        })
        .collect()
}

pub fn read_outside(path: &Path) -> Contents {
    Contents::new(&fs::read_to_string(path).unwrap_or_default())
}

#[cfg(test)]
mod tests;
