mod code;

use platform::Cursor;
use ui::{
    Align, Button, Canvas, Count, Draw, FontSize, Icon, Id, Interaction, Kind, Label, Layout,
    Point, Px, Rect, Run, Scrollbar, Sides, Size, Style, Text, Ui, Wrap,
};

use crate::action::Action;
use crate::field::{
    Attention, Field, FieldAct, FieldShape, FieldWindow, Piece, Pointing, Tint, Which,
};
use crate::grid::Grids;
use crate::ids::{Control, Target};
use crate::keys::Walk;
use crate::model::Metrics;
use crate::panels::Direction;
use crate::peek::Tip;
use crate::status::Status;
use crate::theme::{
    self, ACCENT, BACKGROUND, BAR_PADDING, BORDER, BUTTON_PADDING, CHECKBOX_COLUMNS, Cells,
    DANGER_HOVER, DOCUMENT_PADDING, DROP_BAND, FAINT, FIELD, FIELD_CARET_ROOM, FIELD_PADDING, GAP,
    HOVER, HYPERLINK, INDENT_EXTRA, LABEL_PADDING, NAV_BUTTON, NAV_BUTTON_EXTRA, NOTE_ROWS_LEAST,
    NOTE_ROWS_MOST, PALETTE_TAG, PANEL, PANEL_PADDING, RED, ROW_PADDING, SELECTED,
    SMALL_BUTTON_EXTRA, SMALL_BUTTON_PADDING, SMALL_GAP, STATUS_GAP, STEP_SPACER, TAB_PADDING,
    TAB_STRIP, TEXT, TIGHT_GAP, TOOLTIP_PADDING, WEAK, WIDE_GAP,
};

use crate::field::Fields;
use crate::ids;
use crate::wizard::Tick;
pub(crate) use code::{CodeBlock, Coded, Marks, Width};

#[derive(Clone, Copy, Debug)]
struct Spot {
    across: Px,
    first: Count,
    attention: Attention,
}

#[derive(Clone, Debug)]
pub(crate) struct TipAt {
    pub(crate) tip: Tip,
    pub(crate) at: Point,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Overlay {
    pub(crate) focus: Option<Which>,
    pub(crate) status: Option<Status>,
    pub(crate) asked: Count,
}

pub(crate) struct Frame<'frame> {
    pub(crate) ui: &'frame mut Ui,
    pub(crate) queue: &'frame mut Vec<Action>,
    pub(crate) grids: &'frame mut Grids,
    pub(crate) metrics: Metrics,
    pub(crate) overlay: Overlay,
    pub(crate) tooltip: Option<TipAt>,
    pub(crate) cursor: Cursor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chosen {
    Chosen,
    Plain,
}

impl Chosen {
    pub(crate) fn of<Value: PartialEq>(value: &Value, current: &Value) -> Self {
        if value == current {
            Self::Chosen
        } else {
            Self::Plain
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Enabled {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Container {
    Window,
    Body,
    Center,
    TopBar,
    StatusBar,
    Toolbar,
    TourHeader,
    TourButtons,
    ToolbarTight,
    ToolbarSmall,
    Header,
    Breadcrumb,
    PanelColumn,
    ConsoleColumn,
    Stack,
    StepRow { selected: Chosen },
    CodeColumn { selected: Chosen },
    StepColumn(Id),
    FillRow,
    Tooltip { at: Point },
    Picker { at: Point, width: Px },
    Palette { at: Point, width: Px },
    Settings { at: Point, width: Px },
    PanelHeader,
    Centered,
    StartPage { width: Px },
}

struct Shape {
    layout: Layout,
    style: Style,
    id: Option<Id>,
}

impl Shape {
    const fn new(layout: Layout, style: Style, id: Option<Id>) -> Self {
        Self { layout, style, id }
    }
}

fn toolbar(gap: Px) -> Layout {
    Layout::row()
        .grow_width()
        .padding(BAR_PADDING)
        .gap(gap)
        .cross(Align::Center)
}

fn marked(selected: Chosen) -> Style {
    Style {
        background: None,
        border: if selected == Chosen::Chosen {
            Sides::LEFT
        } else {
            Sides::NONE
        },
        border_color: ACCENT,
    }
}

impl Container {
    fn page_shape(self) -> Shape {
        let layout = match self {
            Self::StartPage { width } => Layout::column().width(width),
            _ => Layout::column().grow_width().cross(Align::Center),
        };
        Shape::new(layout, Style::NONE, None)
    }

    fn shape(self) -> Shape {
        match self {
            Self::Window => {
                Shape::new(Layout::column().grow(), Style::background(BACKGROUND), None)
            }
            Self::Body => Shape::new(Layout::column().grow(), Style::NONE, Some(ids::body())),
            Self::Center => Shape::new(Layout::column().grow(), Style::NONE, None),
            Self::TopBar => Shape::new(
                toolbar(GAP),
                Style::background(PANEL).border(Sides::BOTTOM, BORDER),
                None,
            ),
            Self::StatusBar => Shape::new(
                toolbar(STATUS_GAP),
                Style::background(PANEL).border(Sides::TOP, BORDER),
                None,
            ),
            Self::Toolbar => Shape::new(toolbar(WIDE_GAP), Style::NONE, None),
            Self::TourHeader => {
                Shape::new(toolbar(WIDE_GAP), Style::NONE, Some(ids::tour_header()))
            }
            Self::TourButtons => {
                Shape::new(toolbar(WIDE_GAP), Style::NONE, Some(ids::tour_buttons()))
            }
            Self::ToolbarTight => Shape::new(toolbar(GAP), Style::NONE, None),
            Self::ToolbarSmall => Shape::new(toolbar(SMALL_GAP), Style::NONE, None),
            Self::Header => Shape::new(
                Layout::row().grow_width().padding(PANEL_PADDING),
                Style::NONE,
                None,
            ),
            Self::Breadcrumb => Shape::new(
                toolbar(GAP),
                Style::background(PANEL).border(Sides::TOP.with(Sides::BOTTOM), BORDER),
                None,
            ),
            Self::PanelColumn => {
                Shape::new(Layout::column().grow(), Style::background(PANEL), None)
            }
            Self::ConsoleColumn => {
                Shape::new(Layout::column().grow(), Style::background(FIELD), None)
            }
            Self::Stack => Shape::new(
                Layout::column().grow_width().padding(PANEL_PADDING),
                Style::NONE,
                None,
            ),
            Self::StepRow { selected } => Shape::new(
                Layout::row().grow_width().gap(GAP).cross(Align::Center),
                marked(selected),
                None,
            ),
            Self::CodeColumn { selected } => {
                Shape::new(Layout::column().grow_width(), marked(selected), None)
            }
            Self::StepColumn(id) => Shape::new(
                Layout::column().grow_width().gap(TIGHT_GAP),
                Style::NONE,
                Some(id),
            ),
            Self::FillRow => Shape::new(Layout::row().grow_width(), Style::NONE, None),
            Self::Tooltip { at } => Shape::new(
                Layout::column()
                    .floating(at)
                    .padding(TOOLTIP_PADDING)
                    .gap(TIGHT_GAP),
                Style::background(PANEL).border(Sides::ALL, BORDER),
                None,
            ),
            Self::Picker { at, width }
            | Self::Palette { at, width }
            | Self::Settings { at, width } => Shape::new(
                Layout::column()
                    .floating(at)
                    .width(width)
                    .padding(TOOLTIP_PADDING)
                    .gap(TIGHT_GAP),
                Style::background(PANEL).border(Sides::ALL, ACCENT),
                Some(match self {
                    Self::Picker { .. } => ids::picker(),
                    Self::Settings { .. } => ids::settings_box(),
                    _ => ids::palette_box(),
                }),
            ),
            Self::Centered | Self::StartPage { .. } => self.page_shape(),
            Self::PanelHeader => Shape::new(
                Layout::row().grow_width().cross(Align::Center),
                Style::background(TAB_STRIP).border(Sides::BOTTOM, BORDER),
                None,
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scroller {
    Plain,
    Document,
    Sized { height: Px },
}

pub(crate) struct Scrolled {
    pub(crate) interaction: Interaction,
    pub(crate) offset: Px,
}

pub(crate) struct Window {
    pub(crate) first: Count,
    pub(crate) visible: Count,
}

fn text_kind(runs: Vec<Run>, size: FontSize, wrap: Wrap) -> Kind {
    Kind::Text(Text { runs, size, wrap })
}

impl Frame<'_> {
    pub(crate) const fn cell_width(&self) -> Px {
        self.metrics.cell_width()
    }

    pub(crate) const fn row_height(&self) -> Px {
        self.metrics.row_height()
    }

    pub(crate) fn push(&mut self, action: Action) {
        self.queue.push(action);
    }

    pub(crate) fn start(&mut self, container: Container) -> Interaction {
        let shape = container.shape();
        self.ui
            .open(Kind::None, shape.layout, shape.style, shape.id)
    }

    pub(crate) fn finish(&mut self) {
        self.ui.close();
    }

    pub(crate) fn label(&mut self, text: impl Into<Label>, color: ui::Color) {
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(vec![Run::new(text, color)], size, Wrap::None),
            Layout::row().padding(LABEL_PADDING),
            Style::NONE,
            None,
        );
    }

    pub(crate) fn caption(&mut self, runs: Vec<Run>) {
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(runs, size, Wrap::Clip),
            Layout::row().grow_width().padding(LABEL_PADDING),
            Style::NONE,
            None,
        );
    }

    pub(crate) fn row_text(&mut self, runs: Vec<Run>) {
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(runs, size, Wrap::Clip),
            Layout::row().grow_width().padding(ROW_PADDING),
            Style::NONE,
            None,
        );
    }

    pub(crate) fn plain_line(&mut self, text: impl Into<Label>, color: ui::Color) {
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(vec![Run::new(text, color)], size, Wrap::None),
            Layout::row(),
            Style::NONE,
            None,
        );
    }

    pub(crate) fn title(&mut self, text: impl Into<Label>) {
        let size = theme::title_font(self.metrics.font);
        self.ui.leaf(
            text_kind(vec![Run::new(text, TEXT)], size, Wrap::None),
            Layout::row(),
            Style::NONE,
            None,
        );
    }

    pub(crate) fn note(&mut self, text: impl Into<Label>, color: ui::Color, padding: Padding) {
        let size = self.metrics.font;
        let padding = match padding {
            Padding::Tour => PANEL_PADDING,
            Padding::Step => LABEL_PADDING,
        };
        self.ui.leaf(
            text_kind(vec![Run::new(text, color)], size, Wrap::Words),
            Layout::row().grow_width().padding(padding),
            Style::NONE,
            None,
        );
    }

    pub(crate) fn grow(&mut self) {
        self.ui
            .leaf(Kind::None, Layout::row().grow_width(), Style::NONE, None);
    }

    pub(crate) fn spacer(&mut self, height: Px) {
        self.ui
            .leaf(Kind::None, Layout::row().height(height), Style::NONE, None);
    }

    pub(crate) fn reserve(&mut self, id: Id, height: Px) {
        self.ui.leaf(
            Kind::None,
            Layout::row().grow_width().height(height),
            Style::NONE,
            Some(id),
        );
    }

    pub(crate) fn step_gap(&mut self) {
        self.spacer(STEP_SPACER);
    }

    pub(crate) fn indent(&mut self, width: Px) {
        self.ui.leaf(
            Kind::None,
            Layout::row().width(width + INDENT_EXTRA),
            Style::NONE,
            None,
        );
    }

    pub(crate) fn cells_gap(&mut self, cells: Cells) {
        let width = cells.of(self.cell_width());
        self.ui
            .leaf(Kind::None, Layout::row().width(width), Style::NONE, None);
    }

    pub(crate) fn button(
        &mut self,
        text: impl Into<Label>,
        target: Target,
        chosen: Chosen,
    ) -> Interaction {
        let id = target.id();
        let hovered = self.ui.interaction(id).hovered();
        let selected = chosen == Chosen::Chosen;
        let background = if selected {
            SELECTED
        } else if hovered {
            HOVER
        } else {
            PANEL
        };
        let color = if selected || hovered { TEXT } else { WEAK };
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(vec![Run::new(text, color)], size, Wrap::None),
            Layout::row().padding(BUTTON_PADDING),
            Style::background(background).border(Sides::ALL, BORDER),
            Some(id),
        )
    }

    pub(crate) fn control(&mut self, text: impl Into<Label>, control: Control) -> Interaction {
        self.button(text, control.target(), Chosen::Plain)
    }

    pub(crate) fn small_button(&mut self, text: impl Into<Label>, target: Target) -> Interaction {
        self.small_button_sized(text, None, target)
    }

    pub(crate) fn tab(
        &mut self,
        text: impl Into<Label>,
        target: Target,
        close: Target,
        chosen: Chosen,
    ) -> TabClicks {
        let id = target.id();
        let close_id = close.id();
        let close_hovered = self.ui.interaction(close_id).hovered();
        let hovered = self.ui.interaction(id).hovered() || close_hovered;
        let active = chosen == Chosen::Chosen;
        let background = if active {
            Some(PANEL)
        } else if hovered {
            Some(HOVER)
        } else {
            None
        };
        let size = self.metrics.font;
        let tab = self.ui.open(
            Kind::None,
            Layout::row()
                .padding(TAB_PADDING)
                .gap(SMALL_GAP)
                .cross(Align::Center),
            Style {
                background,
                border: if active { Sides::TOP } else { Sides::NONE },
                border_color: ACCENT,
            },
            Some(id),
        );
        self.ui.leaf(
            text_kind(
                vec![Run::new(text, if active || hovered { TEXT } else { WEAK })],
                size,
                Wrap::None,
            ),
            Layout::row(),
            Style::NONE,
            None,
        );
        let close_color = if close_hovered {
            TEXT
        } else if active || hovered {
            WEAK
        } else {
            FAINT
        };
        let closed = self.ui.leaf(
            text_kind(vec![Run::new(Icon::Close, close_color)], size, Wrap::None),
            Layout::row(),
            Style {
                background: close_hovered.then_some(HOVER),
                ..Style::NONE
            },
            Some(close_id),
        );
        self.ui.close();
        TabClicks { tab, close: closed }
    }

    pub(crate) fn danger_button(&mut self, text: impl Into<Label>, target: Target) -> Interaction {
        let id = target.id();
        let hovered = self.ui.interaction(id).when_aimed().hovered();
        let size = self.metrics.font;
        self.ui
            .leaf(
                text_kind(
                    vec![Run::new(text, if hovered { TEXT } else { WEAK })],
                    size,
                    Wrap::None,
                ),
                Layout::row().padding(SMALL_BUTTON_PADDING),
                Style::background(if hovered { DANGER_HOVER } else { FIELD })
                    .border(Sides::ALL, if hovered { RED } else { BORDER }),
                Some(id),
            )
            .when_aimed()
    }

    pub(crate) fn small_button_room(&mut self, cells: Cells) {
        let width = cells.of(self.cell_width()) + SMALL_BUTTON_EXTRA;
        self.ui
            .leaf(Kind::None, Layout::row().width(width), Style::NONE, None);
    }

    pub(crate) fn small_button_sized(
        &mut self,
        text: impl Into<Label>,
        cells: Option<Cells>,
        target: Target,
    ) -> Interaction {
        let id = target.id();
        let hovered = self.ui.interaction(id).hovered();
        let layout = match cells {
            Some(cells) => Layout::row()
                .padding(SMALL_BUTTON_PADDING)
                .width(cells.of(self.cell_width()) + SMALL_BUTTON_EXTRA),
            None => Layout::row().padding(SMALL_BUTTON_PADDING),
        };
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(
                vec![Run::new(text, if hovered { TEXT } else { WEAK })],
                size,
                Wrap::None,
            ),
            layout,
            Style::background(if hovered { HOVER } else { FIELD }).border(Sides::ALL, BORDER),
            Some(id),
        )
    }

    pub(crate) fn nav_button(
        &mut self,
        text: impl Into<Label>,
        target: Target,
        enabled: Enabled,
    ) -> Interaction {
        let id = target.id();
        let hovered = self.ui.interaction(id).hovered();
        let enabled = enabled == Enabled::Enabled;
        let color = if !enabled {
            theme::DISABLED
        } else if hovered {
            TEXT
        } else {
            WEAK
        };
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(vec![Run::new(text, color)], size, Wrap::None),
            Layout::row()
                .padding(BUTTON_PADDING)
                .width(NAV_BUTTON.of(self.cell_width()) + NAV_BUTTON_EXTRA),
            Style::background(if hovered && enabled { HOVER } else { PANEL })
                .border(Sides::ALL, BORDER),
            Some(id),
        )
    }

    pub(crate) fn tick_row(&mut self, target: Target) -> Interaction {
        let id = target.id();
        let hovered = self.ui.interaction(id).hovered();
        self.ui.open(
            Kind::None,
            Layout::row().grow_width().gap(GAP).cross(Align::Center),
            Style {
                background: hovered.then_some(HOVER),
                ..Style::NONE
            },
            Some(id),
        )
    }

    pub(crate) fn tick_box(&mut self, tick: Tick, cells: Cells, row: Interaction) {
        let hovered = row.hovered();
        let (mark, color, background, border) = match tick {
            Tick::Ticked => (Label::from(Icon::Check), BACKGROUND, ACCENT, ACCENT),
            Tick::Unticked => (
                Label::new(" ".repeat(CHECKBOX_COLUMNS.get())),
                TEXT,
                FIELD,
                if hovered { TEXT } else { WEAK },
            ),
        };
        let size = self.metrics.font;
        self.ui.open(
            Kind::None,
            Layout::row()
                .width(cells.of(self.cell_width()))
                .padding(ROW_PADDING)
                .cross(Align::Center),
            Style::NONE,
            None,
        );
        self.ui.leaf(
            text_kind(vec![Run::new(mark, color)], size, Wrap::None),
            Layout::row(),
            Style::background(background).border(Sides::ALL, border),
            None,
        );
        self.ui.close();
    }

    pub(crate) fn row(&mut self, runs: Vec<Run>, target: Target, chosen: Chosen) -> Interaction {
        self.marked_row(runs, target, (chosen == Chosen::Chosen).then_some(SELECTED))
    }

    pub(crate) fn marked_row(
        &mut self,
        runs: Vec<Run>,
        target: Target,
        marked: Option<ui::Color>,
    ) -> Interaction {
        let id = target.id();
        let hovered = self.ui.interaction(id).hovered();
        let background = marked.or(hovered.then_some(HOVER));
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(runs, size, Wrap::Clip),
            Layout::row().grow_width().padding(ROW_PADDING),
            Style {
                background,
                ..Style::NONE
            },
            Some(id),
        )
    }

    pub(crate) fn row_with_action(
        &mut self,
        runs: Vec<Run>,
        target: Target,
        marked: Option<ui::Color>,
        action: Option<RowAction>,
    ) -> RowClicks {
        let id = target.id();
        let action_hovered = action
            .as_ref()
            .is_some_and(|action| self.ui.interaction(action.target.id()).hovered());
        let hovered = self.ui.interaction(id).hovered() || action_hovered;
        let background = marked.or(hovered.then_some(HOVER));
        let size = self.metrics.font;
        let row = self.ui.open(
            Kind::None,
            Layout::row().grow_width(),
            Style {
                background,
                ..Style::NONE
            },
            Some(id),
        );
        self.ui.leaf(
            text_kind(runs, size, Wrap::Clip),
            Layout::row().grow_width().padding(ROW_PADDING),
            Style::NONE,
            None,
        );
        let shown = hovered || marked.is_some();
        let action = action.filter(|_| shown).map(|action| {
            self.ui.leaf(
                text_kind(
                    vec![Run::new(
                        action.label,
                        if action_hovered { ACCENT } else { WEAK },
                    )],
                    size,
                    Wrap::None,
                ),
                Layout::row().padding(ROW_PADDING),
                Style {
                    background: action_hovered.then_some(SELECTED),
                    ..Style::NONE
                },
                Some(action.target.id()),
            )
        });
        self.ui.close();
        RowClicks { row, action }
    }

    pub(crate) fn palette_row(
        &mut self,
        tag: Run,
        runs: Vec<Run>,
        chord: Option<Label>,
        target: Target,
        chosen: Chosen,
    ) -> Interaction {
        let id = target.id();
        let hovered = self.ui.interaction(id).hovered();
        let background = if chosen == Chosen::Chosen {
            Some(SELECTED)
        } else {
            hovered.then_some(HOVER)
        };
        let size = self.metrics.font;
        let row = self.ui.open(
            Kind::None,
            Layout::row().grow_width().cross(Align::Center),
            Style {
                background,
                ..Style::NONE
            },
            Some(id),
        );
        self.ui.leaf(
            text_kind(vec![tag], size, Wrap::Clip),
            Layout::row()
                .width(PALETTE_TAG.of(self.cell_width()))
                .padding(ROW_PADDING),
            Style::NONE,
            None,
        );
        self.ui.leaf(
            text_kind(runs, size, Wrap::Clip),
            Layout::row().grow_width().padding(ROW_PADDING),
            Style::NONE,
            None,
        );
        if let Some(chord) = chord {
            self.ui.leaf(
                text_kind(vec![Run::new(chord, ACCENT)], size, Wrap::None),
                Layout::row().padding(ROW_PADDING),
                Style::NONE,
                None,
            );
        }
        self.ui.close();
        row
    }

    pub(crate) fn header_text(
        &mut self,
        runs: Vec<Run>,
        target: Target,
        selected: Chosen,
    ) -> Interaction {
        let id = target.id();
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(runs, size, Wrap::None),
            Layout::row().padding(LABEL_PADDING),
            Style {
                background: (selected == Chosen::Chosen).then_some(SELECTED),
                ..Style::NONE
            },
            Some(id),
        )
    }

    pub(crate) fn hyperlink(&mut self, hyperlink: Hyperlink, target: Target) -> Interaction {
        let id = target.id();
        let hovered = self.ui.interaction(id).hovered();
        if hovered {
            self.cursor = Cursor::Pointer;
        }
        let size = self.metrics.font;
        let clicks = self.ui.open(
            Kind::None,
            Layout::row().padding(LABEL_PADDING),
            Style::NONE,
            Some(id),
        );
        if !hyperlink.lead.is_empty() {
            self.ui.leaf(
                text_kind(hyperlink.lead, size, Wrap::None),
                Layout::row(),
                Style::NONE,
                None,
            );
        }
        self.ui.leaf(
            text_kind(vec![Run::new(hyperlink.text, HYPERLINK)], size, Wrap::None),
            Layout::row(),
            Style {
                background: None,
                border: if hovered { Sides::BOTTOM } else { Sides::NONE },
                border_color: HYPERLINK,
            },
            None,
        );
        if !hyperlink.detail.is_empty() {
            self.ui.leaf(
                text_kind(hyperlink.detail, size, Wrap::None),
                Layout::row(),
                Style::NONE,
                None,
            );
        }
        self.ui.close();
        clicks
    }

    pub(crate) fn text_runs(&mut self, runs: Vec<Run>, layout: Fill) {
        let size = self.metrics.font;
        let layout = match layout {
            Fill::Fit => Layout::row(),
            Fill::Grow => Layout::row().grow_width(),
        };
        self.ui
            .leaf(text_kind(runs, size, Wrap::None), layout, Style::NONE, None);
    }

    pub(crate) fn focused(&self, fields: &Fields, which: Which) -> Attention {
        let focused = self.overlay.focus.or(fields.focused()) == Some(which);
        if focused {
            Attention::Focused
        } else {
            Attention::Idle
        }
    }

    pub(crate) fn field_runs(field: &Field, attention: Attention, hint: &Label) -> Vec<Run> {
        if field.text().is_empty() && attention == Attention::Idle {
            return vec![Run::new(hint.clone(), WEAK)];
        }
        field
            .pieces(field.whole(), attention)
            .into_iter()
            .map(|Piece { text, tint }| {
                Run::new(
                    text,
                    match tint {
                        Tint::Plain => TEXT,
                        Tint::Selected | Tint::Caret => ACCENT,
                    },
                )
            })
            .collect()
    }

    fn field_pieces(&mut self, pieces: Vec<Piece>) {
        let size = self.metrics.font;
        if pieces.is_empty() {
            self.ui.leaf(
                text_kind(vec![Run::new("", TEXT)], size, Wrap::None),
                Layout::row(),
                Style::NONE,
                None,
            );
        }
        for Piece { text, tint } in pieces {
            let (color, style) = match tint {
                Tint::Plain => (TEXT, Style::NONE),
                Tint::Selected => (TEXT, Style::background(SELECTED)),
                Tint::Caret => (ACCENT, Style::NONE),
            };
            self.ui.leaf(
                text_kind(vec![Run::new(text, color)], size, Wrap::None),
                Layout::row(),
                style,
                None,
            );
        }
    }

    fn field_pointer(&mut self, which: Which, field: &Field, interaction: Interaction, at: Spot) {
        let Some(rect) = interaction.rect() else {
            return;
        };
        let mods = self.ui.pointer().mods;
        let pointing = if interaction.double_clicked() {
            Pointing::Word
        } else if interaction.clicked() && mods.shift() {
            Pointing::Extend
        } else if interaction.clicked() {
            Pointing::Press
        } else if interaction.down() {
            Pointing::Drag
        } else {
            return;
        };
        let mouse = self.ui.pointer().mouse;
        let cell = self.cell_width().get().max(1);
        let row_height = self.row_height().get().max(1);
        let across = mouse.horizontal.get() - rect.left.get() - FIELD_PADDING.get()
            + at.across.get()
            + cell / 2;
        let down = mouse.vertical.get() - rect.top.get() - FIELD_PADDING.get();
        let column = Count::new(usize::try_from(across / cell).unwrap_or(0));
        let row = Count::new(at.first.get() + usize::try_from(down / row_height).unwrap_or(0));
        let caret = field.caret_at(which.shape(), row, column, at.attention);
        self.push(Action::Field(which, FieldAct::Point(caret, pointing)));
    }

    pub(crate) fn field(&mut self, fields: &Fields, which: Which, hint: &Label, width: Cells) {
        self.field_named(fields, which, which.id(), hint, width);
    }

    pub(crate) fn field_named(
        &mut self,
        fields: &Fields,
        which: Which,
        id: Id,
        hint: &Label,
        width: Cells,
    ) {
        if which.shape() == FieldShape::Multi {
            self.note_field(fields, which, id, hint, width);
            return;
        }
        let attention = self.focused(fields, which);
        let field = fields.get(which);
        let room = Count::new(usize::try_from(width.get().saturating_sub(1)).unwrap_or(0));
        let width = width.of(self.cell_width());
        let across = match attention {
            Attention::Focused => {
                let caret = Px::of_count(field.caret().get() + 1);
                (Px::new(caret.get() * self.cell_width().get()) - (width - FIELD_CARET_ROOM))
                    .max(Px::ZERO)
            }
            Attention::Idle => Px::ZERO,
        };
        let border = if attention == Attention::Focused {
            ACCENT
        } else {
            BORDER
        };
        let interaction = self.ui.open(
            Kind::None,
            Layout::row()
                .width(width)
                .padding(FIELD_PADDING)
                .scroll(Point::new(across, Px::ZERO)),
            Style::background(FIELD).border(Sides::ALL, border),
            Some(id),
        );
        let clicked = interaction.clicked();
        match attention {
            Attention::Idle if field.text().is_empty() => {
                let size = self.metrics.font;
                self.ui.leaf(
                    text_kind(vec![Run::new(hint.clone(), WEAK)], size, Wrap::None),
                    Layout::row(),
                    Style::NONE,
                    None,
                );
            }
            Attention::Idle => self.field_pieces(vec![Piece {
                text: field.idle_text(room),
                tint: Tint::Plain,
            }]),
            Attention::Focused => self.field_pieces(field.pieces(field.whole(), attention)),
        }
        self.ui.close();
        if clicked {
            self.take_focus_at(which, id);
        }
        self.field_pointer(
            which,
            field,
            interaction,
            Spot {
                across,
                first: Count::ZERO,
                attention,
            },
        );
    }

    fn note_field(&mut self, fields: &Fields, which: Which, id: Id, hint: &Label, width: Cells) {
        let attention = self.focused(fields, which);
        let field = fields.get(which);
        let shape = which.shape();
        let width = width.of(self.cell_width());
        let inner = width - FIELD_PADDING * 2;
        let window = FieldWindow {
            columns: Count::new(
                usize::try_from(inner.get() / self.cell_width().get().max(1)).unwrap_or(0),
            ),
            rows: NOTE_ROWS_MOST,
        };
        if field.window() != window {
            self.push(Action::Field(which, FieldAct::Fit(window)));
        }
        let rows = field.rows(shape);
        let shown = rows
            .len()
            .clamp(NOTE_ROWS_LEAST.get(), NOTE_ROWS_MOST.get());
        let first = field.first();
        let border = if attention == Attention::Focused {
            ACCENT
        } else {
            BORDER
        };
        let interaction = self.ui.open(
            Kind::None,
            Layout::column()
                .width(width)
                .height(self.row_height() * Count::new(shown) + FIELD_PADDING * 2)
                .padding(FIELD_PADDING)
                .scroll(Point::new(Px::ZERO, self.row_height() * first)),
            Style::background(FIELD).border(Sides::ALL, border),
            Some(id),
        );
        let clicked = interaction.clicked();
        if field.text().is_empty() && attention == Attention::Idle {
            let size = self.metrics.font;
            self.ui.leaf(
                text_kind(vec![Run::new(hint.clone(), WEAK)], size, Wrap::None),
                Layout::row(),
                Style::NONE,
                None,
            );
        } else {
            for row in rows {
                self.ui.open(Kind::None, Layout::row(), Style::NONE, None);
                self.field_pieces(field.pieces(row, attention));
                self.ui.close();
            }
        }
        self.ui.close();
        if clicked {
            self.take_focus_at(which, id);
        }
        let wheel = interaction.wheel().vertical.get();
        if wheel > 0.0 {
            self.push(Action::Field(which, FieldAct::Scroll(Walk::Up)));
        } else if wheel < 0.0 {
            self.push(Action::Field(which, FieldAct::Scroll(Walk::Down)));
        }
        self.field_pointer(
            which,
            field,
            interaction,
            Spot {
                across: Px::ZERO,
                first,
                attention,
            },
        );
    }

    pub(crate) fn take_focus(&mut self, which: Which) {
        self.take_focus_at(which, which.id());
    }

    fn take_focus_at(&mut self, which: Which, id: Id) {
        self.overlay.focus = Some(which);
        self.push(Action::FocusField(which, id));
    }

    pub(crate) fn take_focus_within(&mut self, popup: Interaction, which: Which) {
        let pointer = self.ui.pointer();
        if pointer.pressed.contains(Button::Left)
            && popup
                .rect()
                .is_some_and(|rect| rect.contains(pointer.mouse))
        {
            self.take_focus(which);
        }
    }

    pub(crate) fn command_row(&mut self, fields: &Fields) {
        let attention = self.focused(fields, Which::Command);
        let background = if attention == Attention::Focused {
            PANEL
        } else {
            FIELD
        };
        let clicked = self
            .ui
            .open(
                Kind::None,
                Layout::row()
                    .grow_width()
                    .padding(BAR_PADDING)
                    .gap(SMALL_GAP)
                    .cross(Align::Center),
                Style::background(background).border(Sides::TOP, BORDER),
                Some(Which::Command.control().id()),
            )
            .clicked();
        if clicked {
            self.take_focus(Which::Command);
        }
        self.text_runs(vec![Run::new(">", WEAK)], Fill::Fit);
        let runs = Self::field_runs(fields.get(Which::Command), attention, &Label::default());
        self.text_runs(runs, Fill::Grow);
        self.ui.close();
    }

    pub(crate) fn scroll_column(
        &mut self,
        id: Id,
        start: Px,
        scroller: Scroller,
        background: Option<ui::Color>,
    ) -> Scrolled {
        let mut offset = Point::new(Px::ZERO, start);
        let offset = self.ui.scroll_by_wheel(id, &mut offset).vertical;
        self.push(Action::Scroll(id, offset));
        let layout = match scroller {
            Scroller::Plain => Layout::column().grow().padding(PANEL_PADDING),
            Scroller::Document => Layout::column()
                .grow()
                .padding(DOCUMENT_PADDING)
                .gap(TIGHT_GAP),
            Scroller::Sized { height } => Layout::column()
                .grow_width()
                .height(height)
                .padding(PANEL_PADDING),
        };
        let style = background.map_or(Style::NONE, Style::background);
        let interaction = self.ui.open(
            Kind::None,
            layout.scroll(Point::new(Px::ZERO, offset)),
            style,
            Some(id),
        );
        Scrolled {
            interaction,
            offset,
        }
    }

    pub(crate) fn rows_window(
        &mut self,
        offset: Px,
        rect: Option<Rect>,
        count: Count,
        row_height: Px,
        guess: Count,
    ) -> Window {
        let visible = rect.map_or(guess, |rect| {
            Count::new(usize::try_from(rect.height.ratio(row_height) + 2).unwrap_or(0))
        });
        let first = Count::new(
            usize::try_from(offset.ratio(row_height).max(0))
                .unwrap_or(0)
                .min(count.get()),
        );
        self.spacer(Px::new(Px::of_count(first.get()).get() * row_height.get()));
        Window { first, visible }
    }

    pub(crate) fn rows_after(&mut self, count: Count, window: &Window, row_height: Px, extra: Px) {
        let rest = count
            .get()
            .saturating_sub(window.first.get() + window.visible.get());
        self.spacer(Px::new(Px::of_count(rest).get() * row_height.get()) + extra);
    }

    pub(crate) fn pane(&mut self, id: Id, direction: Option<Direction>, rect: Rect) {
        let layout = match direction {
            Some(Direction::Right) => Layout::row(),
            Some(Direction::Down) | None => Layout::column(),
        };
        self.ui.open(
            Kind::None,
            layout
                .width(rect.width)
                .height(rect.height)
                .scroll(Point::default()),
            Style::NONE,
            Some(id),
        );
    }

    pub(crate) fn divider(&mut self, target: Target, direction: Direction, rect: Rect) {
        let id = target.id();
        let interaction = self.ui.interaction(id);
        let sides = match direction {
            Direction::Right => Sides::LEFT,
            Direction::Down => Sides::TOP,
        };
        let style = if interaction.hovered() || interaction.down() {
            Style::background(ACCENT)
        } else {
            Style::background(BACKGROUND).border(sides, BORDER)
        };
        self.ui.leaf(
            Kind::None,
            Layout::row().width(rect.width).height(rect.height),
            style,
            Some(id),
        );
    }

    pub(crate) fn drop_band(&mut self, band: Rect) {
        self.ui.leaf(
            Kind::None,
            Layout::column()
                .floating(band.origin())
                .width(band.width)
                .height(band.height),
            Style::background(DROP_BAND).border(Sides::ALL, ACCENT),
            None,
        );
    }

    pub(crate) fn custom(
        &mut self,
        draw: impl FnOnce(&mut Canvas<'_>, Rect) + 'static,
        width: Size,
        height: Px,
        id: Option<Id>,
    ) -> Interaction {
        let layout = match width {
            Size::Exact(width) => Layout::row().width(width).height(height),
            Size::Grow | Size::Fit => Layout::row().grow_width().height(height),
        };
        let draw: Box<dyn Draw> = Box::new(draw);
        self.ui.leaf(Kind::Custom(draw), layout, Style::NONE, id)
    }

    pub(crate) fn canvas(
        &mut self,
        draw: impl FnOnce(&mut Canvas<'_>, Rect) + 'static,
        id: Id,
    ) -> Interaction {
        let draw: Box<dyn Draw> = Box::new(draw);
        self.ui.leaf(
            Kind::Custom(draw),
            Layout::column().grow(),
            Style::background(BACKGROUND),
            Some(id),
        )
    }

    pub(crate) const fn scrollbar_width() -> Px {
        Scrollbar::WIDTH
    }
}

pub(crate) struct RowAction {
    pub(crate) label: Label,
    pub(crate) target: Target,
}

impl RowAction {
    pub(crate) fn add_step(target: Target) -> Self {
        Self {
            label: Label::new("+ step"),
            target,
        }
    }
}

pub(crate) struct RowClicks {
    pub(crate) row: Interaction,
    pub(crate) action: Option<Interaction>,
}

impl RowClicks {
    pub(crate) fn acted(&self) -> bool {
        self.action.is_some_and(Interaction::clicked)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Padding {
    Tour,
    Step,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fill {
    Fit,
    Grow,
}

pub(crate) struct TabClicks {
    pub(crate) tab: Interaction,
    pub(crate) close: Interaction,
}

pub(crate) struct Hyperlink {
    pub(crate) lead: Vec<Run>,
    pub(crate) text: Label,
    pub(crate) detail: Vec<Run>,
}
