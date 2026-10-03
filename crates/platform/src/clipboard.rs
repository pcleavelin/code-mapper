use io_clipboard::Clipboard as System;

use ui::Label;

use crate::report::report;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Clip {
    #[default]
    Keep,
    Copy(Label),
    Paste,
}

pub(crate) enum Clipboard {
    System(Option<System>),
    Kept(Label),
}

impl Clipboard {
    pub(crate) const fn system() -> Self {
        Self::System(None)
    }

    pub(crate) fn kept() -> Self {
        Self::Kept(Label::default())
    }

    fn opened(slot: &mut Option<System>) -> Option<&mut System> {
        if slot.is_none() {
            match System::open() {
                Ok(opened) => *slot = Some(opened),
                Err(error) => report(format_args!("{error}")),
            }
        }
        slot.as_mut()
    }

    pub(crate) fn copy(&mut self, text: Label) {
        match self {
            Self::System(slot) => {
                if let Some(system) = Self::opened(slot)
                    && let Err(error) = system.write(text.as_str())
                {
                    report(format_args!("{error}"));
                }
            }
            Self::Kept(kept) => *kept = text,
        }
    }

    pub(crate) fn paste(&mut self) -> Option<Label> {
        match self {
            Self::System(slot) => match Self::opened(slot)?.read() {
                Ok(text) => Some(Label::new(text)),
                Err(error) => {
                    report(format_args!("{error}"));
                    None
                }
            },
            Self::Kept(kept) => Some(kept.clone()),
        }
    }
}
