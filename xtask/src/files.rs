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
