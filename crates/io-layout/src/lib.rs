mod convert;
mod wire;

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use domain::LayoutTree;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutStore {
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    User,
    OverrideOnly,
}

impl LayoutStore {
    pub const fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn find(reach: Reach) -> Option<Self> {
        if let Some(path) = env::var_os("CODEMAP_LAYOUT") {
            return Some(Self::at(PathBuf::from(path)));
        }
        if reach == Reach::OverrideOnly {
            return None;
        }
        let folder = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .or_else(|| env::var_os("APPDATA").map(PathBuf::from))?;
        Some(Self::at(folder.join("codemap").join("layout")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Option<LayoutTree> {
        let text = fs::read_to_string(&self.path).ok()?;
        convert::layout(&wire::parse(&text)?)
    }

    pub fn save(&self, layout: &LayoutTree) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        io_store::write(&self.path, wire::print(&convert::wire(layout)))
    }
}

#[cfg(test)]
mod tests;
