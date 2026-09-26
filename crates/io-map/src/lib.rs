mod convert;
mod error;
mod wire;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use domain::{Map, PathCount, PathName, RelativePath, Revision, Root};

use crate::convert::{map_from_paths, path_from_wire, path_to_wire};
use crate::error::Located;

pub use crate::error::{
    Fault, FieldKey, FieldValue, MapLoadError, MapSaveError, Origin, ParseError,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapVersion(&'static str);

impl MapVersion {
    pub const CURRENT: Self = Self(wire::VERSION);

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapText(String);

impl MapText {
    pub fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn of(path: &domain::Path) -> Self {
        Self(wire::render(&path_to_wire(path)))
    }

    fn paths(&self, origin: &Origin) -> Result<Vec<domain::Path>, ParseError> {
        let wire_paths =
            wire::parse(&self.0).map_err(|fault| Located::from(fault).within(origin))?;
        wire_paths
            .iter()
            .map(|path| path_from_wire(path).map_err(|fault| fault.within(origin)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Saved;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    modified: SystemTime,
    files: PathCount,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapStore {
    directory: PathBuf,
    disk: BTreeMap<PathName, MapText>,
}

impl MapStore {
    pub fn new(root: &Root) -> Self {
        Self {
            directory: root.join(&Self::relative_directory()),
            disk: BTreeMap::new(),
        }
    }

    pub fn relative_directory() -> RelativePath {
        RelativePath::new(wire::MAP_DIRECTORY)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn load(&mut self) -> Result<Map, MapLoadError> {
        if self.directory.is_file() {
            return Err(MapLoadError::OldFormat(self.directory.clone()));
        }
        let Ok(entries) = fs::read_dir(&self.directory) else {
            self.disk.clear();
            return Ok(Map::default());
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|file| {
                file.extension()
                    .is_some_and(|extension| extension == wire::MAP_EXTENSION)
            })
            .collect();
        files.sort();
        let mut disk = BTreeMap::new();
        let mut paths = Vec::new();
        for file in files {
            let text = fs::read_to_string(&file).map(MapText).map_err(|error| {
                MapLoadError::Unreadable {
                    file: file.clone(),
                    error,
                }
            })?;
            let mut found = text
                .paths(&Origin::File(file.clone()))
                .map_err(MapLoadError::Parse)?;
            let (Some(path), true) = (found.pop(), found.is_empty()) else {
                return Err(MapLoadError::OnePathPerFile(file));
            };
            if file
                .file_stem()
                .is_none_or(|stem| stem != path.name().as_str())
            {
                return Err(MapLoadError::Misplaced {
                    file,
                    path: path.name().clone(),
                });
            }
            disk.insert(path.name().clone(), text);
            paths.push(path);
        }
        let map = map_from_paths(paths).map_err(|fault| {
            MapLoadError::Parse(fault.within(&Origin::File(self.directory.clone())))
        })?;
        self.disk = disk;
        Ok(map)
    }

    pub fn save(&mut self, map: &Map) -> Result<Saved, MapSaveError> {
        fs::create_dir_all(&self.directory).map_err(|error| MapSaveError::CreateDirectory {
            directory: self.directory.clone(),
            error,
        })?;
        let mut now = BTreeMap::new();
        for path in map.paths() {
            let text = MapText::of(path);
            if self.disk.get(path.name()) != Some(&text) {
                let file = self.file_of(path.name());
                io_store::write(&file, text.as_str())
                    .map_err(|error| MapSaveError::Write { file, error })?;
            }
            now.insert(path.name().clone(), text);
        }
        for gone in self.disk.keys().filter(|name| !now.contains_key(*name)) {
            let file = self.file_of(gone);
            if file.exists() {
                io_store::remove(&file).map_err(|error| MapSaveError::Remove { file, error })?;
            }
        }
        self.disk = now;
        Ok(Saved)
    }

    pub fn base(text: &MapText, revision: &Revision) -> Result<Map, ParseError> {
        let origin = Origin::Revision(revision.clone());
        let mut paths = text.paths(&origin)?;
        paths.sort_by(|one, other| one.name().cmp(other.name()));
        map_from_paths(paths).map_err(|fault| fault.within(&origin))
    }

    pub fn stamp(&self) -> Option<Stamp> {
        let mut modified = fs::metadata(&self.directory).ok()?.modified().ok()?;
        let mut files: u32 = 0;
        for entry in fs::read_dir(&self.directory).ok()?.flatten() {
            let map_file = entry
                .path()
                .extension()
                .is_some_and(|extension| extension == wire::MAP_EXTENSION);
            if map_file {
                files = files.saturating_add(1);
                if let Ok(time) = entry.metadata().and_then(|metadata| metadata.modified()) {
                    modified = modified.max(time);
                }
            }
        }
        Some(Stamp {
            modified,
            files: PathCount::new(files),
        })
    }

    fn file_of(&self, name: &PathName) -> PathBuf {
        self.directory
            .join(format!("{}.{}", name.as_str(), wire::MAP_EXTENSION))
    }
}

#[cfg(test)]
mod tests;
