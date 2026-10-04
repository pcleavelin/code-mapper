use crate::app::App;
use crate::welcome::Spell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Menu {
    Tour,
}

impl Menu {
    pub(crate) const fn word(self) -> Spell {
        Spell::new(match self {
            Self::Tour => "tour",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuAct {
    Toggle(Menu),
    Close,
}

impl App {
    pub(crate) fn menu(&mut self, act: MenuAct) {
        self.model.toolbar_menu = match act {
            MenuAct::Toggle(menu) if self.model.toolbar_menu != Some(menu) => Some(menu),
            MenuAct::Toggle(_) | MenuAct::Close => None,
        };
    }
}
