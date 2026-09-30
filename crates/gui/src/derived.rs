use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use domain::{Index, Map};

use crate::sequence::Unit;
use crate::types::{TypeModel, type_model};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Fingerprint(u64);

pub(crate) fn fingerprint(index: &Index) -> Fingerprint {
    let mut hasher = DefaultHasher::new();
    for file in index.files() {
        file.path().as_str().hash(&mut hasher);
        file.hash().value().hash(&mut hasher);
        file.symbols().count().hash(&mut hasher);
        for symbol in file.symbols() {
            symbol.callees().len().hash(&mut hasher);
        }
    }
    Fingerprint(hasher.finish())
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Layers {
    pub(crate) order: Vec<String>,
    pub(crate) level: BTreeMap<String, usize>,
}

impl Layers {
    pub(crate) fn rank(&self, name: &str) -> usize {
        self.order
            .iter()
            .position(|known| known == name)
            .unwrap_or(self.order.len())
    }
}

fn manifest_edges(index: &Index) -> BTreeMap<String, BTreeSet<String>> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in index.files() {
        let path = file.path().as_str();
        if !path.ends_with("Cargo.toml") {
            continue;
        }
        let owner = Unit::Crate.of(path);
        if owner == path {
            continue;
        }
        let deps = edges.entry(owner.clone()).or_default();
        let count = file.text().count().value();
        for number in 0..count {
            let Some(line) = file.text().line(domain::Line::new(number)) else {
                continue;
            };
            let text = line.as_str();
            let Some(at) = text.find("path = \"") else {
                continue;
            };
            let rest = text.get(at + 8..).unwrap_or_default();
            let target = rest.split('"').next().unwrap_or_default();
            let name = target.rsplit('/').next().unwrap_or_default();
            if !name.is_empty() && name != owner {
                deps.insert(name.to_owned());
            }
        }
    }
    edges
}

fn call_edges(index: &Index) -> BTreeMap<String, BTreeSet<String>> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let crate_of = |id: domain::SymbolId| {
        index
            .file(id.file())
            .map(|file| Unit::Crate.of(file.path().as_str()))
    };
    for file in index.files() {
        let owner = Unit::Crate.of(file.path().as_str());
        for symbol in file.symbols() {
            for callee in symbol.callees() {
                if let Some(target) = crate_of(*callee)
                    && target != owner
                {
                    edges.entry(owner.clone()).or_default().insert(target);
                }
            }
        }
    }
    edges
}

pub(crate) fn layers(index: &Index) -> Layers {
    let mut edges = manifest_edges(index);
    if edges.values().all(BTreeSet::is_empty) {
        edges = call_edges(index);
    }
    let mut names: BTreeSet<String> = edges.keys().cloned().collect();
    for deps in edges.values() {
        names.extend(deps.iter().cloned());
    }
    for file in index.files() {
        if file.symbols().next().is_some() {
            names.insert(Unit::Crate.of(file.path().as_str()));
        }
    }
    let mut level: BTreeMap<String, usize> = BTreeMap::new();
    fn depth(
        name: &str,
        edges: &BTreeMap<String, BTreeSet<String>>,
        level: &mut BTreeMap<String, usize>,
        seen: &mut BTreeSet<String>,
    ) -> usize {
        if let Some(known) = level.get(name) {
            return *known;
        }
        if !seen.insert(name.to_owned()) {
            return 0;
        }
        let below = edges
            .get(name)
            .map(|deps| {
                deps.iter()
                    .map(|dep| depth(dep, edges, level, seen) + 1)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        level.insert(name.to_owned(), below);
        below
    }
    for name in &names {
        let mut seen = BTreeSet::new();
        depth(name, &edges, &mut level, &mut seen);
    }
    let mut order: Vec<String> = names.into_iter().collect();
    order.sort_by(|a, b| {
        let la = level.get(a).copied().unwrap_or(0);
        let lb = level.get(b).copied().unwrap_or(0);
        lb.cmp(&la).then(a.cmp(b))
    });
    Layers { order, level }
}

#[derive(Default)]
pub(crate) struct Derived {
    index_key: Option<Fingerprint>,
    layers: Rc<Layers>,
    types: Rc<TypeModel>,
    map_key: Option<Map>,
    pub(crate) atlas: RefCell<Option<(AtlasKey, Rc<crate::atlas::Atlas>)>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AtlasKey {
    pub(crate) index: Fingerprint,
    pub(crate) selected: Option<crate::model::TourSlot>,
    pub(crate) state: crate::atlas::AtlasState,
}

pub(crate) struct Cache(RefCell<Derived>);

impl Default for Cache {
    fn default() -> Self {
        Self(RefCell::new(Derived::default()))
    }
}

impl Cache {
    fn refresh(&self, index: &Index, map: &Map) -> Fingerprint {
        let key = fingerprint(index);
        let mut derived = self.0.borrow_mut();
        if derived.index_key != Some(key) {
            let layers = Rc::new(layers(index));
            derived.types = Rc::new(type_model(index));
            derived.layers = layers;
            derived.index_key = Some(key);
            *derived.atlas.borrow_mut() = None;
        }
        if derived.map_key.as_ref() != Some(map) {
            derived.map_key = Some(map.clone());
            *derived.atlas.borrow_mut() = None;
        }
        key
    }

    pub(crate) fn layers(&self, index: &Index, map: &Map) -> Rc<Layers> {
        self.refresh(index, map);
        Rc::clone(&self.0.borrow().layers)
    }

    pub(crate) fn types(&self, index: &Index, map: &Map) -> Rc<TypeModel> {
        self.refresh(index, map);
        Rc::clone(&self.0.borrow().types)
    }

    pub(crate) fn atlas(
        &self,
        index: &Index,
        map: &Map,
        selected: Option<crate::model::TourSlot>,
        state: crate::atlas::AtlasState,
        build: impl FnOnce(&Layers) -> crate::atlas::Atlas,
    ) -> Rc<crate::atlas::Atlas> {
        let fingerprint = self.refresh(index, map);
        let key = AtlasKey {
            index: fingerprint,
            selected,
            state,
        };
        let derived = self.0.borrow();
        if let Some((held, atlas)) = derived.atlas.borrow().as_ref()
            && *held == key
        {
            return Rc::clone(atlas);
        }
        let atlas = Rc::new(build(&derived.layers));
        *derived.atlas.borrow_mut() = Some((key, Rc::clone(&atlas)));
        atlas
    }
}
