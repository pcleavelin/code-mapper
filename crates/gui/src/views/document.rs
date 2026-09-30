use std::collections::BTreeMap;

use domain::{Change, Depth, FileId, Line, Path, PathDiff, Span, Step, Symbol, SymbolName};
use ui::{Count, Icon, Id, Label, Px, Run};

use crate::action::{Action, Collapse, ContextChange, Hide};
use crate::ids::{self, Control, Target};
use crate::model::{
    Context, Measured, Model, Numbered, PathSlot, StepKey, StepShape, StepSlot, StepView, Tab,
    ViewFlag,
};
use crate::nav::Scrolling;
use crate::text::{Counted, Noun, Tag};
use crate::theme::{
    ACCENT, COLLAPSE_ROOM, DANGER_GAP, FAINT, GREEN, HIDE_BUTTON, INDENT, INLINE_BUTTON, PENDING,
    PIXEL, RED, SLICE, TEXT, WEAK, WHOLE_BUTTON,
};
use crate::widgets::{Chosen, CodeBlock, Container, Frame, Marks, Padding, Scroller, Width};
use std::mem;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Occurrence(usize);

struct Walk<'walk> {
    base: Count,
    prefix: Label,
    diff: Option<&'walk PathDiff>,
    chain: Vec<PathSlot>,
    occurrence: Option<Occurrence>,
    next: Count,
}

fn element(control: Control, occurrence: Option<Occurrence>, step: StepSlot) -> Target {
    let number = Count::new(step.get());
    match occurrence {
        None => control.nth(number),
        Some(Occurrence(nested)) => control.nested(Count::new(nested), number),
    }
}

fn code_id(occurrence: Option<Occurrence>, step: StepSlot) -> Id {
    let name = ids::DOCUMENT_CODE.as_str();
    match occurrence {
        None => Id::from_name(name).nth(step.get()),
        Some(Occurrence(nested)) => ids::linked().nth(nested).with(name).nth(step.get()),
    }
}

fn track_steps(
    model: &Model,
    frame: &mut Frame<'_>,
    numbered: &[Numbered],
    offset: &mut Px,
) -> Option<StepSlot> {
    let mut top_step = model.nav.top_step();
    let Some(placement) = frame.ui.placement(ids::document()) else {
        return top_step;
    };
    let area = placement.rect;
    let mut top = None;
    for step in numbered {
        if let Some(rect) = frame
            .ui
            .interaction(ids::STEP_HEADER.id().nth(step.step.get()))
            .rect()
            && (top.is_none() || rect.top <= area.top + PIXEL)
        {
            top = Some(step.step);
        }
    }
    if top.is_some() {
        top_step = top;
        frame.push(Action::TopStep(top));
    }
    if let Some(request) = model.nav.scroll_to_step() {
        let header = frame
            .ui
            .interaction(ids::STEP_HEADER.id().nth(request.step.get()))
            .rect();
        match header {
            Some(rect) => {
                *offset = (*offset + (rect.top - area.top)).max(Px::ZERO);
                frame.push(Action::ScrolledToStep(request.ticket, None));
            }
            None if request.tries.left() => {
                frame.push(Action::ScrolledToStep(
                    request.ticket,
                    Some(request.tries.fewer()),
                ));
            }
            None => frame.push(Action::ScrolledToStep(request.ticket, None)),
        }
    }
    top_step
}

fn header_bar(frame: &mut Frame<'_>, path: PathSlot, found: &Path, diff: Option<&PathDiff>) {
    frame.start(Container::Toolbar);
    frame.title(found.name().as_str());
    frame.label(
        format!(
            "[{}]{}  {}",
            Tag::kind(found.kind()),
            Tag::author(found.author()),
            Counted::new(Count::new(found.steps().len()), Noun::Step)
        ),
        WEAK,
    );
    if let Some(group) = found.group() {
        frame.label(format!("in {group}"), WEAK);
    }
    match diff.map(PathDiff::change) {
        Some(Change::Added) => frame.label("new since the parent revision", GREEN),
        Some(Change::Changed) => frame.label("changed since the parent revision", GREEN),
        _ => {}
    }
    frame.grow();
    let buttons = [
        ("graph", ids::SHOW_GRAPH, Action::OpenPath(path, Tab::Graph)),
        (
            "hide all code",
            ids::HIDE_ALL_CODE,
            Action::HideAll(path, Hide::Hide),
        ),
        (
            "show all code",
            ids::SHOW_ALL_CODE,
            Action::HideAll(path, Hide::Show),
        ),
        (
            "collapse all",
            ids::COLLAPSE_ALL,
            Action::CollapseAll(path, Collapse::Collapse),
        ),
        (
            "expand all",
            ids::EXPAND_ALL,
            Action::CollapseAll(path, Collapse::Expand),
        ),
    ];
    for (label, control, action) in buttons {
        if frame.small_button(label, control.target()).clicked() {
            frame.push(action);
        }
    }
    frame.cells_gap(DANGER_GAP);
    if frame
        .danger_button("delete path", ids::REMOVE_PATH.target())
        .clicked()
    {
        frame.push(Action::RemovePath(path));
    }
    frame.finish();
}

fn linked_from(model: &Model, frame: &mut Frame<'_>, found: &Path) {
    let from = model.map.links_to(found.name());
    if from.is_empty() {
        return;
    }
    frame.start(Container::ToolbarTight);
    frame.label("linked from", WEAK);
    for (position, address) in from.iter().enumerate() {
        let Some(source) = model.find_path(&address.path) else {
            continue;
        };
        let Some(step) = model.step_slot(source, &address.step) else {
            continue;
        };
        let key = StepKey { path: source, step };
        let number = model.number_of(key);
        if frame
            .small_button(
                format!("{} {}", address.path, number.as_str()),
                ids::LINKED_FROM.nth(Count::new(position)),
            )
            .clicked()
        {
            frame.push(Action::SelectStep(key, Scrolling::Scroll));
        }
    }
    frame.finish();
}

fn breadcrumb(
    model: &Model,
    frame: &mut Frame<'_>,
    path: PathSlot,
    numbered: &[Numbered],
    top_step: Option<StepSlot>,
) {
    let number_of: BTreeMap<StepSlot, Label> = numbered
        .iter()
        .map(|step| (step.step, step.number.clone()))
        .collect();
    frame.start(Container::Breadcrumb);
    let mut chain: Vec<StepSlot> = Vec::new();
    let count = model.step_count(path).get();
    let mut current = top_step.filter(|step| step.get() < count);
    while let Some(step) = current {
        chain.push(step);
        current = model
            .parent_of(StepKey { path, step })
            .filter(|parent| parent.get() < count && !chain.contains(parent));
    }
    if chain.is_empty() {
        frame.label(" ", WEAK);
    }
    for (position, step) in chain.iter().rev().enumerate() {
        if position > 0 {
            frame.label("\u{203a}", WEAK);
        }
        let key = StepKey { path, step: *step };
        let Some(found_step) = model.step(key) else {
            continue;
        };
        let name = found_step.symbol().map_or("(lines)", SymbolName::as_str);
        let file = found_step.file().as_str().rsplit('/').next().unwrap_or("");
        let crumb = format!("{} {name}", number_of.get(step).map_or("", Label::as_str));
        let id = ids::CRUMB.nth(Count::new(step.get()));
        let hovered = frame.ui.interaction(id.id()).hovered();
        let runs = vec![
            Run::new(crumb, if hovered { ACCENT } else { TEXT }),
            Run::new(format!(" {file}"), FAINT),
        ];
        if frame.link_text(runs, id, Chosen::Plain).clicked() {
            frame.push(Action::SelectStep(key, Scrolling::Scroll));
        }
    }
    frame.finish();
}

fn removed_steps(frame: &mut Frame<'_>, diff: Option<&PathDiff>) {
    let Some(diff) = diff.filter(|diff| !diff.removed().is_empty()) else {
        return;
    };
    frame.label("steps removed since the parent revision:", WEAK);
    for step in diff.removed() {
        let note = step
            .note()
            .map_or_else(String::new, |note| format!("  -- {note}"));
        frame.label(
            format!(
                "- {} {}{note}",
                step.file(),
                step.symbol().map_or("", SymbolName::as_str)
            ),
            WEAK,
        );
    }
}

pub(super) fn path_document(model: &Model, frame: &mut Frame<'_>) {
    let Some(path) = model
        .nav
        .path()
        .filter(|path| path.get() < model.path_count().get())
    else {
        frame.label(
            if model.map.paths().is_empty() {
                "no paths yet: the agent writes them (path-new, path-add in the console)"
            } else {
                "pick a path on the left"
            },
            WEAK,
        );
        return;
    };
    let Some(found) = model.path(path) else {
        return;
    };
    let diffs = model.diffs();
    let diff = diffs.iter().find(|diff| diff.name() == found.name());
    let numbered = model.numbered(path);
    let mut offset = model.scrolls.get(ids::document());
    let top_step = track_steps(model, frame, &numbered, &mut offset);
    header_bar(frame, path, found, diff);
    match found.note() {
        Some(note) => frame.note(note.as_str(), TEXT, Padding::Path),
        None => frame.note("(no path note)", WEAK, Padding::Path),
    }
    linked_from(model, frame, found);
    breadcrumb(model, frame, path, &numbered, top_step);
    frame.scroll_column(ids::document(), offset, Scroller::Document, None);
    if found.steps().is_empty() {
        frame.note(
            "No steps yet. Select lines in the Source view and press 'add step', or press '+ step' on a row in Symbols or References, or on a node in the Graph. Drag steps in the Paths steps list to rearrange them.",
            WEAK,
            Padding::Path,
        );
    }
    let mut walk = Walk {
        base: Count::ZERO,
        prefix: Label::default(),
        diff,
        chain: vec![path],
        occurrence: None,
        next: Count::ZERO,
    };
    steps(model, frame, path, &mut walk);
    removed_steps(frame, diff);
    frame.finish();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum First {
    Pending,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gone {
    File,
    Symbol,
}

struct Row<'row> {
    key: StepKey,
    step: &'row Step,
    number: Label,
    indent: Px,
    chosen: Chosen,
    view: StepView,
    occurrence: Option<Occurrence>,
    file: Option<FileId>,
    symbol_span: Option<Span>,
    gone: Option<Gone>,
}

impl Row<'_> {
    fn target(&self, control: Control) -> Target {
        element(control, self.occurrence, self.key.step)
    }

    fn has(&self, flag: ViewFlag) -> bool {
        self.view.flags.has(flag)
    }
}

fn title_runs(row: &Row<'_>, frame: &Frame<'_>, walk: &Walk<'_>) -> Vec<Run> {
    let step = row.step;
    let span = step.span();
    let name = step.symbol().map_or("(lines)", SymbolName::as_str);
    let place = match row.gone {
        Some(Gone::File) => format!("{} (file gone)", step.file()),
        Some(Gone::Symbol) => format!("{} (symbol gone)", step.file()),
        None => format!(
            "{}:{}-{}",
            step.file(),
            span.start().number(),
            span.end().number()
        ),
    };
    let stale = step.is_stale();
    let title = format!(
        "{}  {}{name}  ",
        row.number.as_str(),
        if stale { "STALE " } else { "" }
    );
    let hovered = frame
        .ui
        .interaction(row.target(ids::STEP_HEADER).id())
        .hovered();
    let title_color = if stale {
        RED
    } else if hovered {
        ACCENT
    } else {
        TEXT
    };
    let mut runs = vec![
        Run::new(title, title_color),
        Run::new(place, WEAK),
        Run::new(Tag::author(step.author()).to_string(), WEAK),
    ];
    if let Some(change) = walk
        .diff
        .filter(|diff| diff.change() == Change::Changed)
        .and_then(|diff| diff.steps().iter().find(|found| found.step == *step.id()))
        .and_then(|found| found.change)
    {
        runs.push(Run::new(format!("  {}", Tag::change(change)), GREEN));
    }
    runs
}

fn link_buttons(
    model: &Model,
    frame: &mut Frame<'_>,
    row: &Row<'_>,
    walk: &Walk<'_>,
) -> Option<PathSlot> {
    let link = row.step.link()?;
    let Some(target) = model.find_path(link) else {
        frame.label(format!("\u{2192} {link} (missing)"), RED);
        return None;
    };
    if frame
        .small_button(format!("\u{2192} {link}"), row.target(ids::LINK))
        .clicked()
    {
        frame.push(Action::OpenPath(target, Tab::Path));
    }
    if walk.chain.contains(&target) {
        frame.label("inlined above", WEAK);
    } else {
        let label = if row.has(ViewFlag::Inlined) {
            "un-inline"
        } else {
            "inline"
        };
        if frame
            .small_button_sized(label, Some(INLINE_BUTTON), row.target(ids::INLINE))
            .clicked()
        {
            frame.push(Action::Toggle(row.key, ViewFlag::Inlined));
        }
    }
    Some(target)
}

fn step_header(
    model: &Model,
    frame: &mut Frame<'_>,
    row: &Row<'_>,
    walk: &Walk<'_>,
    first: &mut First,
) -> Option<PathSlot> {
    let key = row.key;
    let collapsed = row.has(ViewFlag::Collapsed);
    let hidden = row.has(ViewFlag::Hidden);
    let children = model.descendants(key);
    frame.start(Container::StepRow {
        selected: row.chosen,
    });
    frame.indent(row.indent);
    if children.get() > 0 {
        let arrow = if collapsed {
            Icon::Collapsed
        } else {
            Icon::Expanded
        };
        if frame
            .small_button(arrow, row.target(ids::COLLAPSE))
            .clicked()
        {
            frame.push(Action::Toggle(key, ViewFlag::Collapsed));
        }
    } else {
        frame.cells_gap(COLLAPSE_ROOM);
    }
    let mut runs = title_runs(row, frame, walk);
    if collapsed {
        runs.insert(3, Run::new(format!("  +{children}"), WEAK));
    }
    if row.occurrence.is_some() && mem::replace(first, First::Done) == First::Pending {
        let name = model
            .path(key.path)
            .map_or_else(String::new, |path| path.name().to_string());
        runs.push(Run::new(format!("  in {name}"), WEAK));
    }
    if frame
        .link_text(runs, row.target(ids::STEP_HEADER), row.chosen)
        .clicked()
    {
        let scrolling = if row.occurrence.is_none() {
            Scrolling::Stay
        } else {
            Scrolling::Scroll
        };
        frame.push(Action::SelectStep(key, scrolling));
    }
    if row.gone.is_none()
        && frame
            .small_button_sized(
                if hidden { "code" } else { "hide code" },
                Some(HIDE_BUTTON),
                row.target(ids::HIDE_CODE),
            )
            .clicked()
    {
        frame.push(Action::Toggle(key, ViewFlag::Hidden));
    }
    if !hidden
        && row
            .symbol_span
            .is_some_and(|symbol| symbol != row.step.span())
        && frame
            .small_button_sized(
                if row.has(ViewFlag::Whole) {
                    "slice"
                } else {
                    "whole symbol"
                },
                Some(WHOLE_BUTTON),
                row.target(ids::WHOLE),
            )
            .clicked()
    {
        frame.push(Action::Toggle(key, ViewFlag::Whole));
    }
    if !row.view.context.is_empty()
        && frame
            .small_button("no context", row.target(ids::NO_CONTEXT))
            .clicked()
    {
        frame.push(Action::Context(key, ContextChange::Reset));
    }
    let target = link_buttons(model, frame, row, walk);
    frame.grow();
    if row.occurrence.is_none()
        && frame
            .danger_button("delete", row.target(ids::REMOVE_STEP))
            .clicked()
    {
        frame.push(Action::RemoveStep(key));
    }
    frame.finish();
    target
}

fn step_note(frame: &mut Frame<'_>, row: &Row<'_>) {
    frame.start(Container::FillRow);
    frame.indent(row.indent + COLLAPSE_ROOM.of(frame.cell_width()));
    match row.step.note() {
        Some(note) => frame.note(note.as_str(), GREEN, Padding::Step),
        None => frame.note("(no note)", PENDING, Padding::Step),
    }
    frame.finish();
}

fn step_code(model: &Model, frame: &mut Frame<'_>, row: &Row<'_>) {
    let (Some(file), None, false) = (row.file, row.gone, row.has(ViewFlag::Hidden)) else {
        return;
    };
    let span = row.step.span();
    let shown = if row.has(ViewFlag::Whole) {
        row.symbol_span.unwrap_or(span)
    } else {
        span
    };
    let context = row.view.context;
    let last = model
        .index
        .file(file)
        .and_then(|source| source.text().count().last())
        .unwrap_or(Line::new(0));
    let start = Line::new(shown.start().value().saturating_sub(context.above.value()));
    let end = Line::new(shown.end().value() + context.below.value()).min(last);
    let marked = Span::new(start, end) != Some(span);
    frame.start(Container::CodeColumn {
        selected: row.chosen,
    });
    if start.value() > 0 {
        context_button(frame, row, ContextChange::Above, ids::CONTEXT_ABOVE);
    }
    frame.start(Container::FillRow);
    frame.indent(row.indent);
    let background = |line: Line| (marked && span.contains(line)).then_some(SLICE);
    let bar = |_: Line| None;
    frame.code_block(
        model,
        &CodeBlock {
            file,
            start,
            end,
            id: code_id(row.occurrence, row.key.step),
            width: Width::Wide,
            marks: Marks {
                background: &background,
                bar: &bar,
            },
        },
    );
    frame.finish();
    if end < last {
        context_button(frame, row, ContextChange::Below, ids::CONTEXT_BELOW);
    }
    frame.finish();
}

fn step_column(occurrence: Option<Occurrence>, step: StepSlot) -> Id {
    let name = ids::STEP_COLUMN.as_str();
    match occurrence {
        None => Id::from_name(name).nth(step.get()),
        Some(Occurrence(nested)) => ids::linked().nth(nested).with(name).nth(step.get()),
    }
}

fn near(last: Option<ui::Rect>, view: Option<ui::Rect>) -> bool {
    match (last, view) {
        (Some(rect), Some(view)) => {
            rect.bottom() >= view.top - view.height && rect.top <= view.bottom() + view.height
        }
        _ => true,
    }
}

fn step_or_reserve(
    model: &Model,
    frame: &mut Frame<'_>,
    row: &Row<'_>,
    walk: &Walk<'_>,
    first: &mut First,
    view: Option<ui::Rect>,
) -> Option<PathSlot> {
    let id = step_column(row.occurrence, row.key.step);
    let shape = StepShape {
        view: row.view,
        span: row.gone.is_none().then(|| row.step.span()),
        note: Count::new(
            row.step
                .note()
                .map_or(0, |note| note.as_str().chars().count()),
        ),
        width: view.map_or(Px::ZERO, |view| view.width),
        row: frame.row_height(),
    };
    let last = frame.ui.interaction(id).rect();
    let pinned = row.chosen == Chosen::Chosen
        || (row.occurrence.is_none()
            && model
                .nav
                .scroll_to_step()
                .is_some_and(|request| request.step == row.key.step));
    let culled = model
        .measures
        .height(id, shape)
        .filter(|_| !pinned && !near(last, view));
    if let Some(height) = culled {
        frame.reserve(id, height);
        if row.occurrence.is_some() {
            *first = First::Done;
        }
        row.step.link().and_then(|link| model.find_path(link))
    } else {
        frame.start(Container::StepColumn(id));
        let target = step_header(model, frame, row, walk, first);
        step_note(frame, row);
        step_code(model, frame, row);
        frame.step_gap();
        frame.finish();
        if let Some(rect) = last {
            frame.push(Action::Measured(
                id,
                Measured {
                    shape,
                    height: rect.height,
                },
            ));
        }
        target
    }
}

fn steps(model: &Model, frame: &mut Frame<'_>, path: PathSlot, walk: &mut Walk<'_>) {
    let occurrence = walk.occurrence;
    let mut hide_below: Option<Depth> = None;
    let mut first = First::Pending;
    let view = frame
        .ui
        .placement(ids::document())
        .map(|placement| placement.rect);
    for numbered in model.numbered(path) {
        if hide_below.is_some_and(|depth| numbered.depth > depth) {
            continue;
        }
        hide_below = None;
        let key = StepKey {
            path,
            step: numbered.step,
        };
        let Some(step) = model.step(key) else {
            continue;
        };
        let depth = usize::try_from(numbered.depth.value()).unwrap_or(0);
        let file = model.index.find_file(step.file());
        let symbol_span = file
            .and(step.resolved_symbol())
            .and_then(|symbol| model.index.symbol(symbol))
            .map(Symbol::span);
        let chosen = if occurrence.is_none() && model.nav.step() == Some(numbered.step) {
            Chosen::Chosen
        } else {
            Chosen::Plain
        };
        let row = Row {
            key,
            step,
            number: Label::new(format!(
                "{}{}",
                walk.prefix.as_str(),
                numbered.number.as_str()
            )),
            indent: INDENT.of(frame.cell_width())
                * i32::try_from(walk.base.get() + depth).unwrap_or(0),
            chosen,
            view: model.views.get(key),
            occurrence,
            file,
            symbol_span,
            gone: match (file, step.symbol(), symbol_span) {
                (None, _, _) => Some(Gone::File),
                (Some(_), Some(_), None) => Some(Gone::Symbol),
                _ => None,
            },
        };
        let target = step_or_reserve(model, frame, &row, walk, &mut first, view);
        let collapsed = row.has(ViewFlag::Collapsed);
        if collapsed {
            hide_below = Some(numbered.depth);
        }
        if let Some(target) = target.filter(|target| {
            row.has(ViewFlag::Inlined) && !collapsed && !walk.chain.contains(target)
        }) {
            inline(model, frame, walk, &row, target, Count::new(depth));
        }
    }
}

fn inline(
    model: &Model,
    frame: &mut Frame<'_>,
    walk: &mut Walk<'_>,
    row: &Row<'_>,
    target: PathSlot,
    depth: Count,
) {
    walk.next += Count::new(1);
    let nested = Occurrence(walk.next.get());
    let saved_base = walk.base;
    let saved_prefix = mem::replace(
        &mut walk.prefix,
        Label::new(format!("{} \u{203a} ", row.number.as_str())),
    );
    let saved_diff = walk.diff.take();
    let saved_occurrence = walk.occurrence.replace(nested);
    walk.base = Count::new(saved_base.get() + depth.get() + 1);
    walk.chain.push(target);
    steps(model, frame, target, walk);
    walk.chain.pop();
    walk.base = saved_base;
    walk.prefix = saved_prefix;
    walk.diff = saved_diff;
    walk.occurrence = saved_occurrence;
}

fn context_button(frame: &mut Frame<'_>, row: &Row<'_>, change: ContextChange, control: Control) {
    let label = match change {
        ContextChange::Above => format!(
            "{} {} lines above",
            Icon::MoreAbove.glyph().get(),
            Context::LINES
        ),
        ContextChange::Below | ContextChange::Reset => {
            format!(
                "{} {} lines below",
                Icon::MoreBelow.glyph().get(),
                Context::LINES
            )
        }
    };
    frame.start(Container::FillRow);
    frame.indent(row.indent);
    if frame.small_button(label, row.target(control)).clicked() {
        frame.push(Action::Context(row.key, change));
    }
    frame.finish();
}
