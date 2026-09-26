mod code;

use platform::Cursor;
use ui::{
    Align, Canvas, Count, Draw, FontSize, Id, Interaction, Kind, Label, Layout, Point, Px, Rect,
    Run, Scrollbar, Sides, Size, Style, Text, Ui, Wrap,
};

use crate::action::Action;
use crate::dock::Edge;
use crate::field::{Attention, Field, Looks, Which};
use crate::grid::Grids;
use crate::ids::{Control, Target};
use crate::model::{LeftTab, Metrics};
use crate::peek::Tip;
use crate::status::Status;
use crate::theme::{
    self, ACCENT, BACKGROUND, BAR_PADDING, BORDER, BUTTON_PADDING, Cells, DOCUMENT_PADDING,
    DROP_BAND, FIELD, FIELD_CARET_ROOM, FIELD_PADDING, GAP, HOVER, INDENT_EXTRA, LABEL_PADDING,
    NAV_BUTTON, NAV_BUTTON_EXTRA, PANEL, PANEL_PADDING, ROW_PADDING, SELECTED, SMALL_BUTTON_EXTRA,
    SMALL_BUTTON_PADDING, SMALL_GAP, STATUS_GAP, STEP_SPACER, TEXT, TIGHT_GAP, TOOLTIP_PADDING,
    WEAK, WIDE_GAP,
};

use crate::field::Fields;
use crate::ids;
pub(crate) use code::{CodeBlock, Marks, Width};

#[derive(Clone, Debug)]
pub(crate) struct TipAt {
    pub(crate) tip: Tip,
    pub(crate) at: Point,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Overlay {
    pub(crate) focus: Option<Which>,
    pub(crate) left: Option<LeftTab>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Enabled {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Container {
    Window,
    Body,
    DockRow,
    Center,
    TopBar,
    StatusBar,
    Toolbar,
    ToolbarTight,
    ToolbarSmall,
    Header,
    Breadcrumb,
    PanelColumn,
    OutputColumn,
    Stack,
    StepRow { selected: Chosen },
    CodeColumn { selected: Chosen },
    FillRow,
    Tooltip { at: Point },
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

impl Container {
    fn shape(self) -> Shape {
        let toolbar = |gap: Px| {
            Layout::row()
                .grow_width()
                .padding(BAR_PADDING)
                .gap(gap)
                .cross(Align::Center)
        };
        let marked = |selected: Chosen| Style {
            background: None,
            border: if selected == Chosen::Chosen {
                Sides::LEFT
            } else {
                Sides::NONE
            },
            border_color: ACCENT,
        };
        match self {
            Self::Window => {
                Shape::new(Layout::column().grow(), Style::background(BACKGROUND), None)
            }
            Self::Body => Shape::new(Layout::column().grow(), Style::NONE, Some(ids::body())),
            Self::DockRow => Shape::new(Layout::row().grow(), Style::NONE, Some(ids::dock_row())),
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
            Self::OutputColumn => {
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
            Self::FillRow => Shape::new(Layout::row().grow_width(), Style::NONE, None),
            Self::Tooltip { at } => Shape::new(
                Layout::column()
                    .floating(at)
                    .padding(TOOLTIP_PADDING)
                    .gap(TIGHT_GAP),
                Style::background(PANEL).border(Sides::ALL, BORDER),
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
            Padding::Path => PANEL_PADDING,
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
            text_kind(runs, size, Wrap::None),
            Layout::row().grow_width().padding(ROW_PADDING),
            Style {
                background,
                ..Style::NONE
            },
            Some(id),
        )
    }

    pub(crate) fn link_text(
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
        let shown = field.shown(attention);
        let color = match shown.looks {
            Looks::Hint => return vec![Run::new(hint.clone(), WEAK)],
            Looks::Marked => ACCENT,
            Looks::Plain => TEXT,
        };
        let caret = if attention == Attention::Focused {
            "\u{258f}"
        } else {
            ""
        };
        vec![
            Run::new(shown.before, color),
            Run::new(caret, ACCENT),
            Run::new(shown.after, color),
        ]
    }

    pub(crate) fn field(&mut self, fields: &Fields, which: Which, hint: &Label, width: Cells) {
        let attention = self.focused(fields, which);
        let field = fields.get(which);
        let width = width.of(self.cell_width());
        let caret = Px::of_count(field.caret().get() + 1);
        let across = (Px::new(caret.get() * self.cell_width().get()) - (width - FIELD_CARET_ROOM))
            .max(Px::ZERO);
        let runs = Self::field_runs(field, attention, hint);
        let border = if attention == Attention::Focused {
            ACCENT
        } else {
            BORDER
        };
        let clicked = self
            .ui
            .open(
                Kind::None,
                Layout::row()
                    .width(width)
                    .padding(FIELD_PADDING)
                    .scroll(Point::new(across, Px::ZERO)),
                Style::background(FIELD).border(Sides::ALL, border),
                Some(which.control().id()),
            )
            .clicked();
        let size = self.metrics.font;
        self.ui.leaf(
            text_kind(runs, size, Wrap::None),
            Layout::row(),
            Style::NONE,
            None,
        );
        self.ui.close();
        if clicked {
            self.take_focus(which);
        }
    }

    pub(crate) fn take_focus(&mut self, which: Which) {
        self.overlay.focus = Some(which);
        self.push(Action::FocusField(which));
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
        let mut offset = start;
        let offset = self.ui.scroll_by_wheel(id, &mut offset);
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

    pub(crate) fn docked(&mut self, id: Id, edge: Edge, size: Px) {
        let layout = if edge == Edge::Bottom {
            Layout::column().grow_width().height(size)
        } else {
            Layout::row().width(size).grow_height()
        };
        self.ui.open(
            Kind::None,
            layout.scroll(Point::default()),
            Style::NONE,
            Some(id),
        );
    }

    pub(crate) fn splitter(&mut self, target: Target, edge: Edge, width: Px) {
        let id = target.id();
        let interaction = self.ui.interaction(id);
        let (layout, sides) = match edge {
            Edge::Left => (Layout::column().width(width).grow_height(), Sides::LEFT),
            Edge::Right => (Layout::column().width(width).grow_height(), Sides::RIGHT),
            Edge::Bottom => (Layout::row().grow_width().height(width), Sides::BOTTOM),
        };
        let style = if interaction.hovered() || interaction.down() {
            Style::background(ACCENT)
        } else {
            Style::background(BACKGROUND).border(sides, BORDER)
        };
        self.ui.leaf(Kind::None, layout, style, Some(id));
    }

    pub(crate) fn grip(&mut self, target: Target, lit: Chosen) {
        let id = target.id();
        self.ui.open(
            Kind::None,
            Layout::row()
                .grow_width()
                .padding(BAR_PADDING)
                .gap(SMALL_GAP)
                .cross(Align::Center),
            Style::background(if lit == Chosen::Chosen { HOVER } else { PANEL })
                .border(Sides::BOTTOM, BORDER),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Padding {
    Path,
    Step,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fill {
    Fit,
    Grow,
}
