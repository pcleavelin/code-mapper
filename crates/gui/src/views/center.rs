use domain::{Change, FileId, Line, RelativePath, StepChange, SymbolName, TourDiff};
use ui::{Axis, Count, Icon, Label, Px, Rect, Run, Scrollbar, Size};

use crate::action::Action;
use crate::authoring::Authoring;
use crate::field::Which;
use crate::graph::{GraphAction, GraphFrame, Minimap, draw_scene};
use crate::ids::{self, Target};
use crate::keys::{self, LineGesture};
use crate::model::{HIT_LIMIT, HitsShown, Model, StepKey, Tab, TourSlot};
use crate::panels::{Direction, View};
use crate::status::Status;
use crate::text::{Counted, Noun, Tag};
use crate::theme::{
    Cells, EDGE_SCROLL_BAND, EDGE_SCROLL_MOST, FIELD, FIT_BUTTON, GREEN, LINE_FIELD,
    LINES_SELECTED, ORANGE, PIXEL, RED, ROW_EXTRA, SOURCE_GUESS, TEXT, WEAK, ZOOM_BUTTON,
    ZOOM_PERCENT, Zoom, ZoomStep,
};
use crate::widgets::{Chosen, CodeBlock, Coded, Container, Frame, Marks, Padding, Scroller, Width};

use super::authoring;

fn source_toolbar(model: &Model, frame: &mut Frame<'_>, path: &RelativePath) {
    frame.start(Container::Toolbar);
    frame.label(path.as_str(), TEXT);
    let add = match model.nav.lines() {
        Some(chosen) if chosen.low() == chosen.high() => {
            format!("add line {} as a step", chosen.low().number())
        }
        Some(chosen) => format!(
            "add lines {}-{} as a step",
            chosen.low().number(),
            chosen.high().number()
        ),
        None => "add step".to_owned(),
    };
    if frame.control(add, ids::ADD_LINES).clicked() {
        frame.push(Action::Authoring(Authoring::AddLines));
    }
    frame.label("line", WEAK);
    frame.field(
        &model.fields,
        Which::GoToLine,
        &ui::Label::default(),
        LINE_FIELD,
    );
    frame.caption(vec![Run::new(
        "click a line or drag over lines to select; double-click or ctrl-click an identifier to jump, alt-click to peek",
        WEAK,
    )]);
    frame.finish();
}

fn edge_scroll(vertical: Px, rect: Rect, row_height: Px) -> Px {
    let most = row_height * EDGE_SCROLL_MOST;
    if vertical < rect.top {
        -EDGE_SCROLL_BAND
            .apply(rect.top - vertical)
            .clamp(PIXEL, most)
    } else if vertical >= rect.bottom() {
        EDGE_SCROLL_BAND
            .apply(vertical - rect.bottom() + PIXEL)
            .clamp(PIXEL, most)
    } else {
        Px::ZERO
    }
}

fn select_lines(model: &Model, frame: &mut Frame<'_>, coded: &Coded, scrollbar: Option<Scrollbar>) {
    let pointer = frame.ui.pointer();
    let on_scrollbar = scrollbar.is_some_and(|shown| shown.track.contains(pointer.mouse));
    match (
        keys::selects_line(coded.interaction, pointer),
        coded.spot,
        coded.dragged_to,
    ) {
        (Some(LineGesture::Press), Some(spot), _) if !on_scrollbar => {
            frame.push(Action::SelectLine(spot.line, LineGesture::Press));
        }
        (Some(LineGesture::Drag), _, Some(line)) => {
            frame.push(Action::SelectLine(line, LineGesture::Drag));
        }
        _ if model.line_grab.is_some()
            && (coded.interaction.clicked() || !coded.interaction.down()) =>
        {
            frame.push(Action::ClearLineSelection);
        }
        _ => {}
    }
}

pub(super) fn source(model: &Model, frame: &mut Frame<'_>) {
    let Some(file) = model.nav.file() else {
        frame.note("click a symbol to open its file", WEAK, Padding::Step);
        return;
    };
    let Some(source) = model.index.file(file) else {
        return;
    };
    let id = ids::source();
    let row_height = frame.row_height();
    let count = source.text().count();
    let lines = Px::new(i32::try_from(count.value()).unwrap_or(0));
    let pointer = frame.ui.pointer();
    let column = frame.ui.placement(id);
    let mut offset = model.scrolls.get(id);
    if model.line_grab.is_some()
        && frame.ui.interaction(ids::LINES.id()).down()
        && let Some(column) = column
    {
        offset += edge_scroll(pointer.mouse.vertical, column.rect, row_height);
    }
    if let Some(request) = model.nav.scroll_to() {
        let height = frame
            .ui
            .placement(id)
            .map_or(SOURCE_GUESS, |placement| placement.rect.height);
        let above = Px::new(
            (i32::try_from(request.line.value()).unwrap_or(0) - 3).max(0) * row_height.get(),
        );
        offset = above.min((Px::new(lines.get() * row_height.get()) - height).max(Px::ZERO));
        frame.push(Action::ScrolledToLine(request.ticket));
    }
    source_toolbar(model, frame, source.path());
    authoring::target_strip(model, frame, View::Source);
    let scrolled = frame.scroll_column(id, offset, Scroller::Plain, Some(FIELD));
    let selection = model.nav.lines();
    let anchors: Vec<(domain::Span, bool)> = model
        .nav
        .tour()
        .and_then(|tour| model.tour(tour))
        .map(|tour| {
            tour.steps()
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
        let scrollbar = column.and_then(|placement| placement.scrollbar(Axis::Vertical, offset));
        select_lines(model, frame, &coded, scrollbar);
    }
    frame.rows_after(total, &window, row_height, row_height);
    frame.finish();
}

enum HitLine {
    File { file: FileId, hits: Count },
    Hit(Count),
}

fn hit_lines(model: &Model) -> Vec<HitLine> {
    let mut lines = Vec::with_capacity(model.hits.len());
    let mut header: Option<usize> = None;
    for (position, hit) in model.hits.iter().enumerate() {
        match header.and_then(|at| lines.get_mut(at)) {
            Some(HitLine::File { file, hits }) if *file == hit.file => {
                *hits = Count::new(hits.get() + 1);
            }
            _ => {
                header = Some(lines.len());
                lines.push(HitLine::File {
                    file: hit.file,
                    hits: Count::new(1),
                });
            }
        }
        lines.push(HitLine::Hit(Count::new(position)));
    }
    lines
}

pub(super) fn search_view(model: &Model, frame: &mut Frame<'_>) {
    if model.hits.is_empty() {
        let search = model.fields.get(Which::Search).text();
        frame.note(
            if search.is_empty() {
                "type a regex in the search box and press enter"
            } else {
                "no hits"
            },
            WEAK,
            Padding::Step,
        );
        return;
    }
    if model.hits_shown == HitsShown::First {
        frame.start(Container::Toolbar);
        frame.note(
            format!(
                "showing the first {HIT_LIMIT} hits; the search stops there, a narrower regex finds the rest",
            ),
            ORANGE,
            Padding::Step,
        );
        frame.finish();
    }
    let lines = hit_lines(model);
    let id = ids::search();
    let row_height = frame.row_height() + ROW_EXTRA;
    let scrolled = frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let count = Count::new(lines.len());
    let window = frame.rows_window(
        scrolled.offset,
        scrolled.interaction.rect(),
        count,
        row_height,
        Count::new(60),
    );
    let width = model
        .hits
        .iter()
        .map(|hit| hit.line.number())
        .max()
        .map_or(1, |number| number.to_string().len());
    for line in lines
        .iter()
        .skip(window.first.get())
        .take(window.visible.get())
    {
        match line {
            HitLine::File { file, hits } => {
                let Some(source) = model.index.file(*file) else {
                    continue;
                };
                let runs = vec![
                    Run::new(source.path().as_str(), TEXT),
                    Run::new(format!("  {}", Counted::new(*hits, Noun::Hit)), WEAK),
                ];
                if frame
                    .row(
                        runs,
                        ids::HIT_FILE_ROW.nth(Count::new(file.number())),
                        Chosen::Plain,
                    )
                    .clicked()
                {
                    frame.push(Action::GoTo(*file, Line::new(0)));
                }
            }
            HitLine::Hit(position) => {
                let Some(hit) = model.hits.get(position.get()) else {
                    continue;
                };
                let text = model
                    .index
                    .file(hit.file)
                    .and_then(|source| source.text().line(hit.line))
                    .map_or("", |shown| shown.as_str().trim());
                let runs = vec![
                    Run::new(format!("  {:>width$}  ", hit.line.number()), WEAK),
                    Run::new(text, TEXT),
                ];
                if frame
                    .row(runs, ids::HIT_ROW.nth(*position), Chosen::Plain)
                    .clicked()
                {
                    frame.push(Action::GoTo(hit.file, hit.line));
                }
            }
        }
    }
    frame.rows_after(count, &window, row_height, Px::ZERO);
    frame.finish();
}

pub(super) fn summary(diff: &TourDiff) -> Label {
    Label::new(match diff.change() {
        Change::Added => Counted::new(Count::new(diff.steps().len()), Noun::Step).to_string(),
        Change::Removed => Counted::new(Count::new(diff.removed().len()), Noun::Step).to_string(),
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
                parts.push("tour note, kind or group changed".to_owned());
            }
            parts.join(", ")
        }
    })
}

fn step_lines(model: &Model, frame: &mut Frame<'_>, tour: TourSlot, diff: &TourDiff) {
    for step_diff in diff.steps() {
        let Some(change) = step_diff.change else {
            continue;
        };
        let Some(slot) = model.step_slot(tour, &step_diff.step) else {
            continue;
        };
        let key = StepKey { tour, step: slot };
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

fn diff_row(model: &Model, frame: &mut Frame<'_>, position: Count, diff: &TourDiff) {
    let (mark, color) = match diff.change() {
        Change::Same => return,
        Change::Added => ("+", GREEN),
        Change::Removed => ("-", RED),
        Change::Changed => ("~", GREEN),
    };
    let tour = model.find_tour(diff.name());
    let runs = vec![
        Run::new(format!("{mark} {}   ", diff.name()), color),
        Run::new(summary(diff), WEAK),
    ];
    if frame
        .row(runs, ids::DIFF_ROW.nth(position), Chosen::Plain)
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
    if let Some(tour) = tour.filter(|_| diff.change() == Change::Changed) {
        step_lines(model, frame, tour, diff);
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
        frame.note(
            format!("no map to compare with: {why}"),
            WEAK,
            Padding::Step,
        );
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
    authoring::target_strip(model, frame, View::Graph);
    let Some(GraphFrame {
        scene,
        deferred,
        tooltip,
        zoom,
        minimap,
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
    if frame
        .small_button("auto layout", ids::GRAPH_AUTO.target())
        .clicked()
    {
        frame.push(Action::Graph(GraphAction::AutoLayout));
    }
    let turn = match model.graph.direction() {
        Direction::Right => "top to bottom",
        Direction::Down => "left to right",
    };
    if frame.small_button(turn, ids::GRAPH_TURN.target()).clicked() {
        frame.push(Action::Graph(GraphAction::Turn));
    }
    let mut runs = Vec::new();
    if let Some(tour) = model.graph.built().tour.and_then(|tour| model.tour(tour)) {
        runs.extend([
            Run::new(format!("{}: ", tour.name()), TEXT),
            Run::new("\u{2500} step  ", GREEN),
            Run::new("\u{2500} revealed  ", WEAK),
            Run::new("\u{2500} call back up  ", ORANGE),
        ]);
    }
    runs.push(Run::new(
        "drag or scroll to pan, pinch or ctrl+wheel to zoom, drag a title to move a node",
        WEAK,
    ));
    frame.caption(runs);
    frame.finish();
    let canvas = frame.canvas(
        move |canvas, _| draw_scene(canvas, &scene),
        ids::GRAPH_CANVAS.id(),
    );
    if let Some(over) = canvas.rect() {
        graph_corner(frame, over, zoom, minimap);
    }
}

fn graph_corner(frame: &mut Frame<'_>, over: Rect, zoom: Zoom, minimap: Option<Minimap>) {
    frame.start(Container::CanvasCorner { over });
    frame.fill();
    if let Some(minimap) = minimap {
        let size = minimap.size();
        frame.start(Container::FillRow);
        frame.grow();
        let target = ids::GRAPH_MINIMAP.target();
        frame.custom(
            move |canvas, rect| minimap.draw(canvas, rect),
            Size::Exact(size.width),
            size.height,
            Some(target.id()),
        );
        frame.attach_tip(target);
        frame.finish();
    }
    frame.start(Container::FillRow);
    frame.grow();
    zoom_cluster(frame, zoom);
    frame.finish();
    frame.finish();
}

fn cluster_button(frame: &mut Frame<'_>, text: &Label, cells: Cells, target: Target) -> bool {
    let width = usize::try_from(cells.get()).unwrap_or(0);
    let text = text.as_str();
    let clicked = frame
        .small_button_sized(format!("{text:^width$}"), Some(cells), target)
        .clicked();
    frame.attach_tip(target);
    clicked
}

fn zoom_cluster(frame: &mut Frame<'_>, zoom: Zoom) {
    frame.start(Container::Cluster);
    let minus = Label::from(Icon::Remove);
    if cluster_button(frame, &minus, ZOOM_BUTTON, ids::GRAPH_ZOOM_OUT.target()) {
        frame.push(Action::Graph(GraphAction::WantZoom(ZoomStep::Out)));
    }
    let percent = Label::new(format!("{:.0}%", zoom.get() * 100.0));
    if cluster_button(
        frame,
        &percent,
        ZOOM_PERCENT,
        ids::GRAPH_ONE_TO_ONE.target(),
    ) {
        frame.push(Action::Graph(GraphAction::OneToOne));
    }
    let plus = Label::from(Icon::Add);
    if cluster_button(frame, &plus, ZOOM_BUTTON, ids::GRAPH_ZOOM_IN.target()) {
        frame.push(Action::Graph(GraphAction::WantZoom(ZoomStep::In)));
    }
    if cluster_button(
        frame,
        &Label::new("fit"),
        FIT_BUTTON,
        ids::GRAPH_FIT.target(),
    ) {
        frame.push(Action::Graph(GraphAction::WantFit));
    }
    frame.finish();
}
