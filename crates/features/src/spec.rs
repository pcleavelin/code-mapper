#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Text(&'static str);

impl Text {
    pub const fn new(text: &'static str) -> Self {
        Self(text)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Element(&'static str);

impl Element {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Letter(char);

impl Letter {
    pub const fn new(letter: char) -> Self {
        Self(letter)
    }

    pub const fn as_char(self) -> char {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Letter(Letter),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Modifiers {
    Plain,
    Control,
    Alt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Chord {
    key: Key,
    modifiers: Modifiers,
}

impl Chord {
    pub const fn plain(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::Plain,
        }
    }

    pub const fn control(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::Control,
        }
    }

    pub const fn alt(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::Alt,
        }
    }

    pub const fn key(self) -> Key {
        self.key
    }

    pub const fn modifiers(self) -> Modifiers {
        self.modifiers
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gesture {
    ShiftClick,
    ControlClick,
    AltClick,
    DoubleClick,
    Hover,
    Drag,
    Wheel,
    ShiftWheel,
    ControlWheel,
    Pinch,
    BackButton,
    ForwardButton,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Trigger {
    Command(Text),
    Key(Chord),
    Click(Element),
    Type(Element),
    Gesture(Gesture, Element),
    Palette(Text, Option<Chord>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Surface {
    Command,
    Window,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spec {
    name: Text,
    summary: Text,
    surface: Surface,
    triggers: &'static [Trigger],
}

impl Spec {
    pub const fn new(
        name: Text,
        summary: Text,
        surface: Surface,
        triggers: &'static [Trigger],
    ) -> Self {
        Self {
            name,
            summary,
            surface,
            triggers,
        }
    }

    pub const fn name(self) -> Text {
        self.name
    }

    pub const fn summary(self) -> Text {
        self.summary
    }

    pub const fn surface(self) -> Surface {
        self.surface
    }

    pub const fn triggers(self) -> &'static [Trigger] {
        self.triggers
    }
}
