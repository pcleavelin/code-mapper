use domain::{Line, LineCount, SymbolId};
use ui::{Button, Count, Extent, Label, Point, Px, Run};

use crate::action::Action;
use crate::element_tip::{AttachedTip, Pressing, beside};
use crate::grid::GUTTER;
use crate::ids;
use crate::model::Model;
use crate::peek::Tip;
use crate::theme::{
    ACCENT, Cells, ELEMENT_TIP_WIDTH, PIXEL, TEXT, TOOLTIP_FRAME, TOOLTIP_LEAST, TOOLTIP_MARGIN,
    TOOLTIP_OFFSET, TOOLTIP_ROW_GAP, TOOLTIP_RULE, TOOLTIP_SYMBOL_WIDTH, TOOLTIP_TEXT_WIDTH, WEAK,
};
use crate::widgets::{CodeBlock, Container, Fill, Frame, Marks, TipAt, Width};

const TEXT_LINES: Count = Count::new(24);
const SYMBOL_LINES: LineCount = LineCount::new(23);

fn place_at(frame: &Frame<'_>, at: Point, columns: Cells, rows: Cells) -> Point {
    let window = frame.ui.size();
    let cell = frame.metrics.cell;
    let width = columns.of(cell.width) + TOOLTIP_FRAME;
    let height = rows.of(cell.height + TOOLTIP_ROW_GAP) + TOOLTIP_FRAME;
    let across = if at.horizontal + TOOLTIP_OFFSET + width <= window.width {
        at.horizontal + TOOLTIP_OFFSET
    } else {
        (at.horizontal - TOOLTIP_OFFSET - width).max(Px::ZERO)
    };
    let down = if at.vertical + TOOLTIP_OFFSET + height <= window.height {
        at.vertical + TOOLTIP_OFFSET
    } else {
        (at.vertical - TOOLTIP_OFFSET - height).max(Px::ZERO)
    };
    Point::new(across, down)
}

pub(super) fn tooltip(model: &Model, frame: &mut Frame<'_>) {
    let attached = frame.attached_tip.take();
    let pressing = Pressing::of(frame.ui.pointer());
    frame.push(Action::Rest(
        attached.as_ref().map(|attached| attached.id),
        pressing,
    ));
    let shown = show(model, frame).or_else(|| {
        attached
            .filter(|attached| {
                pressing == Pressing::Released && model.resting.due(attached.id, model.now)
            })
            .map(|attached| element_tip(frame, &attached))
    });
    frame.push(Action::TipShown(shown));
}

fn element_tip(frame: &mut Frame<'_>, attached: &AttachedTip) -> Label {
    let cell = frame.metrics.cell;
    let lines = attached.tip.lines(ELEMENT_TIP_WIDTH);
    let chord = attached
        .tip
        .chord()
        .map(|chord| format!("  {}", chord.as_str()));
    let last = lines.len().saturating_sub(1);
    let widest = lines
        .iter()
        .enumerate()
        .map(|(row, line)| {
            let chord_room = chord
                .as_ref()
                .filter(|_| row == last)
                .map_or(0, |chord| chord.chars().count());
            line.as_str().chars().count() + chord_room
        })
        .max()
        .unwrap_or(0);
    let size = Extent::new(
        Cells::of_count(widest).of(cell.width) + TOOLTIP_FRAME,
        Cells::of_count(lines.len()).of(cell.height + TOOLTIP_ROW_GAP) + TOOLTIP_FRAME,
    );
    let at = beside(frame.ui.size(), attached.rect, size);
    frame.start(Container::ElementTip { at });
    for (row, line) in lines.into_iter().enumerate() {
        let mut runs = vec![Run::new(line, TEXT)];
        if let Some(chord) = chord.clone().filter(|_| row == last) {
            runs.push(Run::new(chord, ACCENT));
        }
        frame.text_runs(runs, Fill::Fit);
    }
    frame.finish();
    attached.tip.shown()
}

fn show(model: &Model, frame: &mut Frame<'_>) -> Option<Label> {
    let TipAt { tip, at } = frame.tooltip.take()?;
    if frame.ui.pointer().down.contains(Button::Left) {
        return None;
    }
    let window = frame.ui.size();
    let cell = frame.metrics.cell;
    let most = usize::try_from(
        ((window.width - TOOLTIP_MARGIN).ratio(cell.width.max(PIXEL))).max(TOOLTIP_LEAST.get()),
    )
    .unwrap_or(0);
    match tip {
        Tip::Symbol(symbol) => symbol_tip(model, frame, at, symbol),
        Tip::Text(text) => text_tip(frame, at, &text, Count::new(most)),
    }
}

fn symbol_tip(model: &Model, frame: &mut Frame<'_>, at: Point, symbol: SymbolId) -> Option<Label> {
    {
        {
            let index = &model.index;
            let found = index.symbol(symbol)?;
            let source = index.file(symbol.file())?;
            let span = found.span();
            let name = found.name().as_str().to_owned();
            let place = format!(
                "{} {}:{}-{}",
                found.kind(),
                source.path(),
                span.start().number(),
                span.end().number()
            );
            let last_line = source.text().count().last().unwrap_or(Line::new(0));
            let last = span
                .end()
                .min(last_line)
                .min(Line::new(span.start().value() + SYMBOL_LINES.value()));
            let widest = (span.start().value()..=last.value())
                .filter_map(|line| source.text().line(Line::new(line)))
                .map(|line| line.as_str().chars().count() + GUTTER.get())
                .max()
                .unwrap_or(0)
                .max(place.len())
                .max(usize::try_from(TOOLTIP_SYMBOL_WIDTH.get()).unwrap_or(0));
            let rows =
                i32::try_from(last.value().saturating_sub(span.start().value())).unwrap_or(0) + 5;
            let spot = place_at(frame, at, Cells::of_count(widest), Cells::new(rows));
            frame.start(Container::Tooltip { at: spot });
            frame.label(name.clone(), TEXT);
            frame.label(place, WEAK);
            let none = |_: Line| None;
            frame.code_block(
                model,
                &CodeBlock {
                    file: symbol.file(),
                    start: span.start(),
                    end: last,
                    id: ids::tooltip_code(),
                    width: Width::Fit,
                    marks: Marks {
                        background: &none,
                        bar: &none,
                    },
                },
            );
            if last < span.end() {
                frame.label(
                    format!(
                        "      \u{2026} {} more lines",
                        span.end().value() - last.value()
                    ),
                    WEAK,
                );
            }
            frame.label(
                "alt-click: keep in the peek panel   ctrl-click or double-click: go there",
                WEAK,
            );
            frame.finish();
            Some(Label::new(name))
        }
    }
}

fn text_tip(frame: &mut Frame<'_>, at: Point, text: &Label, room: Count) -> Option<Label> {
    let most = room.get();
    {
        {
            let lines: Vec<&str> = text.as_str().lines().collect();
            let widest = lines
                .iter()
                .take(TEXT_LINES.get())
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0)
                .min(most)
                .max(usize::try_from(TOOLTIP_TEXT_WIDTH.get()).unwrap_or(0));
            let rows = Cells::of_count(lines.len().min(TEXT_LINES.get()) + 2);
            let spot = place_at(frame, at, Cells::of_count(widest), rows);
            frame.start(Container::Tooltip { at: spot });
            for line in lines.iter().take(TEXT_LINES.get()) {
                let line: String = line.chars().take(most).collect();
                let rule = line.starts_with(Marker::Rule.name().as_str());
                let run = if rule {
                    Run::new(
                        "\u{2500}".repeat(usize::try_from(TOOLTIP_RULE.get()).unwrap_or(0)),
                        WEAK,
                    )
                } else {
                    Run::new(line, TEXT)
                };
                frame.text_runs(vec![run], Fill::Fit);
            }
            if lines.len() > TEXT_LINES.get() {
                frame.label(
                    format!("\u{2026} {} more lines", lines.len() - TEXT_LINES.get()),
                    WEAK,
                );
            }
            frame.label(
                "alt-click: keep the definition in the peek panel   ctrl-click or double-click: go to it",
                WEAK,
            );
            frame.finish();
            lines.first().map(|line| Label::new(*line))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Literal(&'static str);

impl Literal {
    const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    Rule,
}

impl Marker {
    const fn name(self) -> Literal {
        Literal(match self {
            Self::Rule => "---",
        })
    }
}
