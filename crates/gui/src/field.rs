use std::collections::BTreeMap;
use std::mem;

use ui::{Count, Id, Label, Typed};

use crate::ids::{self, Control};
use crate::text::Clipped;
use crate::wizard::BranchId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    Enter,
    Backspace,
    Remove,
    Left,
    Right,
    Home,
    End,
    SelectAll,
    Older,
    Newer,
    Escape,
    ClearLine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Which {
    Search,
    Command,
    SymbolFilter,
    TourFilter,
    ViewSearch,
    GoToLine,
    Palette,
    WizardName,
    WizardGroup,
    WizardSearch,
    WizardNote,
    StepNote(BranchId),
}

impl Which {
    pub(crate) fn id(self) -> Id {
        self.control().id()
    }

    pub(crate) const fn control(self) -> Control {
        match self {
            Self::Search => ids::SEARCH_FIELD,
            Self::Command => ids::COMMAND_FIELD,
            Self::SymbolFilter => ids::FILTER_FIELD,
            Self::TourFilter => ids::TOUR_FILTER_FIELD,
            Self::ViewSearch => ids::VIEW_FIELD,
            Self::GoToLine => ids::LINE_FIELD,
            Self::Palette => ids::PALETTE_FIELD,
            Self::WizardName => ids::WIZARD_NAME_FIELD,
            Self::WizardGroup => ids::WIZARD_GROUP_FIELD,
            Self::WizardSearch => ids::WIZARD_SEARCH_FIELD,
            Self::WizardNote => ids::WIZARD_NOTE_FIELD,
            Self::StepNote(_) => ids::EDIT_NOTE_FIELD,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Enter {
    Keep,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marked {
    Whole,
    Nothing,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FieldText(String);

impl FieldText {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn characters(&self) -> usize {
        self.0.chars().count()
    }

    fn byte_at(&self, character: usize) -> usize {
        self.0
            .char_indices()
            .nth(character)
            .map_or(self.0.len(), |pair| pair.0)
    }

    fn remove_at(&mut self, character: usize) {
        let at = self.byte_at(character);
        if at < self.0.len() {
            self.0.remove(at);
        }
    }

    fn insert_at(&mut self, character: usize, text: &str) {
        let at = self.byte_at(character);
        self.0.insert_str(at, text);
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    fn before(&self, character: usize) -> Label {
        Label::new(self.0.chars().take(character).collect::<String>())
    }

    fn after(&self, character: usize) -> Label {
        Label::new(self.0.chars().skip(character).collect::<String>())
    }

    pub(crate) fn label(&self) -> Label {
        Label::new(self.0.clone())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Caret(usize);

impl Caret {
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Recalled(usize);

#[derive(Clone, Debug)]
pub(crate) struct Field {
    text: FieldText,
    caret: Caret,
    marked: Marked,
    history: Vec<FieldText>,
    recalled: Option<Recalled>,
}

impl Default for Field {
    fn default() -> Self {
        Self {
            text: FieldText::default(),
            caret: Caret::default(),
            marked: Marked::Nothing,
            history: Vec::new(),
            recalled: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Looks {
    Hint,
    Marked,
    Plain,
}

pub(crate) struct Shown {
    pub(crate) looks: Looks,
    pub(crate) before: Label,
    pub(crate) after: Label,
}

impl Field {
    pub(crate) fn text(&self) -> &FieldText {
        &self.text
    }

    pub(crate) const fn caret(&self) -> Caret {
        self.caret
    }

    fn clear(&mut self) {
        self.text.clear();
        self.caret = Caret(0);
    }

    fn apply(&mut self, edit: Edit, enter: Enter, focus: &mut Focus) -> Option<FieldText> {
        let mut submitted = None;
        match edit {
            Edit::Enter => {
                let line = match enter {
                    Enter::Keep => {
                        self.marked = if self.text.is_empty() {
                            Marked::Nothing
                        } else {
                            Marked::Whole
                        };
                        self.text.clone()
                    }
                    Enter::Clear => {
                        self.caret = Caret(0);
                        mem::take(&mut self.text)
                    }
                };
                self.recalled = None;
                if !line.as_str().trim().is_empty() {
                    self.history.push(line.clone());
                    submitted = Some(line);
                }
            }
            Edit::Backspace | Edit::Remove if self.marked == Marked::Whole => {
                self.clear();
                self.marked = Marked::Nothing;
            }
            Edit::Backspace => {
                if self.caret.0 > 0 {
                    self.text.remove_at(self.caret.0 - 1);
                    self.caret = Caret(self.caret.0 - 1);
                }
            }
            Edit::Remove => {
                if self.caret.0 < self.text.characters() {
                    self.text.remove_at(self.caret.0);
                }
            }
            Edit::Left => {
                self.caret = Caret(self.caret.0.saturating_sub(1));
                self.marked = Marked::Nothing;
            }
            Edit::Right => {
                self.caret = Caret((self.caret.0 + 1).min(self.text.characters()));
                self.marked = Marked::Nothing;
            }
            Edit::Home => {
                self.caret = Caret(0);
                self.marked = Marked::Nothing;
            }
            Edit::End => {
                self.caret = Caret(self.text.characters());
                self.marked = Marked::Nothing;
            }
            Edit::SelectAll => {
                self.marked = if self.text.is_empty() {
                    Marked::Nothing
                } else {
                    Marked::Whole
                };
            }
            Edit::Older | Edit::Newer => self.recall(edit),
            Edit::Escape => *focus = Focus::Lost,
            Edit::ClearLine => self.clear(),
        }
        submitted
    }

    fn recall(&mut self, edit: Edit) {
        if self.history.is_empty() {
            return;
        }
        let count = self.history.len();
        let at = match (self.recalled, edit) {
            (None, Edit::Older) => count - 1,
            (Some(Recalled(at)), Edit::Older) => at.saturating_sub(1),
            (Some(Recalled(at)), Edit::Newer) if at + 1 < count => at + 1,
            _ => {
                self.recalled = None;
                self.clear();
                return;
            }
        };
        self.recalled = Some(Recalled(at));
        if let Some(line) = self.history.get(at) {
            self.text = line.clone();
        }
        self.caret = Caret(self.text.characters());
    }

    fn type_text(&mut self, typed: &Typed) {
        if typed.is_empty() {
            return;
        }
        if mem::replace(&mut self.marked, Marked::Nothing) == Marked::Whole {
            self.clear();
        }
        self.text.insert_at(self.caret.0, typed.as_str());
        self.caret = Caret(self.caret.0 + typed.as_str().chars().count());
    }

    pub(crate) fn shown(&self, attention: Attention, room: FieldRoom) -> Shown {
        match attention {
            Attention::Idle if self.text.is_empty() => Shown {
                looks: Looks::Hint,
                before: Label::default(),
                after: Label::default(),
            },
            Attention::Idle => Shown {
                looks: Looks::Plain,
                before: match room {
                    FieldRoom::Cells(cells) => {
                        Label::new(Clipped::right(self.text.as_str(), cells.get()).to_string())
                    }
                    FieldRoom::Open => self.text.label(),
                },
                after: Label::default(),
            },
            Attention::Focused => Shown {
                looks: if self.marked == Marked::Whole {
                    Looks::Marked
                } else {
                    Looks::Plain
                },
                before: self.text.before(self.caret.0),
                after: self.text.after(self.caret.0),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldRoom {
    Cells(Count),
    Open,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attention {
    Focused,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Kept,
    Lost,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Fields {
    search: Field,
    command: Field,
    filter: Field,
    tour_filter: Field,
    view_search: Field,
    line: Field,
    palette: Field,
    wizard_name: Field,
    wizard_group: Field,
    wizard_search: Field,
    wizard_note: Field,
    step_notes: BTreeMap<BranchId, Field>,
    blank: Field,
    focused: Option<Which>,
}

impl Fields {
    pub(crate) fn get(&self, which: Which) -> &Field {
        match which {
            Which::Search => &self.search,
            Which::Command => &self.command,
            Which::SymbolFilter => &self.filter,
            Which::TourFilter => &self.tour_filter,
            Which::ViewSearch => &self.view_search,
            Which::GoToLine => &self.line,
            Which::Palette => &self.palette,
            Which::WizardName => &self.wizard_name,
            Which::WizardGroup => &self.wizard_group,
            Which::WizardSearch => &self.wizard_search,
            Which::WizardNote => &self.wizard_note,
            Which::StepNote(branch) => self.step_notes.get(&branch).unwrap_or(&self.blank),
        }
    }

    fn get_mut(&mut self, which: Which) -> &mut Field {
        match which {
            Which::Search => &mut self.search,
            Which::Command => &mut self.command,
            Which::SymbolFilter => &mut self.filter,
            Which::TourFilter => &mut self.tour_filter,
            Which::ViewSearch => &mut self.view_search,
            Which::GoToLine => &mut self.line,
            Which::Palette => &mut self.palette,
            Which::WizardName => &mut self.wizard_name,
            Which::WizardGroup => &mut self.wizard_group,
            Which::WizardSearch => &mut self.wizard_search,
            Which::WizardNote => &mut self.wizard_note,
            Which::StepNote(branch) => self.step_notes.entry(branch).or_default(),
        }
    }

    pub(crate) const fn focused(&self) -> Option<Which> {
        self.focused
    }

    pub(crate) fn focus(&mut self, which: Which) {
        self.focused = Some(which);
    }

    pub(crate) fn start_empty(&mut self, which: Which) {
        self.get_mut(which).clear();
        self.focused = Some(which);
    }

    pub(crate) fn fill(&mut self, which: Which, text: &Label) {
        let field = self.get_mut(which);
        field.clear();
        field.marked = Marked::Nothing;
        field.text.insert_at(0, text.as_str());
        field.caret = Caret(field.text.characters());
    }

    pub(crate) fn drop_step_notes(&mut self) {
        self.step_notes.clear();
        if matches!(self.focused, Some(Which::StepNote(_))) {
            self.focused = None;
        }
    }

    pub(crate) fn release(&mut self, which: Which) {
        if self.focused == Some(which) {
            self.focused = None;
        }
    }

    pub(crate) fn press(&mut self, hot: Option<Id>) {
        if self
            .focused
            .is_some_and(|which| hot != Some(which.control().id()))
        {
            self.focused = None;
        }
    }

    pub(crate) fn handle(
        &mut self,
        which: Which,
        edits: &[Edit],
        typed: &Typed,
        enter: Enter,
    ) -> Option<FieldText> {
        if self.focused != Some(which) {
            return None;
        }
        let mut focus = Focus::Kept;
        let field = self.get_mut(which);
        let mut submitted = None;
        for edit in edits {
            if let Some(line) = field.apply(*edit, enter, &mut focus) {
                submitted = Some(line);
            }
        }
        field.type_text(typed);
        if focus == Focus::Lost && self.focused == Some(which) {
            self.focused = None;
        }
        submitted
    }
}
