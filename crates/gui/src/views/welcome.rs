use domain::{Change, TourDiff};
use ui::{Count, Extent, Label, Px, Run};

use crate::action::Action;
use crate::ids;
use crate::model::{Model, Openness, Tab};
use crate::panels::View;
use crate::status::Status;
use crate::text::{Clipped, Counted, Noun};
use crate::theme::{
    GREEN, PANEL_PADDING, PANEL_TEXT_ROOM, PIXEL, RED, START_PAGE_WIDTH, TEXT, WEAK,
};
use crate::welcome::{MapFigures, TopGroup, WelcomeAct, key_rows};
use crate::widgets::{Container, Frame, Hyperlink, Scroller};

use super::center;

const CHANGES_SHOWN: Count = Count::new(12);
const ROOTS_SHOWN: Count = Count::new(8);
const GAP_CELLS: Count = Count::new(3);

struct Columns(usize);

impl Columns {
    const fn get(&self) -> usize {
        self.0
    }
}

pub(super) fn start_page(model: &Model, frame: &mut Frame<'_>, area: Extent) {
    let id = ids::start_page();
    frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let cell = frame.cell_width().max(PIXEL);
    let room = area.width - PANEL_TEXT_ROOM - Frame::scrollbar_width() - PANEL_PADDING * 2;
    let width = Px::new(cell.get() * START_PAGE_WIDTH.get())
        .min(room)
        .max(cell);
    let columns = Columns(usize::try_from(width.ratio(cell)).unwrap_or(0));
    frame.start(Container::Centered);
    frame.start(Container::StartPage { width });
    let line = frame.row_height();
    frame.spacer(line + line);
    let figures = model.figures();
    heading(model, frame, &figures, &columns);
    section(frame, &Label::new("Changed since the parent revision"));
    changes(model, frame, &columns);
    section(frame, &Label::new("Start reading"));
    groups(model, frame, &columns, &figures);
    section(frame, &Label::new("Build a tour"));
    build_tour(model, frame, &columns);
    section(frame, &Label::new("Keys"));
    keys(frame);
    frame.spacer(line + line);
    frame.finish();
    frame.finish();
    frame.finish();
}

fn heading(model: &Model, frame: &mut Frame<'_>, figures: &MapFigures, columns: &Columns) {
    frame.title(model.root_name());
    let parts = [
        Run::new(Counted::new(figures.tours, Noun::Tour).to_string(), WEAK),
        Run::new(
            format!(
                "{}/{} symbols covered ({}%)",
                figures.covered.get(),
                figures.symbols.get(),
                figures.percent().get()
            ),
            WEAK,
        ),
        Run::new(
            format!(
                "{} stale {}",
                figures.stale.get(),
                if figures.stale.get() == 1 {
                    "step"
                } else {
                    "steps"
                }
            ),
            if figures.stale.get() > 0 { RED } else { WEAK },
        ),
    ];
    let dot = "  \u{b7}  ";
    let across: usize = parts
        .iter()
        .map(|part| part.text.as_str().chars().count() + dot.len())
        .sum();
    if across > columns.get() {
        for part in parts {
            frame.row_text(vec![part]);
        }
        return;
    }
    let mut runs = Vec::new();
    for part in parts {
        if !runs.is_empty() {
            runs.push(Run::new(dot, WEAK));
        }
        runs.push(part);
    }
    frame.row_text(runs);
}

fn section(frame: &mut Frame<'_>, title: &Label) {
    let line = frame.row_height();
    frame.spacer(line);
    frame.row_text(vec![Run::new(title.clone(), TEXT)]);
}

struct Pad {
    width: Count,
    room: Count,
}

struct Padded {
    shown: Label,
    fill: Label,
}

fn padded(text: &Label, pad: &Pad) -> Padded {
    let shown = Clipped::right(text.as_str(), pad.room.get().min(pad.width.get())).to_string();
    let fill = pad.width.get().saturating_sub(shown.chars().count());
    Padded {
        shown: Label::new(shown),
        fill: Label::new(" ".repeat(fill + GAP_CELLS.get())),
    }
}

fn padded_hyperlink(lead: Vec<Run>, text: &Label, pad: &Pad, detail: Run) -> Hyperlink {
    let Padded { shown, fill } = padded(text, pad);
    Hyperlink {
        lead,
        text: shown,
        detail: vec![Run::new(fill, WEAK), detail],
    }
}

fn changes(model: &Model, frame: &mut Frame<'_>, columns: &Columns) {
    let Some(base) = model.base.map.as_ref() else {
        if model.work.reading_base() {
            frame.row_text(vec![Run::new("reading the parent revision\u{2026}", WEAK)]);
        } else {
            frame.row_text(vec![Run::new(
                format!("nothing to compare with: {}", model.base.why.as_str()),
                WEAK,
            )]);
        }
        return;
    };
    let diffs: Vec<TourDiff> = model
        .map
        .diff(base)
        .into_iter()
        .filter(|diff| diff.change() != Change::Same)
        .collect();
    if diffs.is_empty() {
        frame.row_text(vec![Run::new(
            "no map changes since the parent revision",
            WEAK,
        )]);
        return;
    }
    let shown: Vec<&TourDiff> = diffs.iter().take(CHANGES_SHOWN.get()).collect();
    let width = shown
        .iter()
        .map(|diff| diff.name().as_str().chars().count())
        .max()
        .unwrap_or(0);
    let room = columns.get().saturating_sub(2 + GAP_CELLS.get() + 12);
    let pad = Pad {
        width: Count::new(width),
        room: Count::new(room),
    };
    for (position, diff) in shown.iter().enumerate() {
        change_row(model, frame, Count::new(position), diff, &pad);
    }
    let more = diffs.len() - shown.len();
    if more > 0 {
        let hyperlink = Hyperlink {
            lead: Vec::new(),
            text: Label::new(format!("and {more} more in the Diff view")),
            detail: Vec::new(),
        };
        if frame
            .hyperlink(hyperlink, ids::START_MORE_CHANGES.target())
            .clicked()
        {
            frame.push(Action::ShowView(View::Diff));
        }
    }
}

fn change_row(model: &Model, frame: &mut Frame<'_>, position: Count, diff: &TourDiff, pad: &Pad) {
    let (mark, color) = match diff.change() {
        Change::Same => return,
        Change::Added => ("+ ", GREEN),
        Change::Removed => ("- ", RED),
        Change::Changed => ("~ ", GREEN),
    };
    let detail = match diff.change() {
        Change::Added => format!(
            "({})",
            Counted::new(Count::new(diff.steps().len()), Noun::Step)
        ),
        Change::Removed => "only in the parent revision".to_owned(),
        Change::Same | Change::Changed => center::summary(diff).as_str().to_owned(),
    };
    let tour = model.find_tour(diff.name());
    let hyperlink = padded_hyperlink(
        vec![Run::new(mark, color)],
        &Label::new(diff.name().as_str()),
        pad,
        Run::new(detail, WEAK),
    );
    if frame
        .hyperlink(hyperlink, ids::START_CHANGE.nth(position))
        .clicked()
    {
        if let Some(open) = tour {
            frame.push(Action::OpenTour(open, Tab::Tour));
        } else {
            let status = Status::OnlyInParent {
                name: diff.name().clone(),
                steps: Count::new(diff.removed().len()),
            };
            frame.overlay.status = Some(status.clone());
            frame.push(Action::Status(status));
        }
    }
}

fn groups(model: &Model, frame: &mut Frame<'_>, columns: &Columns, figures: &MapFigures) {
    let groups = model.top_groups();
    let uncovered = format!("{} symbols", figures.uncovered().get());
    let width = groups
        .iter()
        .map(|group_row| match group_row {
            TopGroup::Named(group, _) => group.as_str().chars().count() + 1,
            TopGroup::Ungrouped(_) => "no group".len(),
        })
        .chain([uncovered.chars().count()])
        .max()
        .unwrap_or(0);
    let room = columns.get().saturating_sub(GAP_CELLS.get() + 10);
    let pad = Pad {
        width: Count::new(width),
        room: Count::new(room),
    };
    for group_row in &groups {
        match group_row {
            TopGroup::Named(group, tours) => {
                let count = Count::new(usize::try_from(tours.value()).unwrap_or(0));
                let hyperlink = padded_hyperlink(
                    Vec::new(),
                    &Label::new(format!("{}/", group.as_str())),
                    &pad,
                    Run::new(Counted::new(count, Noun::Tour).to_string(), WEAK),
                );
                if frame
                    .hyperlink(
                        hyperlink,
                        ids::START_GROUP.with(&Label::new(group.as_str())),
                    )
                    .clicked()
                {
                    frame.push(Action::OpenGroup(group.clone(), Openness::Open));
                    frame.push(Action::ShowView(View::Tours));
                }
            }
            TopGroup::Ungrouped(count) => {
                let hyperlink = padded_hyperlink(
                    Vec::new(),
                    &Label::new("no group"),
                    &pad,
                    Run::new(Counted::new(*count, Noun::Tour).to_string(), WEAK),
                );
                if frame
                    .hyperlink(hyperlink, ids::START_UNGROUPED.target())
                    .clicked()
                {
                    frame.push(Action::ShowView(View::Tours));
                }
            }
        }
    }
    let hyperlink = padded_hyperlink(
        Vec::new(),
        &Label::new(&uncovered),
        &pad,
        Run::new("in no tour; Files shows covered/total per file", WEAK),
    );
    if frame
        .hyperlink(hyperlink, ids::START_UNCOVERED.target())
        .clicked()
    {
        frame.push(Action::ShowView(View::Files));
    }
}

fn build_tour(model: &Model, frame: &mut Frame<'_>, columns: &Columns) {
    let build = Hyperlink {
        lead: Vec::new(),
        text: Label::new("Build a tour by hand"),
        detail: vec![Run::new(
            "   name, start symbol, steps, note: a page each",
            WEAK,
        )],
    };
    if frame.hyperlink(build, ids::BUILD_TOUR.target()).clicked() {
        frame.push(Action::Welcome(WelcomeAct::BuildTour));
    }
    if !model.map.tours().is_empty() {
        return;
    }
    frame.row_text(vec![Run::new(
        "or start from an entry point, a symbol nothing calls:",
        WEAK,
    )]);
    let roots: Vec<_> = model
        .index
        .roots()
        .into_iter()
        .filter(|id| !model.index.in_tests(*id))
        .take(ROOTS_SHOWN.get())
        .filter_map(|id| {
            let symbol = model.index.symbol(id)?;
            let file = model.index.file(id.file())?;
            Some((
                id,
                symbol.name().as_str(),
                file.path().as_str(),
                symbol.span(),
            ))
        })
        .collect();
    let width = roots
        .iter()
        .map(|(_, name, _, _)| name.chars().count())
        .max()
        .unwrap_or(0);
    let room = columns.get().saturating_sub(GAP_CELLS.get() + 20);
    let pad = Pad {
        width: Count::new(width),
        room: Count::new(room),
    };
    for (position, (id, name, path, span)) in roots.iter().enumerate() {
        let hyperlink = padded_hyperlink(
            Vec::new(),
            &Label::new(*name),
            &pad,
            Run::new(format!("{path}:{}", span.start().number()), WEAK),
        );
        if frame
            .hyperlink(hyperlink, ids::START_ROOT.nth(Count::new(position)))
            .clicked()
        {
            frame.push(Action::Jump(*id));
        }
    }
}

fn keys(frame: &mut Frame<'_>) {
    let rows = key_rows();
    let width = rows
        .iter()
        .map(|row| row.chord.as_str().chars().count())
        .max()
        .unwrap_or(0);
    for row in rows {
        let fill = width.saturating_sub(row.chord.as_str().chars().count()) + GAP_CELLS.get();
        frame.row_text(vec![
            Run::new(row.chord, TEXT),
            Run::new(" ".repeat(fill), WEAK),
            Run::new(row.meaning, WEAK),
        ]);
    }
}
