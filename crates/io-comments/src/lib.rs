mod convert;
mod error;
mod wire;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use domain::{Comment, CommentId, Comments, RelativePath, Root};
use io_vcs::Vcs;

use crate::convert::{comment_from_text, comment_to_wire};

pub use crate::error::{
    CommentFault, CommentLoadError, CommentParseError, CommentSaveError, FieldKey, FieldValue,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommentVersion(&'static str);

impl CommentVersion {
    pub const CURRENT: Self = Self(wire::VERSION);

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Saved;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentFileText(String);

impl CommentFileText {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommentCount(u32);

impl CommentCount {
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommentStamp {
    modified: SystemTime,
    files: CommentCount,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentStore {
    directory: PathBuf,
}

impl CommentStore {
    pub fn new(root: &Root) -> Self {
        let shared = Vcs::detect(root)
            .ok()
            .and_then(|vcs| vcs.shared_directory().ok());
        let directory = match shared {
            Some(shared) => shared.join(wire::SHARED_DIRECTORY),
            None => root.join(&RelativePath::new(wire::LOCAL_DIRECTORY)),
        };
        Self { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn load(&self) -> Result<Comments, CommentLoadError> {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return Ok(Comments::default());
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|file| is_comment_file(file))
            .collect();
        files.sort();
        let mut comments = Vec::new();
        for file in files {
            let id = file
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(CommentId::new)
                .ok_or_else(|| CommentLoadError::InvalidId(file.clone()))?;
            let text = fs::read_to_string(&file)
                .map(CommentFileText)
                .map_err(|error| CommentLoadError::Unreadable {
                    file: file.clone(),
                    error,
                })?;
            comments.push(comment_from_text(id, &file, &text).map_err(CommentLoadError::Parse)?);
        }
        Ok(Comments::new(comments))
    }

    pub fn write(&self, comment: &Comment) -> Result<Saved, CommentSaveError> {
        fs::create_dir_all(&self.directory).map_err(|error| CommentSaveError::CreateDirectory {
            directory: self.directory.clone(),
            error,
        })?;
        let file = self.file_of(comment.id());
        io_store::write(&file, wire::render(&comment_to_wire(comment)))
            .map_err(|error| CommentSaveError::Write { file, error })?;
        Ok(Saved)
    }

    pub fn remove(&self, id: &CommentId) -> Result<Saved, CommentSaveError> {
        let file = self.file_of(id);
        match io_store::remove(&file) {
            Ok(()) => Ok(Saved),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Saved),
            Err(error) => Err(CommentSaveError::Remove { file, error }),
        }
    }

    pub fn stamp(&self) -> Option<CommentStamp> {
        let mut modified = fs::metadata(&self.directory).ok()?.modified().ok()?;
        let mut files: u32 = 0;
        for entry in fs::read_dir(&self.directory).ok()?.flatten() {
            if is_comment_file(&entry.path()) {
                files = files.saturating_add(1);
                if let Ok(time) = entry.metadata().and_then(|metadata| metadata.modified()) {
                    modified = modified.max(time);
                }
            }
        }
        Some(CommentStamp {
            modified,
            files: CommentCount(files),
        })
    }

    fn file_of(&self, id: &CommentId) -> PathBuf {
        self.directory
            .join(format!("{}.{}", id.as_str(), wire::COMMENT_EXTENSION))
    }
}

fn is_comment_file(file: &Path) -> bool {
    file.extension()
        .is_some_and(|extension| extension == wire::COMMENT_EXTENSION)
}

#[cfg(test)]
mod tests;
