use io_clipboard::Clipboard as OsClipboard;

use ui::Label;

use crate::report::report;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ClipboardRequest {
    #[default]
    Keep,
    Copy(Label),
    Paste,
}

pub(crate) enum Clipboard {
    Os(Option<OsClipboard>),
    Local(Label),
}

impl Clipboard {
    pub(crate) const fn os() -> Self {
        Self::Os(None)
    }

    pub(crate) fn local() -> Self {
        Self::Local(Label::default())
    }

    fn opened(slot: &mut Option<OsClipboard>) -> Option<&mut OsClipboard> {
        if slot.is_none() {
            match OsClipboard::open() {
                Ok(opened) => *slot = Some(opened),
                Err(error) => report(format_args!("{error}")),
            }
        }
        slot.as_mut()
    }

    pub(crate) fn copy(&mut self, text: Label) {
        match self {
            Self::Os(slot) => {
                if let Some(os) = Self::opened(slot)
                    && let Err(error) = os.write(text.as_str())
                {
                    report(format_args!("{error}"));
                }
            }
            Self::Local(local) => *local = text,
        }
    }

    pub(crate) fn paste(&mut self) -> Option<Label> {
        match self {
            Self::Os(slot) => match Self::opened(slot)?.read() {
                Ok(text) => Some(Label::new(text)),
                Err(error) => {
                    report(format_args!("{error}"));
                    None
                }
            },
            Self::Local(local) => Some(local.clone()),
        }
    }
}
