use std::fmt;

#[derive(Debug)]
pub enum ClipboardError {
    Open(arboard::Error),
    Read(arboard::Error),
    Write(arboard::Error),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(error) => write!(formatter, "cannot open the clipboard: {error}"),
            Self::Read(error) => write!(formatter, "cannot read the clipboard: {error}"),
            Self::Write(error) => write!(formatter, "cannot write the clipboard: {error}"),
        }
    }
}

pub struct Clipboard(arboard::Clipboard);

impl Clipboard {
    pub fn open() -> Result<Self, ClipboardError> {
        arboard::Clipboard::new()
            .map(Self)
            .map_err(ClipboardError::Open)
    }

    pub fn read(&mut self) -> Result<String, ClipboardError> {
        self.0.get_text().map_err(ClipboardError::Read)
    }

    pub fn write(&mut self, text: &str) -> Result<(), ClipboardError> {
        self.0.set_text(text).map_err(ClipboardError::Write)
    }
}
