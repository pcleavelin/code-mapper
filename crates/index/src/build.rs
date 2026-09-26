use std::collections::BTreeSet;
use std::io;

use domain::{Backend, Index, Readiness, RelativePath, Root};
use io_cache::CacheStore;
use io_source::{Stamp, Stamps};

use crate::link::link;
use crate::parse::Parsers;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indexed {
    pub index: Index,
    pub stamps: Stamps,
}

pub fn build(root: &Root) -> Indexed {
    let mut parsers = Parsers::default();
    let mut cache = CacheStore::load(root).unwrap_or_default();
    let mut changed: BTreeSet<RelativePath> = BTreeSet::new();
    let mut files = Vec::new();
    let mut stamps = Vec::new();
    for read in io_source::walk(root) {
        let hash = read.contents.hash();
        let mut file = match cache.take(&read.path) {
            Some(mut cached) if cached.hash() == hash => {
                cached.set_text(read.contents.text());
                cached
            }
            _ => {
                changed.insert(read.path.clone());
                parsers.parse(read.path.clone(), &read.contents)
            }
        };
        let readiness = if file.language().is_some() && file.backend() != Backend::Server {
            Readiness::Pending
        } else {
            Readiness::Ready
        };
        file.set_readiness(readiness);
        stamps.push(Stamp {
            path: read.path,
            modified: read.modified,
        });
        files.push(file);
    }
    for file in &mut files {
        let stale = file.backend() == Backend::Server
            && file.symbols().any(|symbol| {
                symbol
                    .targets()
                    .iter()
                    .chain(symbol.references())
                    .any(|location| changed.contains(&location.file))
            });
        if stale {
            file.set_readiness(Readiness::Pending);
        }
    }
    files.sort_by(|one, other| one.path().cmp(other.path()));
    let mut index = Index::new(root.clone());
    for file in files {
        index.push(file);
    }
    link(&mut index);
    if !changed.is_empty() || !cache.is_empty() {
        drop(save_cache(&index));
    }
    Indexed {
        index,
        stamps: Stamps::new(stamps),
    }
}

pub fn save_cache(index: &Index) -> io::Result<()> {
    CacheStore::save(index)
}
