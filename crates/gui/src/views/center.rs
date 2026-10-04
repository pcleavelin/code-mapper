use domain::{Change, Comment, FileId, Line, RelativePath, Span, StepChange, SymbolName, TourDiff};
use ui::{Axis, Count, Icon, Id, Label, Px, Rect, Run, Scrollbar, Size};

use crate::action::Action;
use crate::comments::{DraftOn, InsetAt};
use crate::field::Which;
use crate::graph::{GraphAction, GraphFrame, Hit, Minimap, draw_scene};
use crate::grid::GUTTER;
use crate::ids::{self, Target};
use crate::keys::{self, LineGesture};
use crate::model::{Gone, HIT_LIMIT, HitsShown, Model, StepKey, Tab, TourSlot};
use crate::panels::{Direction, View};
use crate::status::Status;
use crate::text::{Counted, Noun, Tag};
use crate::theme::{
    COMMENT_ANSWERED_FILL, COMMENT_OPEN_FILL, Cells, DRAFT_FRAME, DRAFT_ROWS, EDGE_SCROLL_BAND,
    EDGE_SCROLL_MOST, FIELD, FIT_BUTTON, GREEN, LINE_FIELD, LINES_SELECTED, ORANGE, PIXEL, RED,
    ROW_EXTRA, SEARCH_FIELD, SOURCE_GUESS, TEXT, WEAK, ZOOM_BUTTON, ZOOM_PERCENT, Zoom, ZoomStep,
};
use crate::widgets::{
    AcrossBar, AcrossScroll, Chosen, CodeBlock, Coded, Container, Frame, Marks, Padding, Scroller,
    Width,
};

use super::{authoring, comments};

fn source_toolbar(model: &Model, frame: &mut Frame<'_>, path: &RelativePath) {
    frame.start(Container::Toolbar);
    frame.label(path.as_str(), TEXT);
    frame.label("line", WEAK);
    frame.field(
        &model.fields,
        Which::GoToLine,
        &ui::Label::default(),
        LINE_FIELD,
    );
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

enum InsetKind<'inset> {
    Comment(&'inset Comment),
    Draft(&'inset DraftOn),
}

struct Inset<'inset> {
    at: InsetAt,
    height: Px,
    kind: InsetKind<'inset>,
}

fn rows_to(line: Line) -> Count {
    Count::new(usize::try_from(line.value()).unwrap_or(0))
}

impl Inset<'_> {
    fn row_id(&self) -> Id {
        match self.kind {
            InsetKind::Comment(comment) => comments::box_id(comment).with("row"),
            InsetKind::Draft(_) => comments::draft_id().with("row"),
        }
    }

    fn above(&self, line: Line) -> bool {
        match self.at {
            InsetAt::Top => true,
            InsetAt::After(after) => after < line,
        }
    }

    fn rows_above(&self) -> Count {
        match self.at {
            InsetAt::Top => Count::ZERO,
            InsetAt::After(after) => rows_to(after) + Count::new(1),
        }
    }
}

struct Start {
    line: Line,
    inset: Count,
    top: Px,
}

enum Piece<'plan, 'inset> {
    Inset(&'plan Inset<'inset>),
    Code { id: Id, start: Line, end: Line },
}

struct Plan<'plan, 'inset> {
    pieces: Vec<Piece<'plan, 'inset>>,
    drawn: Px,
}

struct Insets<'inset> {
    all: Vec<Inset<'inset>>,
    row: Px,
}

impl<'inset> Insets<'inset> {
    fn of(model: &'inset Model, frame: &Frame<'_>, file: FileId, last: Line) -> Self {
        let row = frame.row_height();
        let measured = |id: Id| {
            frame
                .ui
                .placement(id)
                .map(|placement| placement.rect.height)
        };
        let within = |at: InsetAt| match at {
            InsetAt::Top => InsetAt::Top,
            InsetAt::After(after) => InsetAt::After(after.min(last)),
        };
        let mut all: Vec<Inset<'inset>> = model
            .shelf
            .in_file(&model.index, file)
            .map(|comment| {
                let mut inset = Inset {
                    at: within(InsetAt::of(comment)),
                    height: comments::estimate(comment, row),
                    kind: InsetKind::Comment(comment),
                };
                if let Some(height) = measured(inset.row_id()) {
                    inset.height = height;
                }
                inset
            })
            .collect();
        if let Some(draft @ DraftOn::Lines { file: on, span }) = model.shelf.draft.as_ref()
            && *on == file
        {
            let mut inset = Inset {
                at: within(InsetAt::After(span.end())),
                height: row * DRAFT_ROWS + DRAFT_FRAME,
                kind: InsetKind::Draft(draft),
            };
            if let Some(height) = measured(inset.row_id()) {
                inset.height = height;
            }
            all.push(inset);
        }
        all.sort_by_key(|inset| inset.at);
        Self { all, row }
    }

    fn line_top(&self, line: Line) -> Px {
        let above: Px = self
            .all
            .iter()
            .filter(|inset| inset.above(line))
            .fold(Px::ZERO, |sum, inset| sum + inset.height);
        self.row * rows_to(line) + above
    }

    fn height(&self, lines: Count) -> Px {
        self.all
            .iter()
            .fold(self.row * lines, |sum, inset| sum + inset.height)
    }

    fn start(&self, offset: Px, last: Line) -> Start {
        let row = self.row;
        let line_at = |extra: Px| {
            Line::new(u32::try_from((offset - extra).ratio(row).max(0)).unwrap_or(0)).min(last)
        };
        let mut extra = Px::ZERO;
        for (position, inset) in self.all.iter().enumerate() {
            let rows = inset.rows_above();
            let candidate = line_at(extra);
            if rows_to(candidate) < rows {
                return Start {
                    line: candidate,
                    inset: Count::new(position),
                    top: row * rows_to(candidate) + extra,
                };
            }
            if offset < row * rows + extra + inset.height {
                let before = Count::new(rows.get().saturating_sub(1));
                return Start {
                    line: Line::new(u32::try_from(before.get()).unwrap_or(0)),
                    inset: Count::new(position),
                    top: if rows == Count::ZERO {
                        extra
                    } else {
                        row * before + extra
                    },
                };
            }
            extra += inset.height;
        }
        let line = line_at(extra);
        Start {
            line,
            inset: Count::new(self.all.len()),
            top: row * rows_to(line) + extra,
        }
    }

    fn plan(&self, start: &Start, bottom: Px, last: Line, lines: Count) -> Plan<'_, 'inset> {
        let row = self.row;
        let mut pieces = Vec::new();
        let mut drawn = start.top;
        let mut next = start.inset.get();
        let mut line = start.line;
        loop {
            while let Some(shown) = self.all.get(next).filter(|shown| shown.above(line)) {
                pieces.push(Piece::Inset(shown));
                drawn += shown.height;
                next += 1;
            }
            if lines == Count::ZERO || line > last || drawn >= bottom {
                break;
            }
            let fit = u32::try_from((bottom - drawn).ratio(row).max(0) + 1).unwrap_or(1);
            let mut end = Line::new(line.value().saturating_add(fit - 1)).min(last);
            if let Some(InsetAt::After(after)) = self.all.get(next).map(|inset| inset.at) {
                end = end.min(after.max(line));
            }
            pieces.push(Piece::Code {
                id: segment_id(Count::new(next)),
                start: line,
                end,
            });
            drawn += row * Count::new(rows_to(end).get() + 1 - rows_to(line).get());
            line = Line::new(end.value() + 1);
        }
        Plan { pieces, drawn }
    }

    fn segment_ids(&self) -> impl Iterator<Item = Id> {
        (0..=self.all.len()).map(|before| segment_id(Count::new(before)))
    }
}

fn segment_id(before: Count) -> Id {
    if before == Count::ZERO {
        ids::LINES.id()
    } else {
        ids::LINES.id().nth(before.get())
    }
}

fn inset(model: &Model, frame: &mut Frame<'_>, inset: &Inset<'_>, width: Option<Px>) {
    let gutter = frame.cell_width() * GUTTER;
    frame.start(Container::CommentRow(inset.row_id()));
    frame.indent(gutter);
    match inset.kind {
        InsetKind::Comment(comment) => {
            comments::comment_box(frame, comment, Gone::of(model, comment));
        }
        InsetKind::Draft(draft) => {
            comments::draft_box(model, frame, draft, width.map(|width| width - gutter));
        }
    }
    frame.finish();
}

struct Segment {
    id: Id,
    start: Line,
    end: Line,
    coded: Coded,
}

fn line_under_mouse(frame: &Frame<'_>, segments: &[Segment]) -> Option<Line> {
    let mouse = frame.ui.pointer().mouse.vertical;
    let row = frame.row_height();
    segments.iter().rev().find_map(|segment| {
        let rect = frame.ui.placement(segment.id)?.rect;
        (mouse >= rect.top).then(|| {
            let down = u32::try_from((mouse - rect.top).ratio(row).max(0)).unwrap_or(0);
            Line::new(segment.start.value().saturating_add(down)).min(segment.end)
        })
    })
}

fn select_lines(
    model: &Model,
    frame: &mut Frame<'_>,
    segments: &[Segment],
    scrollbar: Option<Scrollbar>,
) {
    let pointer = frame.ui.pointer();
    let on_scrollbar = scrollbar.is_some_and(|shown| shown.track.contains(pointer.mouse));
    let mut acted = false;
    for segment in segments {
        let coded = &segment.coded;
        match (
            keys::selects_line(coded.interaction, pointer),
            coded.spot,
            coded.dragged_to,
        ) {
            (Some(LineGesture::Press), Some(spot), _) if !on_scrollbar => {
                frame.push(Action::SelectLine(spot.line, LineGesture::Press));
                acted = true;
            }
            (Some(LineGesture::Drag), _, Some(line)) => {
                let line = line_under_mouse(frame, segments).unwrap_or(line);
                frame.push(Action::SelectLine(line, LineGesture::Drag));
                acted = true;
            }
            _ => {}
        }
    }
    let clicked = segments
        .iter()
        .any(|segment| segment.coded.interaction.clicked());
    let down = segments
        .iter()
        .any(|segment| segment.coded.interaction.down());
    if !acted && model.line_grab.is_some() && (clicked || !down) {
        frame.push(Action::ClearLineSelection);
    }
}

fn draw_plan(
    model: &Model,
    frame: &mut Frame<'_>,
    file: FileId,
    plan: &Plan<'_, '_>,
    marks: &Marks<'_>,
    width: Option<Px>,
) -> Vec<Segment> {
    let columns = plan
        .pieces
        .iter()
        .filter_map(|piece| match piece {
            Piece::Code { start, end, .. } => Span::new(*start, *end),
            Piece::Inset(_) => None,
        })
        .fold(Count::ZERO, |widest, span| {
            widest.max(frame.code_columns(model, file, span))
        });
    let last_code = plan
        .pieces
        .iter()
        .rposition(|piece| matches!(piece, Piece::Code { .. }));
    let mut segments = Vec::new();
    for (position, piece) in plan.pieces.iter().enumerate() {
        match piece {
            Piece::Inset(shown) => inset(model, frame, shown, width),
            Piece::Code { id, start, end } => {
                let bar = if last_code == Some(position) {
                    AcrossBar::Drawn
                } else {
                    AcrossBar::Omitted
                };
                let coded = frame.code_block(
                    model,
                    &CodeBlock {
                        file,
                        start: *start,
                        end: *end,
                        id: *id,
                        width: Width::Wide,
                        across: AcrossScroll::Shared {
                            offset: ids::LINES.id(),
                            columns,
                            bar,
                        },
                        marks: Marks {
                            background: marks.background,
                            bar: marks.bar,
                        },
                    },
                );
                segments.push(Segment {
                    id: *id,
                    start: *start,
                    end: *end,
                    coded,
                });
            }
        }
    }
    segments
}

fn scroll_to_request(
    model: &Model,
    frame: &mut Frame<'_>,
    insets: &Insets<'_>,
    total: Count,
) -> Option<Px> {
    let request = model.nav.scroll_to()?;
    let height = frame
        .ui
        .placement(ids::source())
        .map_or(SOURCE_GUESS, |placement| placement.rect.height);
    let above = insets.line_top(Line::new(request.line.value().saturating_sub(3)));
    frame.push(Action::ScrolledToLine(request.ticket));
    Some(above.min((insets.height(total) - height).max(Px::ZERO)))
}

pub(super) fn source(model: &Model, frame: &mut Frame<'_>) {
    let Some(file) = model.nav.file() else {
        frame.note("click a symbol to open its file", WEAK, Padding::Step);
        return;
    };
    let Some(source) = model.index.file(file) else {
        return;
    };
    let column_id = ids::source();
    let row_height = frame.row_height();
    let count = source.text().count();
    let last = count.last().unwrap_or(Line::new(0));
    let total = Count::new(usize::try_from(count.value()).unwrap_or(0));
    let insets = Insets::of(model, frame, file, last);
    let pointer = frame.ui.pointer();
    let column = frame.ui.placement(column_id);
    let mut offset = model.scrolls.get(column_id);
    if model.line_grab.is_some()
        && insets
            .segment_ids()
            .any(|segment| frame.ui.interaction(segment).down())
        && let Some(column) = column
    {
        offset += edge_scroll(pointer.mouse.vertical, column.rect, row_height);
    }
    if let Some(asked) = scroll_to_request(model, frame, &insets, total) {
        offset = asked;
    }
    source_toolbar(model, frame, source.path());
    authoring::target_strip(model, frame, View::Source);
    let scrolled = frame.scroll_column(column_id, offset, Scroller::Plain, Some(FIELD));
    let selection = model.nav.lines();
    let anchors: Vec<(Span, bool)> = model
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
    let commented: Vec<(Span, bool)> = model
        .shelf
        .in_file(&model.index, file)
        .filter_map(|comment| {
            comment
                .resolution()
                .map(|resolution| (resolution.span, comment.is_open()))
        })
        .collect();
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
            .or_else(|| {
                commented
                    .iter()
                    .find(|(span, _)| span.contains(line))
                    .map(|(_, open)| {
                        if *open {
                            COMMENT_OPEN_FILL
                        } else {
                            COMMENT_ANSWERED_FILL
                        }
                    })
            })
    };
    let view = scrolled
        .interaction
        .rect()
        .map_or(SOURCE_GUESS, |rect| rect.height);
    let width = scrolled.interaction.rect().map(|rect| rect.width);
    let bottom = scrolled.offset + view;
    let begin = insets.start(scrolled.offset, last);
    frame.spacer(begin.top);
    let plan = insets.plan(&begin, bottom, last, total);
    let marks = Marks {
        background: &background,
        bar: &bar,
    };
    let segments = draw_plan(model, frame, file, &plan, &marks, width);
    if total.get() > 0 {
        let scrollbar = column.and_then(|placement| placement.scrollbar(Axis::Vertical, offset));
        frame.attach_tip(ids::LINES.target());
        select_lines(model, frame, &segments, scrollbar);
    }
    frame.spacer((insets.height(total) - plan.drawn).max(Px::ZERO) + row_height);
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
    frame.start(Container::ToolbarSmall);
    frame.field(
        &model.fields,
        Which::Search,
        &Label::new("regex"),
        SEARCH_FIELD,
    );
    frame.finish();
    if model.hits.is_empty() {
        let search = model.fields.get(Which::Search).text();
        frame.note(
            if search.is_empty() {
                "type a regex in the field above and press enter"
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
            Run::new("\u{2500} call back up", ORANGE),
        ]);
    }
    frame.caption(runs);
    frame.finish();
    let canvas = frame.canvas(
        move |canvas, _| draw_scene(canvas, &scene),
        ids::GRAPH_CANVAS.id(),
    );
    canvas_tip(model, frame);
    if let Some(over) = canvas.rect() {
        graph_corner(frame, over, zoom, minimap);
    }
}

fn canvas_tip(model: &Model, frame: &mut Frame<'_>) {
    match model.graph.hit_at(frame.ui.pointer().mouse) {
        None => frame.attach_tip(ids::GRAPH_CANVAS.target()),
        Some(Hit::Header(_)) => frame.attach_tip(ids::GRAPH_NODE.on(ids::GRAPH_CANVAS)),
        Some(_) => {}
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
