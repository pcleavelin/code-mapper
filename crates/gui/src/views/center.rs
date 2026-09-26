use domain::{Change, Line, PathDiff, StepChange, SymbolName};
use ui::{Count, Label, Px, Run};

use crate::action::Action;
use crate::field::Which;
use crate::graph::{GraphAction, GraphFrame, draw_scene};
use crate::ids;
use crate::keys;
use crate::model::{Model, PathSlot, StepKey, Tab};
use crate::status::Status;
use crate::text::Tag;
use crate::theme::{
    FIELD, GREEN, LINE_FIELD, LINES_SELECTED, LISTING_GUESS, RED, ROW_EXTRA, TEXT, WEAK,
};
use crate::widgets::{Chosen, CodeBlock, Container, Frame, Marks, Scroller, Width};

pub(super) fn listing(model: &Model, frame: &mut Frame<'_>) {
    let Some(file) = model.nav.file() else {
        frame.label("click a symbol to open its file", WEAK);
        return;
    };
    let Some(source) = model.index.file(file) else {
        return;
    };
    let id = ids::listing();
    let row_height = frame.row_height();
    let count = source.text().count();
    let lines = Px::new(i32::try_from(count.value()).unwrap_or(0));
    let mut offset = model.scrolls.get(id);
    if let Some(request) = model.nav.scroll_to() {
        let height = frame
            .ui
            .placement(id)
            .map_or(LISTING_GUESS, |placement| placement.rect.height);
        let above = Px::new(
            (i32::try_from(request.line.value()).unwrap_or(0) - 3).max(0) * row_height.get(),
        );
        offset = above.min((Px::new(lines.get() * row_height.get()) - height).max(Px::ZERO));
        frame.push(Action::ScrolledToLine(request.ticket));
    }
    frame.start(Container::Toolbar);
    frame.label(source.path().as_str(), TEXT);
    frame.label("click a line, shift-click to extend, then 'pin selection'; double-click or ctrl-click an identifier to jump; alt-click to peek", WEAK);
    frame.grow();
    frame.label("line", WEAK);
    frame.field(
        &model.fields,
        Which::GoToLine,
        &ui::Label::default(),
        LINE_FIELD,
    );
    frame.finish();
    let scrolled = frame.scroll_column(id, offset, Scroller::Plain, Some(FIELD));
    let selection = model.nav.lines();
    let anchors: Vec<(domain::Span, bool)> = model
        .nav
        .path()
        .and_then(|path| model.path(path))
        .map(|path| {
            path.steps()
                .iter()
                .filter(|step| step.file() == source.path())
                .map(|step| (step.span(), step.is_stale()))
                .collect()
        })
        .unwrap_or_default();
    let total = Count::new(usize::try_from(count.value()).unwrap_or(0));
    let window = frame.rows_window(
        scrolled.offset,
        scrolled.interaction.rect(),
        total,
        row_height,
        Count::new(60),
    );
    let end = (window.first.get() + window.visible.get()).min(total.get());
    if total.get() > 0 && end > window.first.get() {
        let bar = |line: Line| {
            anchors
                .iter()
                .find(|(span, _)| span.contains(line))
                .map(|(_, stale)| if *stale { RED } else { GREEN })
        };
        let background = |line: Line| {
            selection
                .filter(|chosen| chosen.low() <= line && line <= chosen.high())
                .map(|_| LINES_SELECTED)
        };
        let coded = frame.code_block(
            model,
            &CodeBlock {
                file,
                start: Line::new(u32::try_from(window.first.get()).unwrap_or(0)),
                end: Line::new(u32::try_from(end - 1).unwrap_or(0)),
                id: ids::LINES.id(),
                width: Width::Wide,
                marks: Marks {
                    background: &background,
                    bar: &bar,
                },
            },
        );
        let pointer = frame.ui.pointer();
        if let Some(spot) = coded.spot
            && let Some(extend) = keys::selects_line(coded.interaction, pointer)
        {
            frame.push(Action::SelectLine(spot.line, extend));
        }
    }
    frame.rows_after(total, &window, row_height, row_height);
    frame.finish();
}

pub(super) fn results_view(model: &Model, frame: &mut Frame<'_>) {
    if model.results.is_empty() {
        let search = model.fields.get(Which::Search).text();
        frame.label(
            if search.is_empty() {
                "type a regex in the search box and press enter"
            } else {
                "no hits"
            },
            WEAK,
        );
        return;
    }
    let id = ids::results();
    let row_height = frame.row_height() + ROW_EXTRA;
    let scrolled = frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let count = Count::new(model.results.len());
    let window = frame.rows_window(
        scrolled.offset,
        scrolled.interaction.rect(),
        count,
        row_height,
        Count::new(60),
    );
    for (position, hit) in model
        .results
        .iter()
        .enumerate()
        .skip(window.first.get())
        .take(window.visible.get())
    {
        let Some(source) = model.index.file(hit.file) else {
            continue;
        };
        let text = source
            .text()
            .line(hit.line)
            .map_or("", |line| line.as_str().trim());
        let runs = vec![
            Run::new(format!("{}:{}: ", source.path(), hit.line.number()), WEAK),
            Run::new(text, TEXT),
        ];
        if frame
            .row(runs, ids::HIT_ROW.nth(Count::new(position)), Chosen::Plain)
            .clicked()
        {
            frame.push(Action::GoTo(hit.file, hit.line));
        }
    }
    frame.rows_after(count, &window, row_height, Px::ZERO);
    frame.finish();
}

fn summary(diff: &PathDiff) -> Label {
    Label::new(match diff.change() {
        Change::Added => format!("{} steps", diff.steps().len()),
        Change::Removed => format!("{} steps", diff.removed().len()),
        Change::Same | Change::Changed => {
            let count_of = |change: StepChange| {
                diff.steps()
                    .iter()
                    .filter(|step| step.change == Some(change))
                    .count()
            };
            let mut parts = Vec::new();
            for (count, what) in [
                (count_of(StepChange::Added), "new"),
                (diff.removed().len(), "removed"),
                (count_of(StepChange::Repinned), "re-pinned"),
                (count_of(StepChange::NoteEdited), "note edited"),
                (count_of(StepChange::Relinked), "link changed"),
            ] {
                if count > 0 {
                    parts.push(format!("{count} {what}"));
                }
            }
            if diff.note_changed() {
                parts.push("path note, kind or group changed".to_owned());
            }
            parts.join(", ")
        }
    })
}

fn step_lines(model: &Model, frame: &mut Frame<'_>, path: PathSlot, diff: &PathDiff) {
    for step_diff in diff.steps() {
        let Some(change) = step_diff.change else {
            continue;
        };
        let Some(slot) = model.step_slot(path, &step_diff.step) else {
            continue;
        };
        let key = StepKey { path, step: slot };
        let Some(step) = model.step(key) else {
            continue;
        };
        let span = step.span();
        frame.label(
            format!(
                "    {} {} {} {}:{}-{}  {}",
                if change == StepChange::Added {
                    "+"
                } else {
                    "~"
                },
                model.number_of(key).as_str(),
                step.symbol().map_or("", SymbolName::as_str),
                step.file(),
                span.start().number(),
                span.end().number(),
                Tag::change(change)
            ),
            WEAK,
        );
    }
}

fn diff_row(model: &Model, frame: &mut Frame<'_>, position: Count, diff: &PathDiff) {
    let (mark, color) = match diff.change() {
        Change::Same => return,
        Change::Added => ("+", GREEN),
        Change::Removed => ("-", RED),
        Change::Changed => ("~", GREEN),
    };
    let path = model.find_path(diff.name());
    let runs = vec![
        Run::new(format!("{mark} {}   ", diff.name()), color),
        Run::new(summary(diff), WEAK),
    ];
    if frame
        .row(runs, ids::DIFF_ROW.nth(position), Chosen::Plain)
        .clicked()
    {
        if let Some(open) = path {
            frame.push(Action::OpenPath(open, Tab::Path));
        } else {
            let status = Status::OnlyInParent {
                name: diff.name().clone(),
                steps: Count::new(diff.removed().len()),
            };
            frame.overlay.status = Some(status.clone());
            frame.push(Action::Status(status));
        }
    }
    if let Some(path) = path.filter(|_| diff.change() == Change::Changed) {
        step_lines(model, frame, path, diff);
    }
    for step in diff.removed() {
        frame.label(
            format!(
                "    - {} {} (removed)",
                step.file(),
                step.symbol().map_or("", SymbolName::as_str)
            ),
            WEAK,
        );
    }
}

pub(super) fn diff_view(model: &Model, frame: &mut Frame<'_>) {
    frame.start(Container::Toolbar);
    frame.label("Changes against the parent revision", TEXT);
    frame.label("@- in jj, HEAD in git", WEAK);
    if frame
        .small_button("refresh", ids::DIFF_REFRESH.target())
        .clicked()
    {
        frame.push(Action::RefreshBase);
    }
    frame.finish();
    let Some(base) = model.base.map.as_ref() else {
        let why = if model.work.reading_base() {
            "reading the parent revision..."
        } else {
            model.base.why.as_str()
        };
        frame.label(format!("no map to compare with: {why}"), WEAK);
        return;
    };
    let diffs = model.map.diff(base);
    if diffs.iter().all(|diff| diff.change() == Change::Same) {
        frame.label("no changes", WEAK);
        return;
    }
    frame.scroll_column(
        ids::diff(),
        model.scrolls.get(ids::diff()),
        Scroller::Plain,
        None,
    );
    for (position, diff) in diffs.iter().enumerate() {
        diff_row(model, frame, Count::new(position), diff);
    }
    frame.finish();
}

pub(super) fn graph_tab(model: &Model, frame: &mut Frame<'_>, graph: Option<GraphFrame>) {
    let Some(GraphFrame {
        scene,
        deferred,
        tooltip,
        zoom,
    }) = graph
    else {
        return;
    };
    for action in deferred {
        match action {
            Action::Hover(language, probe) => frame.push_hover(model, language, probe),
            other => frame.push(other),
        }
    }
    if tooltip.is_some() {
        frame.tooltip = tooltip;
    }
    frame.start(Container::Toolbar);
    frame.label(
        "drag or scroll to pan, pinch or ctrl+wheel to zoom, drag a title to move a node",
        WEAK,
    );
    if frame
        .small_button("1:1", ids::GRAPH_ONE_TO_ONE.target())
        .clicked()
    {
        frame.push(Action::Graph(GraphAction::OneToOne));
    }
    if frame
        .small_button("auto layout", ids::GRAPH_AUTO.target())
        .clicked()
    {
        frame.push(Action::Graph(GraphAction::AutoLayout));
    }
    if frame.small_button("fit", ids::GRAPH_FIT.target()).clicked() {
        frame.push(Action::Graph(GraphAction::WantFit));
    }
    if let Some(path) = model.graph.built().path.and_then(|path| model.path(path)) {
        frame.label(
            format!(
                "path '{}': green edges are its steps, grey ones expansions, orange a call back up the tree",
                path.name()
            ),
            WEAK,
        );
    }
    frame.label(format!("{:.0}%", zoom.get() * 100.0), WEAK);
    frame.finish();
    frame.canvas(
        move |canvas, _| draw_scene(canvas, &scene),
        ids::GRAPH_CANVAS.id(),
    );
}
