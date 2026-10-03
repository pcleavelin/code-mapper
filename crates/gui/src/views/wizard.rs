use std::cmp::Ordering;

use domain::{
    AddedUnder, Depth, EditChange, GroupName, Note, StepId, SymbolId, Tour, TourKind, TourName,
    TreeEntry, Verdict,
};
use strum::VariantArray;
use ui::{Count, Extent, Icon, Label, Px, Run};

use crate::action::Action;
use crate::field::Which;
use crate::ids;
use crate::model::Model;
use crate::panels::View;
use crate::text::{Clipped, Counted, Needle, Noun, Tag};
use crate::theme::{
    ACCENT, Cells, EXPANDER_BUTTON, GREEN, PANEL_PADDING, PANEL_TEXT_ROOM, PIXEL, RED,
    START_PAGE_WIDTH, TEXT, WEAK,
};
use crate::welcome::Spell;
use crate::widgets::{Chosen, Container, Frame, Padding, Scroller};
use crate::wizard::{
    Branch, BranchId, Expander, Fold, Line, Mode, OutlineRow, Page, StepState, Tick, Wizard,
    WizardAct, verdict_words,
};

const MATCHES_SHOWN: Count = Count::new(12);
const SHORT_FIELD: Cells = Cells::new(32);
const LABEL_CELLS: Count = Count::new(7);
const ROW_CELLS: Count = Count::new(8);
const ROW_SLACK: Count = Count::new(3);
const EDIT_PAGE_WIDTH: Cells = Cells::new(120);

struct Columns(Count);

impl Columns {
    const fn get(&self) -> usize {
        self.0.get()
    }
}

pub(super) fn wizard_page(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>, area: Extent) {
    if let Mode::EditTour(tour) = wizard.mode() {
        edit_page(model, wizard, tour, frame, area);
        return;
    }
    let id = ids::wizard();
    frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let PageWidth { width, columns } = page_width(frame, area, START_PAGE_WIDTH);
    frame.start(Container::Centered);
    frame.start(Container::StartPage { width });
    let line = frame.row_height();
    frame.spacer(line + line);
    frame.title("Build a tour");
    header(frame, wizard.page());
    frame.spacer(line);
    match wizard.page() {
        Page::Name => name_page(model, wizard, frame),
        Page::Start => start_page(model, wizard, frame, &columns),
        Page::Steps => steps_page(model, wizard, frame, &columns),
        Page::Note => note_page(model, frame, &columns),
        Page::Create => create_page(model, wizard, frame, &columns),
    }
    if let Some(why) = wizard.refusal() {
        frame.spacer(line);
        frame.row_text(vec![Run::new(why.clone(), RED)]);
    }
    frame.spacer(line);
    buttons(frame, wizard.page());
    if wizard.page() != Page::Create {
        frame.spacer(line);
        frame.row_text(vec![Run::new("Preview", ACCENT)]);
        preview(model, wizard, frame, &columns);
    }
    frame.spacer(line + line);
    frame.finish();
    frame.finish();
    frame.finish();
}

fn header(frame: &mut Frame<'_>, current: Page) {
    let mut runs = Vec::new();
    for page in Page::VARIANTS.iter().copied() {
        if !runs.is_empty() {
            runs.push(Run::new("   ", WEAK));
        }
        let color = match page.cmp(&current) {
            Ordering::Equal => ACCENT,
            Ordering::Less => TEXT,
            Ordering::Greater => WEAK,
        };
        runs.push(Run::new(
            format!("{} {}", page.number().get(), page.word().as_str()),
            color,
        ));
    }
    frame.row_text(runs);
}

fn ask(frame: &mut Frame<'_>, text: impl Into<Label>) {
    frame.note(text, WEAK, Padding::Step);
}

fn caption(text: &Spell) -> Label {
    Label::new(format!(
        "{:<width$}",
        text.as_str(),
        width = LABEL_CELLS.get()
    ))
}

fn row_caption(text: &Spell) -> Label {
    Label::new(format!(
        "{:<width$}",
        text.as_str(),
        width = ROW_CELLS.get()
    ))
}

const fn meaning(kind: TourKind) -> Spell {
    match kind {
        TourKind::Flow => Spell::new("what happens when X: a path through the calls"),
        TourKind::Layer => Spell::new("an abstraction boundary and the functions on its surface"),
        TourKind::Data => Spell::new("a data structure and what mutates it"),
    }
}

fn name_page(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>) {
    ask(
        frame,
        "What is the tour called, and what kind of tour is it?",
    );
    frame.start(Container::ToolbarSmall);
    frame.plain_line(caption(&Spell::new("name")), WEAK);
    frame.field(
        &model.fields,
        Which::WizardName,
        &Label::new("e.g. startup"),
        SHORT_FIELD,
    );
    frame.finish();
    for kind in TourKind::VARIANTS.iter().copied() {
        frame.start(Container::ToolbarSmall);
        frame.plain_line(
            caption(&Spell::new(if kind == TourKind::Flow {
                "kind"
            } else {
                ""
            })),
            WEAK,
        );
        let word = Tag::kind(kind).to_string();
        let chosen = if kind == wizard.kind() {
            Chosen::Chosen
        } else {
            Chosen::Plain
        };
        if frame
            .button(
                format!("{word:<5}"),
                ids::WIZARD_KIND.with(&Label::new(&word)),
                chosen,
            )
            .clicked()
        {
            frame.push(Action::Wizard(WizardAct::Kind(kind)));
        }
        frame.note(meaning(kind).as_str(), WEAK, Padding::Step);
        frame.finish();
    }
    frame.start(Container::ToolbarSmall);
    frame.plain_line(caption(&Spell::new("group")), WEAK);
    frame.field(
        &model.fields,
        Which::WizardGroup,
        &Label::new("(none)"),
        SHORT_FIELD,
    );
    frame.note(
        "where it sits in the Tours list, / nests",
        WEAK,
        Padding::Step,
    );
    frame.finish();
}

struct Place {
    name: Label,
    kind: Label,
    at: Label,
}

fn place(model: &Model, symbol: SymbolId) -> Option<Place> {
    let found = model.index.symbol(symbol)?;
    let file = model.index.file(symbol.file())?;
    Some(Place {
        name: Label::new(found.name().as_str()),
        kind: Label::new(found.kind().as_str()),
        at: Label::new(format!(
            "{}:{}",
            file.path().as_str(),
            found.span().start().number()
        )),
    })
}

fn padded(text: &Label, width: Count) -> Label {
    let shown = Clipped::right(text.as_str(), width.get()).to_string();
    Label::new(format!("{shown:<width$}", width = width.get()))
}

fn start_page(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>, columns: &Columns) {
    ask(
        frame,
        "Which symbol does the tour start from? Click one in Symbols, Source, References or the Graph, or search for it here.",
    );
    focus_choice(model, wizard, frame);
    matches(model, wizard, frame, columns);
}

fn focus_choice(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>) {
    let chosen = wizard.start().and_then(|symbol| place(model, symbol));
    frame.row_text(match chosen {
        Some(found) => vec![
            Run::new(row_caption(&Spell::new("start")), WEAK),
            Run::new(found.name, ACCENT),
            Run::new(
                format!("  {}  {}", found.kind.as_str(), found.at.as_str()),
                WEAK,
            ),
        ],
        None => vec![
            Run::new(row_caption(&Spell::new("start")), WEAK),
            Run::new("nothing chosen yet", WEAK),
        ],
    });
    if let Some(focus) = model.nav.focus()
        && let Some(found) = place(model, focus)
    {
        frame.start(Container::ToolbarSmall);
        frame.plain_line(caption(&Spell::new("focus")), WEAK);
        frame.label(found.name, TEXT);
        frame.label(found.at, WEAK);
        if wizard.start() == Some(focus) {
            frame.label("chosen", ACCENT);
        } else if frame
            .small_button("use it", ids::WIZARD_USE_FOCUS.target())
            .clicked()
        {
            frame.push(Action::Wizard(WizardAct::UseFocus));
        }
        frame.finish();
    }
}

fn matches(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>, columns: &Columns) {
    let needle = Needle::new(model.fields.get(Which::WizardSearch).text().as_str());
    let matching: Vec<SymbolId> = if needle.is_empty() {
        model
            .index
            .roots()
            .into_iter()
            .filter(|id| !model.index.in_tests(*id))
            .take(MATCHES_SHOWN.get())
            .collect()
    } else {
        model
            .index
            .symbol_ids()
            .filter(|id| {
                model
                    .index
                    .symbol(*id)
                    .is_some_and(|symbol| needle.found_in(symbol.name().as_str()))
            })
            .collect()
    };
    frame.start(Container::ToolbarSmall);
    frame.plain_line(caption(&Spell::new("search")), WEAK);
    frame.field(
        &model.fields,
        Which::WizardSearch,
        &Label::new("symbol name"),
        SHORT_FIELD,
    );
    frame.note(
        if needle.is_empty() {
            "entry points, nothing calls them".to_owned()
        } else {
            Counted::new(Count::new(matching.len()), Noun::Symbol).to_string()
        },
        WEAK,
        Padding::Step,
    );
    frame.finish();
    let places: Vec<(SymbolId, Place)> = matching
        .iter()
        .take(MATCHES_SHOWN.get())
        .filter_map(|id| Some((*id, place(model, *id)?)))
        .collect();
    let widest = places
        .iter()
        .map(|(_, spot)| spot.name.as_str().chars().count())
        .max()
        .unwrap_or(0)
        .min(columns.get() / 3);
    let kinds = places
        .iter()
        .map(|(_, spot)| spot.kind.as_str().chars().count())
        .max()
        .unwrap_or(0);
    let path_room = columns
        .get()
        .saturating_sub(ROW_CELLS.get() + widest + 2 + kinds + 2)
        .max(1);
    for (row, (id, spot)) in places.iter().enumerate() {
        let runs = vec![
            Run::new(" ".repeat(ROW_CELLS.get()), WEAK),
            Run::new(padded(&spot.name, Count::new(widest)), TEXT),
            Run::new(
                format!(
                    "  {}  {}",
                    padded(&spot.kind, Count::new(kinds)).as_str(),
                    Clipped::left(spot.at.as_str(), path_room)
                ),
                WEAK,
            ),
        ];
        let look = if wizard.start() == Some(*id) {
            Chosen::Chosen
        } else {
            Chosen::Plain
        };
        if frame
            .row(runs, ids::WIZARD_SYMBOL.nth(Count::new(row)), look)
            .clicked()
        {
            frame.push(Action::Wizard(WizardAct::Choose(*id)));
        }
    }
    let more = matching.len().saturating_sub(places.len());
    if more > 0 {
        frame.row_text(vec![
            Run::new(" ".repeat(ROW_CELLS.get()), WEAK),
            Run::new(format!("and {more} more; type more of the name"), WEAK),
        ]);
    }
}

fn indent_of(depth: Depth) -> Count {
    Count::new(usize::try_from(depth.value()).unwrap_or(0) * 2)
}

fn name_of(model: &Model, symbol: SymbolId) -> Label {
    Label::new(
        model
            .index
            .symbol(symbol)
            .map_or("", |found| found.name().as_str()),
    )
}

fn tree_width(model: &Model, entries: &[TreeEntry], columns: &Columns) -> Count {
    Count::new(
        entries
            .iter()
            .map(|entry| {
                indent_of(entry.depth).get() + name_of(model, entry.symbol).as_str().chars().count()
            })
            .max()
            .unwrap_or(0)
            .min(columns.get() / 3),
    )
}

struct Room {
    name: Count,
    path: Count,
}

fn place_runs(found: &Place, indent: Count, room: &Room, color: ui::Color) -> Vec<Run> {
    let named = Label::new(format!(
        "{}{}",
        " ".repeat(indent.get()),
        found.name.as_str()
    ));
    vec![
        Run::new(padded(&named, room.name), color),
        Run::new(
            format!(
                "  {}",
                padded(
                    &Label::new(
                        Clipped::left(found.at.as_str(), room.path.get().max(1)).to_string()
                    ),
                    room.path
                )
                .as_str()
            ),
            WEAK,
        ),
    ]
}

fn tree_runs(model: &Model, entry: TreeEntry, room: &Room, color: ui::Color) -> Vec<Run> {
    place(model, entry.symbol).map_or_else(Vec::new, |found| {
        place_runs(&found, indent_of(entry.depth), room, color)
    })
}

const BOX_CELLS: Count = Count::new(4);

fn lines_place(model: &Model, wizard: &Wizard, step: &StepId) -> Option<Place> {
    let Mode::EditTour(tour) = wizard.mode() else {
        return None;
    };
    let found = model.map.step(tour, step)?;
    Some(Place {
        name: Label::new("(lines)"),
        kind: Label::default(),
        at: Label::new(format!(
            "{}:{}",
            found.file().as_str(),
            found.span().start().number()
        )),
    })
}

fn branch_place(model: &Model, wizard: &Wizard, branch: &Branch) -> Option<Place> {
    match branch.symbol() {
        Some(symbol) => place(model, symbol),
        None => lines_place(model, wizard, branch.step()?),
    }
}

fn number_of(branch: &Branch) -> Label {
    match &branch.state {
        StepState::Existing { number, .. } => Label::new(number.to_string()),
        StepState::New => Label::new("new"),
    }
}

fn kept_note(model: &Model, wizard: &Wizard, branch: &Branch) -> Option<Note> {
    let Mode::EditTour(tour) = wizard.mode() else {
        return None;
    };
    model.map.step(tour, branch.step()?)?.note().cloned()
}

fn status_of(branch: &Branch) -> Filled {
    match (&branch.state, branch.tick) {
        (StepState::Existing { .. }, Tick::Ticked) => Filled {
            text: Label::default(),
            color: WEAK,
        },
        (StepState::Existing { .. }, Tick::Unticked) => Filled {
            text: Label::new("removed"),
            color: RED,
        },
        (StepState::New, _) if branch.depth() == Depth::default() => Filled {
            text: Label::new("the start"),
            color: WEAK,
        },
        (StepState::New, _) => Filled {
            text: Label::new(verdict_words(branch.verdict()).as_str()),
            color: WEAK,
        },
    }
}

fn steps_page(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>, columns: &Columns) {
    let outline = wizard.outline();
    if outline.is_empty() {
        ask(frame, "Choose a start symbol first: go back to page 2.");
        return;
    }
    ask(
        frame,
        "Which calls become steps? promote's rules ticked these, two calls deep. Open any row to see what it calls, as deep as you like. Tick a row's box to make it a step: ticking one ticks the rows above it, unticking one unticks the rows under it.",
    );
    outline_rows(model, wizard, frame, columns);
}

fn number_width(wizard: &Wizard, branch: &Branch) -> Count {
    match wizard.mode() {
        Mode::Build => Count::ZERO,
        Mode::EditTour(_) => Count::new(number_of(branch).as_str().chars().count() + 1),
    }
}

fn outline_rows(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>, columns: &Columns) {
    let outline = wizard.outline();
    let rows = wizard.rows();
    let branches: Vec<&Branch> = rows
        .iter()
        .filter_map(|shaped| match &shaped.line {
            Line::Branch(id) => outline.branch(*id),
            Line::Fold(_) => None,
        })
        .collect();
    let places: Vec<Option<Place>> = branches
        .iter()
        .map(|branch| branch_place(model, wizard, branch))
        .collect();
    let share = match wizard.mode() {
        Mode::Build => 3,
        Mode::EditTour(_) => 2,
    };
    let width = Count::new(
        branches
            .iter()
            .zip(&places)
            .map(|(branch, found)| {
                indent_of(branch.depth()).get()
                    + number_width(wizard, branch).get()
                    + found
                        .as_ref()
                        .map_or(0, |spot| spot.name.as_str().chars().count())
            })
            .max()
            .unwrap_or(0)
            .min(columns.get() / share),
    );
    let paths = Count::new(
        places
            .iter()
            .flatten()
            .map(|found| found.at.as_str().chars().count())
            .max()
            .unwrap_or(0),
    );
    let reasons = Count::new(
        branches
            .iter()
            .map(|branch| status_of(branch).text.as_str().chars().count())
            .max()
            .unwrap_or(0),
    );
    let tree = Tree {
        model,
        wizard,
        width,
        reasons,
        paths,
        columns,
    };
    for (row, shaped) in rows.iter().enumerate() {
        match &shaped.line {
            Line::Branch(id) => tree.branch_row(frame, Count::new(row), *id, shaped),
            Line::Fold(fold) => tree.fold_row(frame, Count::new(row), fold, shaped),
        }
    }
}

struct Tree<'tree> {
    model: &'tree Model,
    wizard: &'tree Wizard,
    width: Count,
    reasons: Count,
    paths: Count,
    columns: &'tree Columns,
}

const MORE_CELLS: Count = Count::new(13);

impl Tree<'_> {
    fn lead(frame: &mut Frame<'_>, indent: Count) {
        frame.start(Container::StepRow {
            selected: Chosen::Plain,
        });
        frame.cells_gap(Cells::of_count(indent.get()));
    }

    fn expander(frame: &mut Frame<'_>, row: Count, expander: Expander, act: WizardAct) {
        let icon = match expander {
            Expander::Leaf => {
                frame.small_button_room(EXPANDER_BUTTON);
                return;
            }
            Expander::Closed => Icon::Collapsed,
            Expander::Open => Icon::Expanded,
        };
        if frame
            .small_button_sized(icon, Some(EXPANDER_BUTTON), ids::WIZARD_OPEN.nth(row))
            .clicked()
        {
            frame.push(Action::Wizard(act));
        }
    }

    fn room_left(&self, indent: Count) -> Count {
        Count::new(
            self.columns.get().saturating_sub(
                usize::try_from(EXPANDER_BUTTON.get() + 1).unwrap_or(0) + indent.get(),
            ),
        )
    }

    fn editing(&self) -> bool {
        matches!(self.wizard.mode(), Mode::EditTour(_))
    }

    fn branch_runs(&self, branch: &Branch, indent: Count) -> Vec<Run> {
        let model = self.model;
        let room = self.room_left(indent);
        let ticked = branch.tick == Tick::Ticked;
        if branch.verdict() == Verdict::Cycle {
            return vec![
                Run::new(" ".repeat(BOX_CELLS.get()), WEAK),
                Run::new(
                    Clipped::right(
                        &format!(
                            "calls back into {}",
                            branch
                                .symbol()
                                .map(|symbol| name_of(model, symbol))
                                .unwrap_or_default()
                                .as_str()
                        ),
                        room.get().saturating_sub(BOX_CELLS.get()),
                    )
                    .to_string(),
                    WEAK,
                ),
            ];
        }
        let status = status_of(branch);
        let number = number_width(self.wizard, branch);
        let more = if self.editing() { MORE_CELLS.get() } else { 0 };
        let name = Count::new(
            self.width
                .get()
                .saturating_sub(indent.get() + number.get())
                .max(1),
        );
        let path = Count::new(
            room.get()
                .saturating_sub(
                    BOX_CELLS.get() + number.get() + name.get() + 2 + 2 + self.reasons.get() + more,
                )
                .min(self.paths.get()),
        );
        let mut runs = Vec::new();
        if self.editing() {
            let color = match branch.state {
                StepState::New => GREEN,
                StepState::Existing { .. } => WEAK,
            };
            runs.push(Run::new(format!("{} ", number_of(branch).as_str()), color));
        }
        if let Some(found) = branch_place(model, self.wizard, branch) {
            runs.extend(place_runs(
                &found,
                Count::ZERO,
                &Room { name, path },
                if ticked { TEXT } else { WEAK },
            ));
        }
        if !status.text.as_str().is_empty() {
            runs.push(Run::new(
                format!("  {}", status.text.as_str()),
                status.color,
            ));
        }
        runs
    }

    fn branch_row(&self, frame: &mut Frame<'_>, row: Count, id: BranchId, shaped: &OutlineRow) {
        let model = self.model;
        let outline = self.wizard.outline();
        let Some(branch) = outline.branch(id) else {
            return;
        };
        let indent = indent_of(branch.depth());
        Self::lead(frame, indent);
        Self::expander(frame, row, shaped.expander, WizardAct::Expand(id));
        if branch.verdict() != Verdict::Cycle
            && frame
                .tick_box(
                    branch.tick,
                    Cells::of_count(BOX_CELLS.get()),
                    ids::WIZARD_TICK.nth(row),
                )
                .clicked()
        {
            frame.push(Action::Wizard(WizardAct::Toggle(id)));
        }
        frame.row_text(self.branch_runs(branch, indent));
        if let Some(count) = shaped.more {
            let count = count.get();
            if frame
                .small_button(
                    format!("+{count} call{}", if count == 1 { "" } else { "s" }),
                    ids::EDIT_MORE.nth(row),
                )
                .clicked()
            {
                frame.push(Action::Wizard(WizardAct::More(id)));
            }
        }
        frame.finish();
        let ticked = branch.tick == Tick::Ticked;
        if self.editing() && ticked && branch.verdict() != Verdict::Cycle {
            self.note_row(frame, row, indent, id);
        }
        if self.editing()
            && !ticked
            && let Some(note) = kept_note(model, self.wizard, branch)
        {
            self.lost_row(frame, indent, &note);
        }
    }

    fn under_row(&self, frame: &mut Frame<'_>, indent: Count) -> Count {
        Self::lead(frame, indent);
        frame.small_button_room(EXPANDER_BUTTON);
        frame.cells_gap(Cells::of_count(BOX_CELLS.get()));
        Count::new(
            self.room_left(indent)
                .get()
                .saturating_sub(BOX_CELLS.get() + 1)
                .max(1),
        )
    }

    fn lost_row(&self, frame: &mut Frame<'_>, indent: Count, note: &Note) {
        let room = self.under_row(frame, indent);
        frame.label(
            Clipped::right(&format!("its note is lost: {}", note.as_str()), room.get()).to_string(),
            RED,
        );
        frame.finish();
    }

    fn note_row(&self, frame: &mut Frame<'_>, row: Count, indent: Count, id: BranchId) {
        let room = self.under_row(frame, indent);
        frame.field_named(
            &self.model.fields,
            Which::StepNote(id),
            ids::EDIT_NOTE_FIELD.nth(row).id(),
            &Label::new("what this step does for the tour"),
            Cells::of_count(room.get()),
        );
        frame.finish();
    }

    fn fold_row(&self, frame: &mut Frame<'_>, row: Count, fold: &Fold, shaped: &OutlineRow) {
        let outline = self.wizard.outline();
        let indent = fold
            .members
            .first()
            .and_then(|member| outline.branch(*member))
            .map_or(Count::ZERO, |branch| indent_of(branch.depth()));
        Self::lead(frame, indent);
        let act = WizardAct::Fold(fold.parent, fold.cut);
        Self::expander(frame, row, shaped.expander, act);
        let ticked = fold
            .members
            .iter()
            .filter_map(|member| outline.branch(*member))
            .filter(|branch| branch.tick == Tick::Ticked)
            .count();
        let mut runs = vec![
            Run::new(" ".repeat(BOX_CELLS.get()), WEAK),
            Run::new(
                format!(
                    "{} left out: {}",
                    fold.members.len(),
                    verdict_words(Verdict::Cut(fold.cut)).as_str()
                ),
                WEAK,
            ),
        ];
        if ticked > 0 {
            runs.push(Run::new(format!("  {ticked} ticked"), ACCENT));
        }
        if frame
            .row(runs, ids::WIZARD_FOLD.nth(row), Chosen::Plain)
            .clicked()
        {
            frame.push(Action::Wizard(act));
        }
        frame.finish();
    }
}

fn note_page(model: &Model, frame: &mut Frame<'_>, columns: &Columns) {
    ask(
        frame,
        "What is the tour for, as a whole? Step notes are written later, in the Tour view.",
    );
    frame.start(Container::ToolbarSmall);
    frame.plain_line(caption(&Spell::new("note")), WEAK);
    frame.field(
        &model.fields,
        Which::WizardNote,
        &Label::new("what happens, as a whole"),
        Cells::of_count(columns.get().saturating_sub(LABEL_CELLS.get() + 4)),
    );
    frame.finish();
}

fn summary_line(frame: &mut Frame<'_>, what: &Spell, value: Filled) {
    frame.row_text(vec![
        Run::new(row_caption(what), WEAK),
        Run::new(value.text, value.color),
    ]);
}

fn ticked_tree(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>, columns: &Columns) {
    let ticked = wizard.outline().ticked();
    if ticked.is_empty() {
        frame.row_text(vec![
            Run::new(" ".repeat(ROW_CELLS.get()), WEAK),
            Run::new("no steps: no start symbol chosen yet", WEAK),
        ]);
        return;
    }
    let width = tree_width(model, &ticked, columns);
    let room = Room {
        name: width,
        path: Count::new(
            columns
                .get()
                .saturating_sub(ROW_CELLS.get() + width.get() + 2),
        ),
    };
    for entry in ticked {
        let mut runs = vec![Run::new(" ".repeat(ROW_CELLS.get()), WEAK)];
        runs.extend(tree_runs(model, entry, &room, TEXT));
        frame.row_text(runs);
    }
}

fn field_text(model: &Model, which: Which) -> Label {
    Label::new(model.fields.get(which).text().as_str().trim())
}

struct Filled {
    text: Label,
    color: ui::Color,
}

fn or_none(text: Label) -> Filled {
    if text.as_str().is_empty() {
        Filled {
            text: Label::new("(none)"),
            color: WEAK,
        }
    } else {
        Filled { text, color: TEXT }
    }
}

fn create_page(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>, columns: &Columns) {
    ask(
        frame,
        "Create adds this tour to the map, unsaved, and opens it. Save with ctrl+s.",
    );
    summary_line(frame, &Spell::new("name"), or_none(model.typed_name()));
    summary_line(
        frame,
        &Spell::new("kind"),
        or_none(Label::new(Tag::kind(wizard.kind()).to_string())),
    );
    summary_line(
        frame,
        &Spell::new("group"),
        or_none(field_text(model, Which::WizardGroup)),
    );
    summary_line(
        frame,
        &Spell::new("note"),
        or_none(field_text(model, Which::WizardNote)),
    );
    let steps = wizard.outline().ticked().len();
    summary_line(
        frame,
        &Spell::new("steps"),
        or_none(Label::new(
            Counted::new(Count::new(steps), Noun::Step).to_string(),
        )),
    );
    ticked_tree(model, wizard, frame, columns);
}

fn buttons(frame: &mut Frame<'_>, page: Page) {
    frame.start(Container::ToolbarSmall);
    if page != Page::Name
        && frame
            .button("back", ids::WIZARD_BACK.target(), Chosen::Plain)
            .clicked()
    {
        frame.push(Action::Wizard(WizardAct::Back));
    }
    let next = if page == Page::Create {
        frame.button("create", ids::WIZARD_CREATE.target(), Chosen::Chosen)
    } else {
        frame.button("next", ids::WIZARD_NEXT.target(), Chosen::Chosen)
    };
    if next.clicked() {
        frame.push(Action::Wizard(WizardAct::Next));
    }
    if frame
        .button("cancel", ids::WIZARD_CANCEL.target(), Chosen::Plain)
        .clicked()
    {
        frame.push(Action::Wizard(WizardAct::Cancel));
    }
    frame.note(
        if page == Page::Create {
            "enter creates"
        } else {
            "enter goes on; escape cancels if nothing is entered"
        },
        WEAK,
        Padding::Step,
    );
    frame.finish();
}

fn preview(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>, columns: &Columns) {
    let name = or_none(model.typed_name());
    let group = field_text(model, Which::WizardGroup);
    let mut runs = vec![
        Run::new(row_caption(&Spell::new("tour")), WEAK),
        Run::new(name.text, name.color),
        Run::new(format!("  {}", Tag::kind(wizard.kind())), ACCENT),
    ];
    if !group.as_str().is_empty() {
        runs.push(Run::new(format!("  in {}/", group.as_str()), WEAK));
    }
    frame.row_text(runs);
    let note = field_text(model, Which::WizardNote);
    if !note.as_str().is_empty() {
        frame.row_text(vec![
            Run::new(row_caption(&Spell::new("")), WEAK),
            Run::new(note, WEAK),
        ]);
    }
    ticked_tree(model, wizard, frame, columns);
}

pub(super) fn wizard_strip(model: &Model, wizard: &Wizard, frame: &mut Frame<'_>, view: View) {
    let place = Label::new(view.name().as_str());
    frame.start(Container::ToolbarSmall);
    if let Mode::EditTour(tour) = wizard.mode() {
        frame.label("editing", WEAK);
        frame.label(tour.as_str(), TEXT);
        if frame
            .small_button("back to the edit", ids::WIZARD_RETURN.with(&place))
            .clicked()
        {
            frame.push(Action::Wizard(WizardAct::Return));
        }
        frame.finish();
        return;
    }
    frame.label("building", WEAK);
    let typed = model.typed_name();
    if typed.as_str().is_empty() {
        frame.label("a new tour", TEXT);
    } else {
        frame.label(typed, TEXT);
    }
    if wizard.page() == Page::Start {
        frame.label("  start =", WEAK);
        match wizard.start().and_then(|symbol| location_of(model, symbol)) {
            Some(name) => frame.label(name, ACCENT),
            None => frame.label("click a symbol", WEAK),
        }
    } else {
        frame.label(
            format!(
                "  on page {} {}",
                wizard.page().number().get(),
                wizard.page().word().as_str()
            ),
            WEAK,
        );
    }
    if frame
        .small_button("back to the wizard", ids::WIZARD_RETURN.with(&place))
        .clicked()
    {
        frame.push(Action::Wizard(WizardAct::Return));
    }
    frame.finish();
}

fn location_of(model: &Model, symbol: SymbolId) -> Option<Label> {
    let found = place(model, symbol)?;
    Some(Label::new(format!(
        "{}  {}",
        found.name.as_str(),
        found.at.as_str()
    )))
}

fn edit_page(model: &Model, wizard: &Wizard, tour: &TourName, frame: &mut Frame<'_>, area: Extent) {
    let changes = wizard.changes();
    edit_bar(wizard, tour, frame, changes);
    let id = ids::wizard();
    frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let PageWidth { width, columns } = page_width(frame, area, EDIT_PAGE_WIDTH);
    frame.start(Container::Centered);
    frame.start(Container::StartPage { width });
    let line = frame.row_height();
    frame.spacer(line);
    section(frame, &Spell::new("Tour"));
    frame.start(Container::ToolbarSmall);
    frame.plain_line(caption(&Spell::new("name")), WEAK);
    frame.field(
        &model.fields,
        Which::WizardName,
        &Label::new("the tour's name"),
        SHORT_FIELD,
    );
    if let Some(why) = wizard.invalid() {
        frame.label(why.clone(), RED);
    }
    frame.finish();
    frame.start(Container::ToolbarSmall);
    frame.plain_line(caption(&Spell::new("kind")), WEAK);
    for kind in TourKind::VARIANTS.iter().copied() {
        let word = Tag::kind(kind).to_string();
        let chosen = if kind == wizard.kind() {
            Chosen::Chosen
        } else {
            Chosen::Plain
        };
        if frame
            .button(
                word.clone(),
                ids::WIZARD_KIND.with(&Label::new(&word)),
                chosen,
            )
            .clicked()
        {
            frame.push(Action::Wizard(WizardAct::Kind(kind)));
        }
    }
    frame.note(meaning(wizard.kind()).as_str(), WEAK, Padding::Step);
    frame.finish();
    frame.start(Container::ToolbarSmall);
    frame.plain_line(caption(&Spell::new("group")), WEAK);
    frame.field(
        &model.fields,
        Which::WizardGroup,
        &Label::new("(none)"),
        SHORT_FIELD,
    );
    frame.note(
        "where it sits in the Tours list, / nests",
        WEAK,
        Padding::Step,
    );
    frame.finish();
    frame.start(Container::ToolbarSmall);
    frame.plain_line(caption(&Spell::new("note")), WEAK);
    frame.field(
        &model.fields,
        Which::WizardNote,
        &Label::new("what happens, as a whole"),
        Cells::of_count(columns.get().saturating_sub(LABEL_CELLS.get() + 2)),
    );
    frame.finish();
    frame.spacer(line);
    section(frame, &Spell::new("Steps"));
    ask(
        frame,
        "Each step's note is the field under it. Untick a step's box to remove it with the steps under it. Open a step to see the calls it makes that the tour has not, and tick one to add it there.",
    );
    outline_rows(model, wizard, frame, &columns);
    frame.spacer(line);
    section(frame, &Spell::new("Changes"));
    change_rows(model, wizard, tour, frame, changes, &columns);
    frame.spacer(line + line);
    frame.finish();
    frame.finish();
    frame.finish();
}

struct PageWidth {
    width: Px,
    columns: Columns,
}

fn page_width(frame: &Frame<'_>, area: Extent, cells: Cells) -> PageWidth {
    let cell = frame.cell_width().max(PIXEL);
    let room = area.width - PANEL_TEXT_ROOM - Frame::scrollbar_width() - PANEL_PADDING * 2;
    let width = Px::new(cell.get() * cells.get()).min(room).max(cell);
    let columns = Columns(Count::new(
        usize::try_from(width.ratio(cell))
            .unwrap_or(0)
            .saturating_sub(ROW_SLACK.get()),
    ));
    PageWidth { width, columns }
}

fn section(frame: &mut Frame<'_>, title: &Spell) {
    frame.row_text(vec![Run::new(title.as_str(), ACCENT)]);
}

fn edit_bar(wizard: &Wizard, tour: &TourName, frame: &mut Frame<'_>, changes: &[EditChange]) {
    let invalid = wizard.invalid();
    frame.start(Container::TourHeader);
    frame.title(format!("Editing {tour}"));
    frame.grow();
    let count = changes.len();
    let apply = match count {
        0 => "nothing to apply".to_owned(),
        1 => "apply 1 change".to_owned(),
        _ => format!("apply {count} changes"),
    };
    let look = if count > 0 && invalid.is_none() {
        Chosen::Chosen
    } else {
        Chosen::Plain
    };
    if frame
        .button(apply, ids::EDIT_APPLY.target(), look)
        .clicked()
    {
        frame.push(Action::Wizard(WizardAct::Apply));
    }
    if frame
        .button("cancel", ids::EDIT_CANCEL.target(), Chosen::Plain)
        .clicked()
    {
        frame.push(Action::Wizard(WizardAct::Cancel));
    }
    frame.label("ctrl+enter applies; escape cancels if unchanged", WEAK);
    frame.finish();
    let lost = changes
        .iter()
        .filter(|change| matches!(change, EditChange::Removed { lost: Some(_), .. }))
        .count();
    let why = wizard.refusal().or(invalid);
    if why.is_some() || lost > 0 {
        frame.start(Container::ToolbarSmall);
        if let Some(why) = why {
            frame.label(why.clone(), RED);
        }
        if lost > 0 {
            frame.label(
                format!(
                    "{} removed with {} note{}: see Changes",
                    Counted::new(Count::new(lost), Noun::Step),
                    if lost == 1 { "its" } else { "their" },
                    if lost == 1 { "" } else { "s" }
                ),
                RED,
            );
        }
        frame.finish();
    }
}

fn step_named(wizard: &Wizard, model: &Model, step: &StepId) -> Label {
    let outline = wizard.outline();
    outline
        .ids()
        .filter_map(|id| outline.branch(id))
        .find(|branch| branch.step() == Some(step))
        .map_or_else(Label::default, |branch| {
            let name =
                branch_place(model, wizard, branch).map_or_else(Label::default, |found| found.name);
            Label::new(format!("{} {}", number_of(branch).as_str(), name.as_str()))
        })
}

fn clip(note: Option<&Note>, room: Count) -> Label {
    note.map_or_else(
        || Label::new("(none)"),
        |note| {
            Label::new(format!(
                "\"{}\"",
                Clipped::right(note.as_str(), room.get().max(8))
            ))
        },
    )
}

fn change_rows(
    model: &Model,
    wizard: &Wizard,
    tour: &TourName,
    frame: &mut Frame<'_>,
    changes: &[EditChange],
    columns: &Columns,
) {
    if changes.is_empty() {
        ask(frame, "Nothing changed yet.");
        return;
    }
    ask(
        frame,
        "Apply does all of these at once, unsaved; save with ctrl+s.",
    );
    let found = model.map.tour(tour);
    let room = Count::new(columns.get().saturating_sub(ROW_CELLS.get() + 24));
    for change in changes {
        let (sign, color, text, warning) = match change {
            EditChange::Renamed(name) => ("~", ACCENT, format!("rename to {name}"), None),
            EditChange::Kind(kind) => (
                "~",
                ACCENT,
                format!(
                    "kind {} -> {}",
                    found.map_or_else(String::new, |old| Tag::kind(old.kind()).to_string()),
                    Tag::kind(*kind)
                ),
                None,
            ),
            EditChange::Group(group) => (
                "~",
                ACCENT,
                format!(
                    "group {} -> {}",
                    found
                        .and_then(Tour::group)
                        .map_or("(none)", GroupName::as_str),
                    group.as_ref().map_or("(none)", GroupName::as_str)
                ),
                None,
            ),
            EditChange::TourNote(note) => (
                "~",
                ACCENT,
                format!("tour note {}", clip(note.as_ref(), room).as_str()),
                None,
            ),
            EditChange::StepNote(note) => (
                "~",
                ACCENT,
                format!(
                    "note of {}: {}",
                    step_named(wizard, model, &note.step).as_str(),
                    clip(note.note.as_ref(), room).as_str()
                ),
                None,
            ),
            EditChange::Added { symbol, under } => (
                "+",
                GREEN,
                format!(
                    "add {} under {}",
                    name_of(model, *symbol).as_str(),
                    match under {
                        AddedUnder::Step(step) => step_named(wizard, model, step),
                        AddedUnder::Added(parent) =>
                            Label::new(format!("{} (new)", name_of(model, *parent).as_str())),
                    }
                    .as_str()
                ),
                None,
            ),
            EditChange::Removed { step, lost } => (
                "-",
                RED,
                format!("remove {}", step_named(wizard, model, step).as_str()),
                lost.as_ref()
                    .map(|note| format!("  its note is lost: {}", clip(Some(note), room).as_str())),
            ),
        };
        let mut runs = vec![
            Run::new(format!("{:>width$} ", sign, width = 2), color),
            Run::new(text, TEXT),
        ];
        if let Some(warning) = warning {
            runs.push(Run::new(warning, RED));
        }
        frame.row_text(runs);
    }
}
