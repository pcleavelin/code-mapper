use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::mem;

use platform::ClipboardRequest;
use ui::{Count, Glyph, Id, Label, Typed};

use crate::ids::{self, Control};
use crate::keys::{Extend, Walk};
use crate::text::Clipped;
use crate::wizard::BranchId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EnterMods {
    Plain,
    Shift,
    Control,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Character,
    Word,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Motion {
    Left(Unit),
    Right(Unit),
    RowStart,
    RowEnd,
    Up,
    Down,
    Start,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    Enter(EnterMods),
    Backspace(Unit),
    Remove(Unit),
    Move(Motion, Extend),
    SelectAll,
    Escape,
    ClearLine,
    Copy,
    Cut,
    Paste,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldShape {
    Single,
    Multi,
}

impl FieldShape {
    pub(crate) const fn breaks(self, mods: EnterMods) -> bool {
        matches!(
            (self, mods),
            (Self::Multi, EnterMods::Plain | EnterMods::Shift)
        )
    }
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
    TourNote,
    StepNote(BranchId),
    Comment,
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
            Self::TourNote => ids::TOUR_NOTE_FIELD,
            Self::StepNote(_) => ids::EDIT_NOTE_FIELD,
            Self::Comment => ids::COMMENT_FIELD,
        }
    }

    pub(crate) const fn shape(self) -> FieldShape {
        match self {
            Self::TourNote => FieldShape::Multi,
            Self::Search
            | Self::Command
            | Self::SymbolFilter
            | Self::TourFilter
            | Self::ViewSearch
            | Self::GoToLine
            | Self::Palette
            | Self::WizardName
            | Self::WizardGroup
            | Self::WizardSearch
            | Self::StepNote(_)
            | Self::Comment => FieldShape::Single,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AfterSubmit {
    Keep,
    Clear,
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

    fn remove(&mut self, from: Caret, to: Caret) {
        let start = self.byte_at(from.0);
        let end = self.byte_at(to.0);
        if start < end {
            self.0.replace_range(start..end, "");
        }
    }

    fn insert_at(&mut self, at: Caret, text: &str) {
        let byte = self.byte_at(at.0);
        self.0.insert_str(byte, text);
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    pub(crate) fn between(&self, from: Caret, to: Caret) -> Label {
        Label::new(
            self.0
                .chars()
                .skip(from.0)
                .take(to.0.saturating_sub(from.0))
                .collect::<String>(),
        )
    }

    pub(crate) fn label(&self) -> Label {
        Label::new(self.0.clone())
    }

    fn end(&self) -> Caret {
        Caret(self.characters())
    }

    fn class_at(&self, at: usize) -> Option<CharClass> {
        self.0
            .chars()
            .nth(at)
            .map(|character| CharClass::of(Glyph::new(character)))
    }

    fn word_left(&self, from: Caret) -> Caret {
        let mut at = from.0;
        while at > 0 && self.class_at(at - 1) == Some(CharClass::Space) {
            at -= 1;
        }
        let class = at.checked_sub(1).and_then(|before| self.class_at(before));
        while at > 0 && self.class_at(at - 1) == class {
            at -= 1;
        }
        Caret(at)
    }

    fn word_right(&self, from: Caret) -> Caret {
        let end = self.characters();
        let mut at = from.0;
        while at < end && self.class_at(at) == Some(CharClass::Space) {
            at += 1;
        }
        let class = self.class_at(at);
        while at < end && self.class_at(at) == class {
            at += 1;
        }
        Caret(at)
    }

    pub(crate) fn rows(&self, columns: Option<Count>) -> Vec<TextRow> {
        let characters: Vec<char> = self.0.chars().collect();
        let width = columns.map(|columns| columns.get().max(1));
        let is_space = |at: usize| characters.get(at) == Some(&' ');
        let mut rows = Vec::new();
        let mut start = 0;
        loop {
            let hard = characters
                .iter()
                .skip(start)
                .position(|character| *character == '\n')
                .map_or(characters.len(), |offset| start + offset);
            let mut from = start;
            while let Some(width) = width.filter(|width| hard - from > *width) {
                let limit = from + width;
                let end = (from + 1..=limit)
                    .rev()
                    .find(|at| is_space(*at))
                    .map_or(limit, |space| space + 1);
                rows.push(TextRow {
                    start: Caret(from),
                    end: Caret(end),
                    ending: Ending::Soft,
                });
                from = end;
            }
            rows.push(TextRow {
                start: Caret(from),
                end: Caret(hard),
                ending: Ending::Hard,
            });
            if hard >= characters.len() {
                return rows;
            }
            start = hard + 1;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Mark,
}

impl CharClass {
    fn of(glyph: Glyph) -> Self {
        let character = glyph.get();
        if character.is_whitespace() {
            Self::Space
        } else if character.is_alphanumeric() || character == '_' {
            Self::Word
        } else {
            Self::Mark
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ending {
    Soft,
    Hard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextRow {
    pub(crate) start: Caret,
    pub(crate) end: Caret,
    pub(crate) ending: Ending,
}

impl TextRow {
    fn last(self) -> Caret {
        match self.ending {
            Ending::Hard => self.end,
            Ending::Soft => Caret(self.end.0.saturating_sub(1).max(self.start.0)),
        }
    }

    fn at(self, column: Count) -> Caret {
        Caret((self.start.0 + column.get()).min(self.last().0))
    }
}

fn row_of(rows: &[TextRow], caret: Caret) -> Count {
    Count::new(rows.iter().rposition(|row| row.start <= caret).unwrap_or(0))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Caret(usize);

impl Caret {
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Recalled(usize);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FieldWindow {
    pub(crate) columns: Count,
    pub(crate) rows: Count,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldAct {
    Fit(FieldWindow),
    Point(Caret, Pointing),
    Scroll(Walk),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pointing {
    Press,
    Drag,
    Extend,
    Word,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Field {
    text: FieldText,
    caret: Caret,
    anchor: Option<Caret>,
    goal: Option<Count>,
    window: FieldWindow,
    first: Count,
    history: Vec<FieldText>,
    recalled: Option<Recalled>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Handled {
    pub(crate) submitted: Option<FieldText>,
    pub(crate) clipboard: ClipboardRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tint {
    Plain,
    Selected,
    Caret,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextSelection {
    pub(crate) from: Caret,
    pub(crate) to: Caret,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    pub(crate) text: Label,
    pub(crate) tint: Tint,
}

impl Piece {
    fn caret() -> Self {
        Self {
            text: Label::new("\u{258f}"),
            tint: Tint::Caret,
        }
    }
}

impl Field {
    pub(crate) fn text(&self) -> &FieldText {
        &self.text
    }

    pub(crate) const fn caret(&self) -> Caret {
        self.caret
    }

    pub(crate) const fn first(&self) -> Count {
        self.first
    }

    pub(crate) const fn window(&self) -> FieldWindow {
        self.window
    }

    pub(crate) fn text_selection(&self) -> Option<TextSelection> {
        let anchor = self.anchor?;
        match anchor.cmp(&self.caret) {
            Ordering::Less => Some(TextSelection {
                from: anchor,
                to: self.caret,
            }),
            Ordering::Greater => Some(TextSelection {
                from: self.caret,
                to: anchor,
            }),
            Ordering::Equal => None,
        }
    }

    fn columns(&self, shape: FieldShape) -> Option<Count> {
        match shape {
            FieldShape::Single => None,
            FieldShape::Multi => {
                (self.window.columns.get() > 1).then(|| Count::new(self.window.columns.get() - 1))
            }
        }
    }

    pub(crate) fn rows(&self, shape: FieldShape) -> Vec<TextRow> {
        self.text.rows(self.columns(shape))
    }

    pub(crate) fn idle_text(&self, room: Count) -> Label {
        Label::new(Clipped::right(self.text.as_str(), room.get()).to_string())
    }

    pub(crate) fn whole(&self) -> TextRow {
        TextRow {
            start: Caret(0),
            end: self.text.end(),
            ending: Ending::Hard,
        }
    }

    pub(crate) fn pieces(&self, row: TextRow, attention: Attention) -> Vec<Piece> {
        let focused = attention == Attention::Focused;
        let mut cuts = vec![row.start, row.end];
        let selection = self.text_selection().filter(|_| focused);
        if let Some(TextSelection { from, to }) = selection {
            cuts.push(from.max(row.start).min(row.end));
            cuts.push(to.max(row.start).min(row.end));
        }
        let caret_here = focused
            && row.start <= self.caret
            && (self.caret < row.end || (self.caret == row.end && row.ending == Ending::Hard));
        if caret_here {
            cuts.push(self.caret);
        }
        cuts.sort();
        cuts.dedup();
        let mut pieces = Vec::new();
        for pair in cuts.windows(2) {
            let (Some(from), Some(to)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            if caret_here && *from == self.caret {
                pieces.push(Piece::caret());
            }
            let selected = selection.is_some_and(|chosen| chosen.from <= *from && *to <= chosen.to);
            pieces.push(Piece {
                text: self.text.between(*from, *to),
                tint: if selected {
                    Tint::Selected
                } else {
                    Tint::Plain
                },
            });
        }
        if caret_here && self.caret == row.end {
            pieces.push(Piece::caret());
        }
        pieces
    }

    pub(crate) fn caret_at(
        &self,
        shape: FieldShape,
        row: Count,
        column: Count,
        attention: Attention,
    ) -> Caret {
        let rows = self.rows(shape);
        let Some(chosen) = rows.get(row.get()).or_else(|| rows.last()).copied() else {
            return Caret(0);
        };
        let on_caret_row = attention == Attention::Focused
            && rows.get(row_of(&rows, self.caret).get()) == Some(&chosen);
        let caret_column = self.caret.0.saturating_sub(chosen.start.0);
        let column = if on_caret_row && column.get() > caret_column {
            column.get() - 1
        } else {
            column.get()
        };
        chosen.at(Count::new(column))
    }

    fn clear(&mut self) {
        self.text.clear();
        self.caret = Caret(0);
        self.anchor = None;
        self.first = Count::ZERO;
    }

    fn take_selection(&mut self) -> bool {
        let Some(TextSelection { from, to }) = self.text_selection() else {
            return false;
        };
        self.text.remove(from, to);
        self.caret = from;
        self.anchor = None;
        true
    }

    fn insert(&mut self, text: &Label) {
        self.take_selection();
        self.anchor = None;
        self.text.insert_at(self.caret, text.as_str());
        self.caret = Caret(self.caret.0 + text.as_str().chars().count());
    }

    fn select_all(&mut self) {
        if self.text.is_empty() {
            self.anchor = None;
        } else {
            self.anchor = Some(Caret(0));
            self.caret = self.text.end();
        }
    }

    fn target(&mut self, motion: Motion, shape: FieldShape) -> Caret {
        let rows = self.rows(shape);
        let index = row_of(&rows, self.caret).get();
        let row = rows.get(index).copied();
        let caret = self.caret;
        let goal = match motion {
            Motion::Up | Motion::Down => *self.goal.get_or_insert_with(|| {
                Count::new(row.map_or(0, |row| caret.0.saturating_sub(row.start.0)))
            }),
            Motion::Left(_)
            | Motion::Right(_)
            | Motion::RowStart
            | Motion::RowEnd
            | Motion::Start
            | Motion::End => Count::ZERO,
        };
        match motion {
            Motion::Left(Unit::Character) => Caret(caret.0.saturating_sub(1)),
            Motion::Left(Unit::Word) => self.text.word_left(caret),
            Motion::Right(Unit::Character) => Caret((caret.0 + 1).min(self.text.end().0)),
            Motion::Right(Unit::Word) => self.text.word_right(caret),
            Motion::RowStart => row.map_or(Caret(0), |row| row.start),
            Motion::RowEnd => row.map_or(self.text.end(), TextRow::last),
            Motion::Up => index
                .checked_sub(1)
                .and_then(|above| rows.get(above))
                .map_or(Caret(0), |above| above.at(goal)),
            Motion::Down => rows
                .get(index + 1)
                .map_or(self.text.end(), |below| below.at(goal)),
            Motion::Start => Caret(0),
            Motion::End => self.text.end(),
        }
    }

    fn move_caret(&mut self, motion: Motion, extend: Extend, shape: FieldShape) {
        let to = match (motion, extend, self.text_selection()) {
            (Motion::Left(Unit::Character), Extend::Replace, Some(selection)) => selection.from,
            (Motion::Right(Unit::Character), Extend::Replace, Some(selection)) => selection.to,
            _ => self.target(motion, shape),
        };
        self.anchor = match extend {
            Extend::Replace => None,
            Extend::Extend => self.anchor.or(Some(self.caret)),
        };
        self.caret = to;
    }

    fn copied(&self) -> ClipboardRequest {
        self.text_selection()
            .map_or(ClipboardRequest::Keep, |selection| {
                ClipboardRequest::Copy(self.text.between(selection.from, selection.to))
            })
    }

    fn apply(
        &mut self,
        edit: Edit,
        after: AfterSubmit,
        shape: FieldShape,
        focus: &mut Focus,
    ) -> Handled {
        let mut handled = Handled::default();
        if !matches!(edit, Edit::Move(Motion::Up | Motion::Down, _)) {
            self.goal = None;
        }
        match edit {
            Edit::Enter(mods) if shape.breaks(mods) => self.insert(&Label::new("\n")),
            Edit::Enter(_) => {
                let line = match after {
                    AfterSubmit::Keep => {
                        self.select_all();
                        self.text.clone()
                    }
                    AfterSubmit::Clear => {
                        self.caret = Caret(0);
                        self.anchor = None;
                        mem::take(&mut self.text)
                    }
                };
                self.recalled = None;
                if !line.as_str().trim().is_empty() {
                    self.history.push(line.clone());
                    handled.submitted = Some(line);
                }
            }
            Edit::Backspace(unit) => {
                if !self.take_selection() {
                    let from = self.target(Motion::Left(unit), shape);
                    self.text.remove(from, self.caret);
                    self.caret = from;
                    self.anchor = None;
                }
            }
            Edit::Remove(unit) => {
                if !self.take_selection() {
                    let to = self.target(Motion::Right(unit), shape);
                    self.text.remove(self.caret, to);
                    self.anchor = None;
                }
            }
            Edit::Move(motion @ (Motion::Up | Motion::Down), Extend::Replace)
                if shape == FieldShape::Single =>
            {
                self.recall(motion);
            }
            Edit::Move(motion, extend) => self.move_caret(motion, extend, shape),
            Edit::SelectAll => self.select_all(),
            Edit::Escape => *focus = Focus::Lost,
            Edit::ClearLine => self.clear(),
            Edit::Copy => handled.clipboard = self.copied(),
            Edit::Cut => {
                handled.clipboard = self.copied();
                self.take_selection();
            }
            Edit::Paste => handled.clipboard = ClipboardRequest::Paste,
        }
        handled
    }

    fn recall(&mut self, motion: Motion) {
        if self.history.is_empty() {
            return;
        }
        let count = self.history.len();
        let at = match (self.recalled, motion) {
            (None, Motion::Up) => count - 1,
            (Some(Recalled(at)), Motion::Up) => at.saturating_sub(1),
            (Some(Recalled(at)), Motion::Down) if at + 1 < count => at + 1,
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
        self.anchor = None;
        self.caret = self.text.end();
    }

    fn type_text(&mut self, typed: &Typed, shape: FieldShape) {
        if typed.is_empty() {
            return;
        }
        let text: Label = typed
            .as_str()
            .chars()
            .filter(|character| *character != '\r')
            .map(|character| match (character, shape) {
                ('\n', FieldShape::Multi) => '\n',
                ('\n' | '\t', _) => ' ',
                (other, _) => other,
            })
            .collect::<String>()
            .into();
        self.goal = None;
        self.insert(&text);
    }

    fn point(&mut self, at: Caret, pointing: Pointing) {
        self.goal = None;
        match pointing {
            Pointing::Press => {
                self.anchor = Some(at);
                self.caret = at;
            }
            Pointing::Drag => self.caret = at,
            Pointing::Extend => {
                self.anchor = self.anchor.or(Some(self.caret));
                self.caret = at;
            }
            Pointing::Word => {
                let end = self.text.word_right(at);
                let start = self.text.word_left(end);
                self.anchor = Some(start);
                self.caret = end;
            }
        }
    }

    fn follow(&mut self, shape: FieldShape) {
        let shown = self.window.rows.get().max(1);
        let rows = self.rows(shape);
        let row = row_of(&rows, self.caret).get();
        let mut first = self.first.get();
        if row < first {
            first = row;
        }
        if row >= first + shown {
            first = row + 1 - shown;
        }
        self.first = Count::new(first.min(rows.len().saturating_sub(shown)));
    }

    fn scroll(&mut self, shape: FieldShape, walk: Walk) {
        let shown = self.window.rows.get().max(1);
        let most = self.rows(shape).len().saturating_sub(shown);
        let first = self.first.get();
        self.first = Count::new(match walk {
            Walk::Down => (first + 1).min(most),
            Walk::Up => first.saturating_sub(1),
        });
    }
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
    tour_note: Field,
    step_notes: BTreeMap<BranchId, Field>,
    comment: Field,
    blank: Field,
    focused: Option<Holding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Holding {
    which: Which,
    id: Id,
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
            Which::TourNote => &self.tour_note,
            Which::StepNote(branch) => self.step_notes.get(&branch).unwrap_or(&self.blank),
            Which::Comment => &self.comment,
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
            Which::TourNote => &mut self.tour_note,
            Which::StepNote(branch) => self.step_notes.entry(branch).or_default(),
            Which::Comment => &mut self.comment,
        }
    }

    pub(crate) fn focused(&self) -> Option<Which> {
        self.focused.map(|holding| holding.which)
    }

    pub(crate) fn focus(&mut self, which: Which) {
        self.focus_at(which, which.id());
    }

    pub(crate) fn focus_at(&mut self, which: Which, id: Id) {
        self.focused = Some(Holding { which, id });
    }

    pub(crate) fn start_empty(&mut self, which: Which) {
        self.get_mut(which).clear();
        self.focus(which);
    }

    pub(crate) fn fill(&mut self, which: Which, text: &Label) {
        let field = self.get_mut(which);
        field.clear();
        field.text.insert_at(Caret(0), text.as_str());
        field.caret = field.text.end();
    }

    pub(crate) fn drop_step_notes(&mut self) {
        self.step_notes.clear();
        if matches!(self.focused(), Some(Which::StepNote(_))) {
            self.focused = None;
        }
    }

    pub(crate) fn release(&mut self, which: Which) {
        if self.focused() == Some(which) {
            self.focused = None;
        }
    }

    pub(crate) fn act(&mut self, which: Which, act: FieldAct) {
        match act {
            FieldAct::Fit(window) => self.fit(which, window),
            FieldAct::Point(at, pointing) => self.point(which, at, pointing),
            FieldAct::Scroll(walk) => self.scroll(which, walk),
        }
    }

    fn fit(&mut self, which: Which, window: FieldWindow) {
        let field = self.get_mut(which);
        if field.window != window {
            field.window = window;
            field.follow(which.shape());
        }
    }

    fn point(&mut self, which: Which, at: Caret, pointing: Pointing) {
        let field = self.get_mut(which);
        field.point(at, pointing);
        field.follow(which.shape());
    }

    fn scroll(&mut self, which: Which, walk: Walk) {
        self.get_mut(which).scroll(which.shape(), walk);
    }

    pub(crate) fn press(&mut self, hot: Option<Id>) {
        if self.focused.is_some_and(|holding| hot != Some(holding.id)) {
            self.focused = None;
        }
    }

    pub(crate) fn handle(
        &mut self,
        which: Which,
        edits: &[Edit],
        typed: &Typed,
        after: AfterSubmit,
    ) -> Handled {
        let mut handled = Handled::default();
        if self.focused() != Some(which) {
            return handled;
        }
        let mut focus = Focus::Kept;
        let shape = which.shape();
        let field = self.get_mut(which);
        for edit in edits {
            let one = field.apply(*edit, after, shape, &mut focus);
            handled.submitted = one.submitted.or(handled.submitted);
            if one.clipboard != ClipboardRequest::Keep {
                handled.clipboard = one.clipboard;
            }
        }
        field.type_text(typed, shape);
        field.follow(shape);
        if focus == Focus::Lost && self.focused() == Some(which) {
            self.focused = None;
        }
        handled
    }
}
