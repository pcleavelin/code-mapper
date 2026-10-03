mod convert;
mod wire;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use domain::{LayoutTree, Settings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    User,
    OverrideOnly,
}

#[derive(Clone, Copy, Debug)]
struct FileName(&'static str);

#[derive(Clone, Copy, Debug)]
struct Variable(&'static str);

#[derive(Clone, Copy, Debug)]
struct UserFile {
    name: FileName,
    override_variable: Variable,
}

const LAYOUT_FILE: UserFile = UserFile {
    name: FileName("layout"),
    override_variable: Variable("CODEMAP_LAYOUT"),
};

const SETTINGS_FILE: UserFile = UserFile {
    name: FileName("settings"),
    override_variable: Variable("CODEMAP_SETTINGS"),
};

fn user_file(file: UserFile, reach: Reach) -> Option<PathBuf> {
    if let Some(path) = env::var_os(file.override_variable.0) {
        return Some(PathBuf::from(path));
    }
    if reach == Reach::OverrideOnly {
        return None;
    }
    let folder = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .or_else(|| env::var_os("APPDATA").map(PathBuf::from))?;
    Some(folder.join("codemap").join(OsStr::new(file.name.0)))
}

fn save_text<Contents: AsRef<[u8]>>(path: &Path, contents: Contents) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    io_store::write(path, contents)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutStore {
    path: PathBuf,
}

impl LayoutStore {
    pub const fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn find(reach: Reach) -> Option<Self> {
        user_file(LAYOUT_FILE, reach).map(Self::at)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Option<LayoutTree> {
        let text = fs::read_to_string(&self.path).ok()?;
        convert::layout(&wire::parse(&text)?)
    }

    pub fn save(&self, layout: &LayoutTree) -> io::Result<()> {
        save_text(&self.path, wire::print(&convert::wire(layout)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub const fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn find(reach: Reach) -> Option<Self> {
        user_file(SETTINGS_FILE, reach).map(Self::at)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Option<Settings> {
        let text = fs::read_to_string(&self.path).ok()?;
        wire::parse_settings(&text).map(|wire| convert::settings(&wire))
    }

    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        save_text(
            &self.path,
            wire::print_settings(&convert::wire_settings(settings)),
        )
    }
}

#[cfg(test)]
mod tests;
