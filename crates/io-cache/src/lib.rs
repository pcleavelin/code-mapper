mod convert;
mod wire;

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use domain::{Index, RelativePath, Root, SourceFile};

use crate::wire::{Reader, Writer};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cache(BTreeMap<RelativePath, SourceFile>);

impl Cache {
    pub fn take(&mut self, path: &RelativePath) -> Option<SourceFile> {
        self.0.remove(path)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.0.values()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CacheStore;

impl CacheStore {
    pub fn location(root: &Root) -> PathBuf {
        root.as_path().join(".codemap-cache")
    }

    pub fn load(root: &Root) -> Option<Cache> {
        let bytes = fs::read(Self::location(root)).ok()?;
        let cache = Reader::new(&bytes).cache()?;
        let mut files = BTreeMap::new();
        for cached in &cache.files {
            let file = convert::source_file(cached)?;
            files.insert(file.path().clone(), file);
        }
        Some(Cache(files))
    }

    pub fn save(index: &Index) -> io::Result<()> {
        let mut writer = Writer::default();
        writer.cache(&convert::cache_file(index));
        io_store::write(&Self::location(index.root()), writer.bytes())
    }
}

#[cfg(test)]
mod tests;
