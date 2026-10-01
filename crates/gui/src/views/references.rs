use std::rc::Rc;

use domain::{Backend, FileId, Line, SourceFile, Symbol, SymbolId};
use ui::{Canvas, Color, Count, Extent, Grid, Icon, Label, Point, Px, Rect, Run, Size};

use crate::action::Action;
use crate::authoring::{Authoring, Hang};
use crate::ids;
use crate::model::Model;
use crate::peek::{Peek, Probe};
use crate::status::Status;
use crate::text::Clipped;
use crate::theme::{
    CLOSE_BUTTON, FIELD, ORANGE, PEEK_EXTRA, PEEK_LEAST, PEEK_PLACE_ROOM, PEEK_TITLE, PENDING,
    PIXEL, ROW_EXTRA, SLICE, TEXT, WEAK,
};
use crate::widgets::{Chosen, CodeBlock, Container, Frame, Marks, RowAction, Scroller, Width};

enum Body {
    Inside {
        file: domain::FileId,
        start: Line,
        end: Line,
        mark: Option<Line>,
    },
    Outside {
        first: Line,
        mark: Line,
        grid: Rc<Grid>,
    },
}

struct Peeked {
    title: Label,
    place: Label,
    go: Option<Action>,
    body: Body,
}

fn peeked(model: &Model) -> Option<Peeked> {
    let index = &model.index;
    match model.peek.as_ref()? {
        Peek::Symbol(symbol) => {
            let found = index.symbol(*symbol)?;
            let file = index.file(symbol.file())?;
            let span = found.span();
            Some(Peeked {
                title: Label::new(format!("Peek: {}", found.name())),
                place: Label::new(format!("{}:{}", file.path(), span.start().number())),
                go: Some(Action::Focus(*symbol)),
                body: Body::Inside {
                    file: symbol.file(),
                    start: span.start(),
                    end: span.end(),
                    mark: None,
                },
            })
        }
        Peek::Line { file, line } => {
            let source = index.file(*file)?;
            let last = source.text().count().last().unwrap_or(Line::new(0));
            Some(Peeked {
                title: Label::new("Peek"),
                place: Label::new(format!("{}:{}", source.path(), line.number())),
                go: Some(Action::GoTo(*file, *line)),
                body: Body::Inside {
                    file: *file,
                    start: Line::new(line.value().saturating_sub(6)),
                    end: Line::new(line.value() + 20).min(last),
                    mark: Some(*line),
                },
            })
        }
        Peek::Outside {
            file,
            line,
            first,
            grid,
        } => Some(Peeked {
            title: Label::new("Defined outside this repo"),
            place: Label::new(format!(
                "{}:{}",
                file.display().to_string().replace('\\', "/"),
                line.number()
            )),
            go: None,
            body: Body::Outside {
                first: *first,
                mark: *line,
                grid: Rc::clone(grid),
            },
        }),
    }
}

fn peek_section(model: &Model, frame: &mut Frame<'_>, area: Extent) {
    let Some(Peeked {
        title,
        place,
        go,
        body,
    }) = peeked(model)
    else {
        return;
    };
    let columns = usize::try_from(area.width.ratio(frame.cell_width().max(PIXEL))).unwrap_or(0);
    let title_room = usize::try_from(PEEK_TITLE.get())
        .unwrap_or(0)
        .min(columns / 2);
    let title = Label::new(Clipped::right(title.as_str(), title_room).to_string());
    let place_room = columns
        .saturating_sub(title.columns() + usize::try_from(PEEK_PLACE_ROOM.get()).unwrap_or(0));
    let place = Label::new(Clipped::left(place.as_str(), place_room).to_string());
    peek_header(frame, title, place, go);
    peek_body(model, frame, body, area);
}

fn peek_header(frame: &mut Frame<'_>, title: Label, place: Label, go: Option<Action>) {
    frame.start(Container::ToolbarTight);
    frame.label(title, TEXT);
    frame.label(place, WEAK);
    frame.grow();
    if let Some(go) = go
        && frame.small_button("go", ids::PEEK_GO.target()).clicked()
    {
        frame.push(go);
    }
    if frame
        .small_button_sized(Icon::Close, Some(CLOSE_BUTTON), ids::PEEK_CLOSE.target())
        .clicked()
    {
        frame.push(Action::ClosePeek);
    }
    frame.finish();
}

fn peek_body(model: &Model, frame: &mut Frame<'_>, body: Body, area: Extent) {
    let rows = match &body {
        Body::Inside {
            file, start, end, ..
        } => {
            let last = model
                .index
                .file(*file)
                .and_then(|source| source.text().count().last())
                .unwrap_or(Line::new(0));
            i32::try_from(end.min(&last).value().saturating_sub(start.value())).unwrap_or(0) + 1
        }
        Body::Outside { grid, .. } => i32::try_from(grid.rows().get()).unwrap_or(0),
    };
    let row_height = frame.row_height();
    let height = (Px::new(rows * row_height.get()) + PEEK_EXTRA)
        .min((frame.ui.size().height / 3).max(PEEK_LEAST))
        .min(area.height / 2);
    frame.scroll_column(
        ids::peek(),
        model.scrolls.get(ids::peek()),
        Scroller::Sized { height },
        Some(FIELD),
    );
    match body {
        Body::Inside {
            file,
            start,
            end,
            mark,
        } => {
            let background = |line: Line| (mark == Some(line)).then_some(SLICE);
            let bar = |_: Line| None;
            frame.code_block(
                model,
                &CodeBlock {
                    file,
                    start,
                    end,
                    id: ids::peek_code(),
                    width: Width::Wide,
                    marks: Marks {
                        background: &background,
                        bar: &bar,
                    },
                },
            );
        }
        Body::Outside { first, mark, grid } => {
            let size = frame.metrics.font;
            let count = u32::try_from(grid.rows().get()).unwrap_or(0);
            let drawn = Rc::clone(&grid);
            let draw = move |canvas: &mut Canvas<'_>, rect: Rect| {
                if mark >= first && mark.value() < first.value() + count {
                    let offset = i32::try_from(mark.value() - first.value()).unwrap_or(0);
                    canvas.rect(
                        Rect::new(
                            rect.left,
                            rect.top + Px::new(offset * row_height.get()),
                            rect.width,
                            row_height,
                        ),
                        SLICE,
                    );
                }
                canvas.grid(Point::new(rect.left, rect.top), size, &drawn);
            };
            frame.custom(
                draw,
                Size::Grow,
                row_height * grid.rows(),
                Some(ids::peek_outside()),
            );
        }
    }
    frame.finish();
}

fn heading(frame: &mut Frame<'_>, symbol: &Symbol, file: &SourceFile, area: Extent) {
    let span = symbol.span();
    let columns = usize::try_from(area.width.ratio(frame.cell_width().max(PIXEL))).unwrap_or(0);
    let kind = format!("{} ", symbol.kind());
    let place = format!(
        "{}:{}-{}",
        file.path(),
        span.start().number(),
        span.end().number()
    );
    let room = columns
        .saturating_sub(kind.len() + usize::try_from(PEEK_PLACE_ROOM.get()).unwrap_or(0) / 2);
    frame.start(Container::Stack);
    frame.title(symbol.name().as_str());
    frame.caption(vec![
        Run::new(kind, WEAK),
        Run::new(Clipped::left(&place, room).to_string(), WEAK),
    ]);
    if file.is_pending() {
        let server = file.language().map_or_else(
            || "the server".to_owned(),
            |language| language.program().to_string(),
        );
        frame.label(
            format!("waiting for {server}; callers and callees may be incomplete"),
            ORANGE,
        );
    }
    frame.finish();
}
fn focus_actions(model: &Model, frame: &mut Frame<'_>, symbol: SymbolId) {
    let on_step = model
        .nav
        .step_key()
        .and_then(|key| model.step(key))
        .and_then(domain::Step::resolved_symbol)
        == Some(symbol);
    frame.start(Container::ToolbarSmall);
    if model.nav.tour().is_some()
        && !on_step
        && frame
            .small_button("add step", ids::ADD_FOCUS.target())
            .clicked()
    {
        frame.push(Action::Authoring(Authoring::AddSymbol(
            symbol,
            Hang::Target,
        )));
    }
    if frame
        .small_button("promote", ids::PROMOTE_FOCUS.target())
        .clicked()
    {
        frame.push(Action::Authoring(Authoring::Promote(symbol)));
    }
    frame.finish();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Calls {
    Callers,
    Callees,
}

enum ReferenceLine {
    Title(Label),
    Call {
        calls: Calls,
        position: Count,
        symbol: SymbolId,
    },
    File {
        file: FileId,
        references: Count,
    },
    Reference {
        position: Count,
        file: FileId,
        line: Line,
    },
}

fn reference_lines(model: &Model, symbol: &Symbol, reference_title: Label) -> Vec<ReferenceLine> {
    let mut lines = Vec::new();
    for (calls, title, list) in [
        (Calls::Callers, "Callers", symbol.callers()),
        (Calls::Callees, "Callees", symbol.callees()),
    ] {
        lines.push(ReferenceLine::Title(Label::new(format!(
            "{title} ({})",
            list.len()
        ))));
        lines.extend(
            list.iter()
                .enumerate()
                .map(|(position, other)| ReferenceLine::Call {
                    calls,
                    position: Count::new(position),
                    symbol: *other,
                }),
        );
    }
    lines.push(ReferenceLine::Title(reference_title));
    let mut header: Option<usize> = None;
    for (position, location) in symbol.references().iter().enumerate() {
        let Some(file) = model.index.find_file(&location.file) else {
            continue;
        };
        match header.and_then(|at| lines.get_mut(at)) {
            Some(ReferenceLine::File {
                file: open,
                references,
            }) if *open == file => *references = Count::new(references.get() + 1),
            _ => {
                header = Some(lines.len());
                lines.push(ReferenceLine::File {
                    file,
                    references: Count::new(1),
                });
            }
        }
        lines.push(ReferenceLine::Reference {
            position: Count::new(position),
            file,
            line: location.line,
        });
    }
    lines
}

fn call_row(
    model: &Model,
    frame: &mut Frame<'_>,
    calls: Calls,
    position: Count,
    other: SymbolId,
    dimmed: Color,
) {
    let index = &model.index;
    let (Some(found), Some(file)) = (index.symbol(other), index.file(other.file())) else {
        return;
    };
    let (row, add) = match calls {
        Calls::Callers => (ids::CALLER_ROW, ids::ADD_CALLER),
        Calls::Callees => (ids::CALLEE_ROW, ids::ADD_CALLEE),
    };
    let runs = vec![
        Run::new(format!("  {}", found.name()), dimmed),
        Run::new(
            format!("  {}:{}", file.path(), found.span().start().number()),
            WEAK,
        ),
    ];
    let clicks = frame.row_with_action(
        runs,
        row.nth(position),
        None,
        model
            .nav
            .tour()
            .is_some()
            .then(|| RowAction::add_step(add.nth(position))),
    );
    if dimmed == PENDING {
        return;
    }
    if clicks.acted() {
        frame.push(Action::Authoring(Authoring::AddSymbol(other, Hang::Target)));
    } else if clicks.row.clicked() {
        frame.push(Action::Focus(other));
    }
}

fn reference_row(
    model: &Model,
    frame: &mut Frame<'_>,
    line: &ReferenceLine,
    dimmed: Color,
    width: Count,
) {
    match line {
        ReferenceLine::Title(title) => frame.row_text(vec![Run::new(title.clone(), WEAK)]),
        ReferenceLine::Call {
            calls,
            position,
            symbol,
        } => call_row(model, frame, *calls, *position, *symbol, dimmed),
        ReferenceLine::File { file, references } => {
            let Some(source) = model.index.file(*file) else {
                return;
            };
            let runs = vec![
                Run::new(format!("  {}", source.path()), TEXT),
                Run::new(format!("  {references}"), WEAK),
            ];
            if frame
                .row(
                    runs,
                    ids::REFERENCE_FILE_ROW.nth(Count::new(file.number())),
                    Chosen::Plain,
                )
                .clicked()
            {
                frame.push(Action::GoTo(*file, Line::new(0)));
            }
        }
        ReferenceLine::Reference {
            position,
            file,
            line,
        } => {
            let text = model
                .index
                .file(*file)
                .and_then(|source| source.text().line(*line))
                .map_or("", |shown| shown.as_str().trim());
            let runs = vec![
                Run::new(
                    format!("    {:>digits$}  ", line.number(), digits = width.get()),
                    WEAK,
                ),
                Run::new(text, dimmed),
            ];
            if frame
                .row(runs, ids::REFERENCE_ROW.nth(*position), Chosen::Plain)
                .clicked()
            {
                frame.push(Action::GoTo(*file, *line));
            }
        }
    }
}

pub(super) fn references_panel(model: &Model, frame: &mut Frame<'_>, area: Extent) {
    frame.start(Container::PanelColumn);
    peek_section(model, frame, area);
    let index = &model.index;
    let Some((current, symbol, file)) = model
        .nav
        .focus()
        .and_then(|current| Some((current, index.symbol(current)?, index.file(current.file())?)))
    else {
        frame.label(
            "no symbol selected: click one in Symbols, a tour, or the Source view",
            WEAK,
        );
        frame.finish();
        return;
    };
    let references = symbol.references();
    let asking = file.backend() == Backend::Server && references.is_empty();
    let probe = index::name_position(index, current).map(|position| Probe {
        position,
        hash: file.hash(),
    });
    let mut asked = model.queries.asked().get() + frame.overlay.asked.get() > 0;
    if asking
        && let Some(probe) = probe
        && !model.queries.asked_references(&probe)
        && let Some(language) = file
            .language()
            .filter(|language| !model.work.no_server(*language))
    {
        if !model.work.started(language) {
            frame.overlay.status = Some(Status::Starting(language.program()));
        }
        frame.push(Action::AskReferences(language, probe));
        asked = true;
    }
    heading(frame, symbol, file, area);
    focus_actions(model, frame, current);
    let shown = references
        .iter()
        .filter(|location| index.find_file(&location.file).is_some())
        .count();
    let title = Label::new(if asking && asked {
        "Other references (asking the server)".to_owned()
    } else {
        format!("Other references ({shown})")
    });
    let lines = reference_lines(model, symbol, title);
    let width = references
        .iter()
        .map(|location| location.line.number())
        .max()
        .map_or(Count::new(1), |number| Count::new(number.to_string().len()));
    let dimmed = if file.is_pending() { PENDING } else { TEXT };
    let id = ids::references();
    let scrolled = frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let row_height = frame.row_height() + ROW_EXTRA;
    let count = Count::new(lines.len());
    let window = frame.rows_window(
        scrolled.offset,
        scrolled.interaction.rect(),
        count,
        row_height,
        Count::new(40),
    );
    for line in lines
        .iter()
        .skip(window.first.get())
        .take(window.visible.get())
    {
        reference_row(model, frame, line, dimmed, width);
    }
    frame.rows_after(count, &window, row_height, Px::ZERO);
    frame.finish();
    frame.finish();
}
