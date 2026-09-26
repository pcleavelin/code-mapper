use std::rc::Rc;

use domain::{Backend, Line, Location, SourceFile, Symbol};
use ui::{Canvas, Color, Count, Extent, Grid, Label, Point, Px, Rect, Run, Size};

use crate::action::Action;
use crate::dock::Panel;
use crate::ids;
use crate::model::Model;
use crate::peek::{Peek, Probe};
use crate::status::Status;
use crate::text::Clipped;
use crate::theme::{
    CLOSE_BUTTON, FIELD, PEEK_EXTRA, PEEK_LEAST, PEEK_PLACE_ROOM, PEEK_TITLE, PENDING, PIXEL,
    SLICE, TEXT, WEAK,
};
use crate::views::docked::grip;
use crate::widgets::{Chosen, CodeBlock, Container, Frame, Marks, Scroller, Width};

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
            title: Label::new("Peek (outside)"),
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
        .small_button_sized("x", Some(CLOSE_BUTTON), ids::PEEK_CLOSE.target())
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

fn heading(frame: &mut Frame<'_>, symbol: &Symbol, file: &SourceFile) {
    let span = symbol.span();
    let place = format!(
        "{} {}:{}-{}",
        symbol.kind(),
        file.path(),
        span.start().number(),
        span.end().number()
    );
    frame.start(Container::Stack);
    frame.label(symbol.name().as_str(), TEXT);
    frame.label(place, WEAK);
    if file.is_pending() {
        let server = file.language().map_or_else(
            || "the server".to_owned(),
            |language| language.program().to_string(),
        );
        frame.label(format!("waiting for {server}"), WEAK);
    }
    frame.finish();
}

fn call_lists(model: &Model, frame: &mut Frame<'_>, symbol: &Symbol, dimmed: Color) {
    let pending = dimmed == PENDING;
    for (title, list, id) in [
        ("Xrefs to", symbol.callers(), ids::CALLER_ROW),
        ("Xrefs from", symbol.callees(), ids::CALLEE_ROW),
    ] {
        frame.label(format!("{title} ({})", list.len()), WEAK);
        for (position, other) in list.iter().enumerate() {
            let described = cli::symbol_label(&model.index, *other);
            if frame
                .row(
                    vec![Run::new(described.as_str(), dimmed)],
                    id.nth(Count::new(position)),
                    Chosen::Plain,
                )
                .clicked()
                && !pending
            {
                frame.push(Action::Focus(*other));
            }
        }
    }
}

fn reference_rows(model: &Model, frame: &mut Frame<'_>, references: &[Location], dimmed: Color) {
    let index = &model.index;
    for (position, location) in references.iter().enumerate() {
        let Some(found) = index.find_file(&location.file) else {
            continue;
        };
        let text = index
            .file(found)
            .and_then(|source| source.text().line(location.line))
            .map_or("", |line| line.as_str().trim());
        let runs = vec![
            Run::new(
                format!("{}:{}: ", location.file, location.line.number()),
                WEAK,
            ),
            Run::new(text, dimmed),
        ];
        if frame
            .row(
                runs,
                ids::REFERENCE_ROW.nth(Count::new(position)),
                Chosen::Plain,
            )
            .clicked()
        {
            frame.push(Action::GoTo(found, location.line));
        }
    }
}

pub(super) fn xrefs_panel(model: &Model, frame: &mut Frame<'_>, area: Extent) {
    frame.start(Container::PanelColumn);
    grip(model, frame, Panel::Xrefs);
    frame.label("Xrefs", WEAK);
    frame.finish();
    peek_section(model, frame, area);
    let index = &model.index;
    let Some((current, symbol, file)) = model
        .nav
        .focus()
        .and_then(|current| Some((current, index.symbol(current)?, index.file(current.file())?)))
    else {
        frame.label("no symbol selected", WEAK);
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
    heading(frame, symbol, file);
    frame.scroll_column(
        ids::xrefs(),
        model.scrolls.get(ids::xrefs()),
        Scroller::Plain,
        None,
    );
    let dimmed = if file.is_pending() { PENDING } else { TEXT };
    call_lists(model, frame, symbol, dimmed);
    let shown = references
        .iter()
        .filter(|location| index.find_file(&location.file).is_some())
        .count();
    frame.label(
        if asking && asked {
            "References (asking the server)".to_owned()
        } else {
            format!("References ({shown})")
        },
        WEAK,
    );
    reference_rows(model, frame, references, dimmed);
    frame.finish();
    frame.finish();
}
