mod convert;
mod error;
mod wire;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use domain::{Map, RelativePath, Revision, Root, Tour, TourCount, TourName};

use crate::convert::{map_from_tours, tour_from_wire, tour_to_wire};
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

    fn of(tour: &Tour) -> Self {
        Self(wire::render(&tour_to_wire(tour)))
    }

    fn tours(&self, origin: &Origin) -> Result<Vec<Tour>, ParseError> {
        let wire_tours =
            wire::parse(&self.0).map_err(|fault| Located::from(fault).within(origin))?;
        wire_tours
            .iter()
            .map(|tour| tour_from_wire(tour).map_err(|fault| fault.within(origin)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Saved;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    modified: SystemTime,
    files: TourCount,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapStore {
    directory: PathBuf,
    disk: BTreeMap<TourName, MapText>,
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
        let mut tours = Vec::new();
        for file in files {
            let text = fs::read_to_string(&file).map(MapText).map_err(|error| {
                MapLoadError::Unreadable {
                    file: file.clone(),
                    error,
                }
            })?;
            let mut found = text
                .tours(&Origin::File(file.clone()))
                .map_err(MapLoadError::Parse)?;
            let (Some(tour), true) = (found.pop(), found.is_empty()) else {
                return Err(MapLoadError::OneTourPerFile(file));
            };
            if file
                .file_stem()
                .is_none_or(|stem| stem != tour.name().as_str())
            {
                return Err(MapLoadError::Misplaced {
                    file,
                    tour: tour.name().clone(),
                });
            }
            disk.insert(tour.name().clone(), text);
            tours.push(tour);
        }
        let map = map_from_tours(tours).map_err(|fault| {
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
        for tour in map.tours() {
            let text = MapText::of(tour);
            if self.disk.get(tour.name()) != Some(&text) {
                let file = self.file_of(tour.name());
                io_store::write(&file, text.as_str())
                    .map_err(|error| MapSaveError::Write { file, error })?;
            }
            now.insert(tour.name().clone(), text);
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
        let mut tours = text.tours(&origin)?;
        tours.sort_by(|one, other| one.name().cmp(other.name()));
        map_from_tours(tours).map_err(|fault| fault.within(&origin))
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
            files: TourCount::new(files),
        })
    }

    fn file_of(&self, name: &TourName) -> PathBuf {
        self.directory
            .join(format!("{}.{}", name.as_str(), wire::MAP_EXTENSION))
    }
}

#[cfg(test)]
mod tests;
