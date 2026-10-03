mod convert;
mod wire;

use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use domain::FontFamily;
use strum::VariantArray;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FaceIndex(u32);

impl FaceIndex {
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Style {
    Regular,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Spelling(&'static str);

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
enum FontExtension {
    TrueType,
    OpenType,
    Collection,
    OpenCollection,
}

impl FontExtension {
    const fn name(self) -> Spelling {
        Spelling(match self {
            Self::TrueType => "ttf",
            Self::OpenType => "otf",
            Self::Collection => "ttc",
            Self::OpenCollection => "otc",
        })
    }

    fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_lowercase();
        Self::VARIANTS
            .iter()
            .copied()
            .find(|known| known.name().0 == extension)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Depth(u8);

const DEEPEST: Depth = Depth(6);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Installed {
    family: FontFamily,
    style: Style,
    path: PathBuf,
    index: FaceIndex,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fonts {
    installed: Vec<Installed>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontBytes(Vec<u8>);

impl FontBytes {
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFile {
    bytes: FontBytes,
    index: FaceIndex,
}

impl FontFile {
    pub const fn bytes(&self) -> &FontBytes {
        &self.bytes
    }

    pub const fn index(&self) -> FaceIndex {
        self.index
    }
}

fn folders() -> Vec<PathBuf> {
    let home = env::var_os("HOME").map(PathBuf::from);
    let mut folders = vec![
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/Library/Fonts"),
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Some(home) = home {
        folders.push(home.join("Library").join("Fonts"));
        folders.push(home.join(".local").join("share").join("fonts"));
        folders.push(home.join(".fonts"));
    }
    if let Some(data) = env::var_os("XDG_DATA_HOME") {
        folders.push(PathBuf::from(data).join("fonts"));
    }
    if let Some(windows) = env::var_os("WINDIR") {
        folders.push(PathBuf::from(windows).join("Fonts"));
    }
    if let Some(local) = env::var_os("LOCALAPPDATA") {
        folders.push(
            PathBuf::from(local)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"),
        );
    }
    folders
}

fn walk(folder: &Path, depth: Depth, out: &mut Vec<PathBuf>) {
    if depth > DEEPEST {
        return;
    }
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<PathBuf>>();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(&path, Depth(depth.0 + 1), out);
        } else if FontExtension::of(&path).is_some() {
            out.push(path);
        }
    }
}

impl Fonts {
    pub fn scan() -> Self {
        let mut files = Vec::new();
        for folder in folders() {
            walk(&folder, Depth(0), &mut files);
        }
        Self::of_files(&files)
    }

    fn of_files(files: &[PathBuf]) -> Self {
        let mut installed = Vec::new();
        for path in files {
            let Ok(mut file) = File::open(path) else {
                continue;
            };
            for face in wire::faces(&mut file) {
                if let Some(family) = convert::family(&face) {
                    installed.push(Installed {
                        family,
                        style: convert::style(&face),
                        path: path.clone(),
                        index: convert::index(&face),
                    });
                }
            }
        }
        installed
            .sort_by(|left, right| (&left.family, left.style).cmp(&(&right.family, right.style)));
        installed.dedup_by(|later, earlier| later.family == earlier.family);
        Self { installed }
    }

    pub fn families(&self) -> impl Iterator<Item = &FontFamily> {
        self.installed.iter().map(|installed| &installed.family)
    }

    pub fn read(&self, family: &FontFamily) -> Option<FontFile> {
        let installed = self
            .installed
            .iter()
            .find(|installed| installed.family == *family)?;
        Some(FontFile {
            bytes: FontBytes(fs::read(&installed.path).ok()?),
            index: installed.index,
        })
    }
}

#[cfg(test)]
mod tests;
