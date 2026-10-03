use features::{Chord, Feature, Gesture, Key as ChordKey, Modifiers, Trigger};
use ui::{Button, Coordinate, Glyph, Input, Interaction, Key, Mods, Pointer, Press, Vector};

use crate::field::{Edit, Held, Lines, Motion, Unit, Which};
use crate::graph::Heading;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Going {
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Walk {
    Down,
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodeGesture {
    Peek,
    Jump,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineGesture {
    Press,
    Drag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Extend {
    Replace,
    Extend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wheeling {
    Zoom,
    Across,
    Along,
}

fn key_of(key: ChordKey) -> Key {
    match key {
        ChordKey::Up => Key::Up,
        ChordKey::Down => Key::Down,
        ChordKey::Left => Key::Left,
        ChordKey::Right => Key::Right,
        ChordKey::Letter(letter) => Key::Character(Glyph::new(letter.as_char())),
    }
}

const fn mods_of(modifiers: Modifiers) -> Mods {
    match modifiers {
        Modifiers::Plain => Mods::NONE,
        Modifiers::Control => Mods::CTRL,
        Modifiers::Alt => Mods::ALT,
    }
}

fn chords(feature: Feature) -> impl Iterator<Item = Chord> {
    feature
        .spec()
        .triggers()
        .iter()
        .filter_map(|trigger| match trigger {
            Trigger::Key(chord) => Some(*chord),
            _ => None,
        })
}

fn gestures(feature: Feature) -> impl Iterator<Item = Gesture> {
    feature
        .spec()
        .triggers()
        .iter()
        .filter_map(|trigger| match trigger {
            Trigger::Gesture(gesture, _) => Some(*gesture),
            _ => None,
        })
}

fn chord_pressed(input: &Input, chord: Chord) -> bool {
    input.pressed_key(key_of(chord.key()), mods_of(chord.modifiers()))
}

pub(crate) fn save(input: &Input) -> bool {
    chords(Feature::Save).any(|chord| chord_pressed(input, chord))
}

pub(crate) fn going(input: &Input, going: Going, focused: Option<Which>) -> bool {
    let (wanted, button, gesture) = match going {
        Going::Back => (ChordKey::Left, Button::Back, Gesture::BackButton),
        Going::Forward => (ChordKey::Right, Button::Forward, Gesture::ForwardButton),
    };
    (focused.is_none()
        && chords(Feature::GoBack)
            .filter(|chord| chord.key() == wanted)
            .any(|chord| chord_pressed(input, chord)))
        || (gestures(Feature::GoBack).any(|named| named == gesture)
            && input.pointer.pressed.contains(button))
}

pub(crate) fn headings(input: &Input) -> Vec<Heading> {
    [
        (Feature::WalkSteps, ChordKey::Up, Heading::Up),
        (Feature::WalkSteps, ChordKey::Down, Heading::Down),
        (Feature::WalkGraph, ChordKey::Left, Heading::Left),
        (Feature::WalkGraph, ChordKey::Right, Heading::Right),
    ]
    .into_iter()
    .filter(|(feature, wanted, _)| {
        chords(*feature)
            .filter(|chord| chord.key() == *wanted)
            .any(|chord| chord_pressed(input, chord))
    })
    .map(|(_, _, heading)| heading)
    .collect()
}

pub(crate) fn walks(input: &Input) -> Vec<Walk> {
    [(ChordKey::Down, Walk::Down), (ChordKey::Up, Walk::Up)]
        .into_iter()
        .filter(|(wanted, _)| {
            chords(Feature::WalkSteps)
                .filter(|chord| chord.key() == *wanted)
                .any(|chord| chord_pressed(input, chord))
        })
        .map(|(_, walk)| walk)
        .collect()
}

pub(crate) const fn held(mods: Mods) -> Held {
    if mods.ctrl() {
        Held::Control
    } else if mods.shift() {
        Held::Shift
    } else {
        Held::Plain
    }
}

fn clipboard_edit(press: Press) -> Option<Edit> {
    chords(Feature::EditText)
        .filter(|chord| {
            key_of(chord.key()) == press.key && mods_of(chord.modifiers()).same_ctrl_alt(press.mods)
        })
        .find_map(|chord| match chord.key() {
            ChordKey::Letter(letter) => match letter.as_char() {
                'c' => Some(Edit::Copy),
                'x' => Some(Edit::Cut),
                'v' => Some(Edit::Paste),
                _ => None,
            },
            ChordKey::Up | ChordKey::Down | ChordKey::Left | ChordKey::Right => None,
        })
}

fn edit_of(press: Press) -> Option<Edit> {
    let mods = press.mods;
    let unit = if mods.ctrl() || mods.alt() {
        Unit::Word
    } else {
        Unit::Character
    };
    let extend = if mods.shift() {
        Extend::Extend
    } else {
        Extend::Replace
    };
    Some(match press.key {
        Key::Enter => Edit::Enter(held(mods)),
        Key::Backspace => Edit::Backspace(unit),
        Key::Remove => Edit::Remove(unit),
        Key::Left => Edit::Move(Motion::Left(unit), extend),
        Key::Right => Edit::Move(Motion::Right(unit), extend),
        Key::Home if mods.ctrl() => Edit::Move(Motion::Start, extend),
        Key::Home => Edit::Move(Motion::RowStart, extend),
        Key::End if mods.ctrl() => Edit::Move(Motion::End, extend),
        Key::End => Edit::Move(Motion::RowEnd, extend),
        Key::Up => Edit::Move(Motion::Up, extend),
        Key::Down => Edit::Move(Motion::Down, extend),
        Key::Escape => Edit::Escape,
        Key::Character(glyph) if glyph.get() == 'a' && mods.ctrl() => Edit::SelectAll,
        Key::Character(glyph) if glyph.get() == 'u' && mods.ctrl() => Edit::ClearLine,
        Key::Character(_) => return clipboard_edit(press),
    })
}

pub(crate) fn edits(input: &Input) -> Vec<Edit> {
    input
        .keys
        .iter()
        .filter_map(|press| edit_of(*press))
        .collect()
}

fn seen(gesture: Gesture, interaction: Interaction, pointer: Pointer) -> bool {
    let mods = pointer.mods;
    match gesture {
        Gesture::AltClick => interaction.clicked() && mods.alt(),
        Gesture::ControlClick => interaction.clicked() && mods.ctrl(),
        Gesture::ShiftClick => interaction.clicked() && mods.shift(),
        Gesture::DoubleClick => interaction.double_clicked(),
        Gesture::Hover => interaction.hovered(),
        Gesture::Drag => interaction.drag().is_some(),
        Gesture::Wheel => interaction.wheel().vertical.get() != 0.0,
        Gesture::ShiftWheel => mods.shift(),
        Gesture::Swipe => interaction.wheel().horizontal.get() != 0.0,
        Gesture::ControlWheel => mods.ctrl(),
        Gesture::Pinch => interaction.pinch().get() != 0.0,
        Gesture::BackButton => pointer.pressed.contains(Button::Back),
        Gesture::ForwardButton => pointer.pressed.contains(Button::Forward),
    }
}

fn fired(feature: Feature, interaction: Interaction, pointer: Pointer) -> bool {
    gestures(feature).any(|gesture| seen(gesture, interaction, pointer))
}

pub(crate) fn shows_references(interaction: Interaction, pointer: Pointer) -> bool {
    fired(Feature::ShowReferences, interaction, pointer)
}

pub(crate) fn code_gesture(interaction: Interaction, pointer: Pointer) -> Option<CodeGesture> {
    if fired(Feature::PeekDefinition, interaction, pointer) {
        Some(CodeGesture::Peek)
    } else if fired(Feature::JumpToDefinition, interaction, pointer) {
        Some(CodeGesture::Jump)
    } else {
        None
    }
}

pub(crate) fn selects_line(interaction: Interaction, pointer: Pointer) -> Option<LineGesture> {
    let mods = pointer.mods;
    if fired(Feature::SelectLines, interaction, pointer) {
        Some(LineGesture::Drag)
    } else if interaction.clicked() && !mods.ctrl() && !mods.alt() && !interaction.double_clicked()
    {
        Some(LineGesture::Press)
    } else {
        None
    }
}

pub(crate) fn wheeling(pointer: Pointer) -> Wheeling {
    let wheel_on = |feature: Feature, wanted: Gesture| {
        gestures(feature).any(|gesture| gesture == wanted)
            && seen(wanted, Interaction::default(), pointer)
    };
    if wheel_on(Feature::ZoomGraph, Gesture::ControlWheel) {
        Wheeling::Zoom
    } else if wheel_on(Feature::ScrollCode, Gesture::ShiftWheel) {
        Wheeling::Across
    } else {
        Wheeling::Along
    }
}

pub(crate) fn turn_wheel(pointer: &mut Pointer) {
    if wheeling(*pointer) == Wheeling::Across {
        let wheel = pointer.wheel;
        pointer.wheel = Vector::new(
            Coordinate::new(wheel.horizontal.get() + wheel.vertical.get()),
            Coordinate::ZERO,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaletteKey {
    Move(Walk),
    Run,
    Close,
}

pub(crate) fn palette_toggled(input: &Input) -> bool {
    chords(Feature::CommandPalette).any(|chord| chord_pressed(input, chord))
}

pub(crate) fn settings_toggled(input: &Input) -> bool {
    chords(Feature::Settings).any(|chord| chord_pressed(input, chord))
}

pub(crate) fn escaped(input: &Input) -> bool {
    input
        .keys
        .iter()
        .any(|press| press.key == Key::Escape && !press.mods.ctrl() && !press.mods.alt())
}

pub(crate) fn palette_keys(input: &Input) -> Vec<PaletteKey> {
    input
        .keys
        .iter()
        .filter(|press| !press.mods.ctrl() && !press.mods.alt())
        .filter_map(|press| match press.key {
            Key::Up => Some(PaletteKey::Move(Walk::Up)),
            Key::Down => Some(PaletteKey::Move(Walk::Down)),
            Key::Enter => Some(PaletteKey::Run),
            Key::Escape => Some(PaletteKey::Close),
            _ => None,
        })
        .collect()
}

pub(crate) fn palette_edits(input: &Input) -> Vec<Edit> {
    edits(input)
        .into_iter()
        .filter(|edit| {
            !matches!(
                edit,
                Edit::Enter(_) | Edit::Escape | Edit::Move(Motion::Up | Motion::Down, _)
            )
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WizardKey {
    Next,
    Apply,
    Escape,
}

pub(crate) fn wizard_keys(input: &Input, lines: Lines) -> Vec<WizardKey> {
    input
        .keys
        .iter()
        .filter(|press| !press.mods.alt())
        .filter_map(|press| match press.key {
            Key::Enter if press.mods.ctrl() => Some(WizardKey::Apply),
            Key::Enter if lines.breaks(held(press.mods)) => None,
            _ if press.mods.ctrl() => None,
            Key::Enter => Some(WizardKey::Next),
            Key::Escape => Some(WizardKey::Escape),
            _ => None,
        })
        .collect()
}
