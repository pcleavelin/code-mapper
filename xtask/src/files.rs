use std::fs;
use std::path::Path;

use crate::text::Message;

#[expect(
    clippy::disallowed_methods,
    reason = "xtask writes every file it writes here"
)]
pub(crate) fn write(path: &Path, text: &Message) -> Result<(), Message> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)
            .map_err(|error| Message::new(format!("{}: {error}", folder.display())))?;
    }
    fs::write(path, text.as_str())
        .map_err(|error| Message::new(format!("{}: {error}", path.display())))
}

pub(crate) fn fresh_directory(path: &Path) -> Result<(), Message> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|error| Message::new(format!("{}: {error}", path.display())))?;
    }
    fs::create_dir_all(path).map_err(|error| Message::new(format!("{}: {error}", path.display())))
}

pub(crate) fn copy(from: &Path, to: &Path) -> Result<(), Message> {
    if let Some(folder) = to.parent() {
        fs::create_dir_all(folder)
            .map_err(|error| Message::new(format!("{}: {error}", folder.display())))?;
    }
    fs::copy(from, to)
        .map(drop)
        .map_err(|error| Message::new(format!("{} -> {}: {error}", from.display(), to.display())))
}
