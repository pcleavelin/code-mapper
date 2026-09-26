use domain::{Change, Depth, FileId, GroupName, PathCount, PathDiff, Row, Step, SymbolName};
use ui::{Count, Icon, Label, Px, Run};

use crate::action::Action;
use crate::field::Which;
use crate::ids;
use crate::model::{Model, Openness, PathSlot, StepKey, Tab, ViewFlag};
use crate::nav::Scrolling;
use crate::text::{Clipped, Tag};
use crate::theme::{
    Cells, FAINT, FILTER_FIELD, GREEN, OUTLINE_TOP, PANEL_TEXT_ROOM, PATHS_GUESS, PATHS_LEAST,
    PENDING, PIXEL, RED, ROW_EXTRA, SELECTED, SYMBOL_FIXED, SYMBOL_KIND, SYMBOL_NAME,
    SYMBOLS_GUESS, SYMBOLS_LEAST, TEXT, WEAK,
};
use crate::widgets::{Chosen, Container, Frame, Scroller};

fn columns(frame: &Frame<'_>, rect: Option<ui::Rect>, guess: Cells, least: Cells) -> Count {
    let cell = frame.cell_width().max(PIXEL);
    let columns = rect.map_or(guess.get(), |rect| {
        (rect.width - PANEL_TEXT_ROOM - Frame::scrollbar_width())
            .ratio(cell)
            .max(least.get())
    });
    Count::new(usize::try_from(columns).unwrap_or(0))
}

fn group_row(
    model: &Model,
    frame: &mut Frame<'_>,
    group: &GroupName,
    paths: PathCount,
    pad: &Label,
    reading: &Label,
    forced: Option<Openness>,
) -> Openness {
    let full = group.as_str();
    let reading = reading.as_str();
    let holds = reading == full || reading.starts_with(&format!("{full}/"));
    let open = forced
        .or_else(|| model.groups.get(group).copied())
        .map_or(holds, |openness| openness == Openness::Open);
    let label = format!(
        "{}{} {}/  {} paths",
        pad.as_str(),
        if open { Icon::Unfolded } else { Icon::Folded }
            .glyph()
            .get(),
        group.last_segment(),
        paths.value()
    );
    let stale = model.map.paths().iter().any(|path| {
        path.group().is_some_and(|inside| {
            inside.as_str() == full || inside.as_str().starts_with(&format!("{full}/"))
        }) && path.steps().iter().any(Step::is_stale)
    });
    let id = ids::GROUP_ROW.with(&Label::new(full));
    let runs = vec![Run::new(label, if stale { RED } else { TEXT })];
    if frame.row(runs, id, Chosen::Plain).clicked() && forced.is_none() {
        let openness = if open {
            Openness::Closed
        } else {
            Openness::Open
        };
        frame.push(Action::OpenGroup(group.clone(), openness));
    }
    if open {
        Openness::Open
    } else {
        Openness::Closed
    }
}

fn path_row(
    model: &Model,
    frame: &mut Frame<'_>,
    slot: PathSlot,
    diffs: &[PathDiff],
    pad: &Label,
    columns: Count,
) -> Chosen {
    let Some(path) = model.path(slot) else {
        return Chosen::Plain;
    };
    let name = path.name();
    let pad = pad.as_str();
    let steps = path.steps().len();
    let stale = path.steps().iter().filter(|step| step.is_stale()).count();
    let mark = match diffs
        .iter()
        .find(|diff| diff.name() == name)
        .map(PathDiff::change)
    {
        Some(Change::Added) => "+ ",
        Some(Change::Changed) => "~ ",
        _ => "",
    };
    let color = if stale > 0 {
        RED
    } else if !mark.is_empty() {
        GREEN
    } else {
        TEXT
    };
    let chosen = if model.nav.path() == Some(slot) {
        Chosen::Chosen
    } else {
        Chosen::Plain
    };
    let rest = format!(
        " [{}]{}  {steps} steps",
        Tag::kind(path.kind()),
        Tag::author(path.author())
    );
    let room = columns
        .get()
        .saturating_sub(pad.len() + mark.len() + rest.chars().count());
    let line = format!("{pad}{mark}{}{rest}", Clipped::right(name.as_str(), room));
    if frame
        .row(
            vec![Run::new(line, color)],
            ids::PATH_ROW.nth(Count::new(slot.get())),
            chosen,
        )
        .clicked()
    {
        frame.push(Action::OpenPath(slot, Tab::Path));
    }
    chosen
}

fn follow_outline(model: &Model, frame: &mut Frame<'_>, offset: Px) {
    let base = ids::paths();
    let (Some(step), Some(placement)) = (model.nav.top_step(), frame.ui.placement(base)) else {
        return;
    };
    if model.nav.outline_shown() == Some(step) || frame.ui.dragging() {
        return;
    }
    let view = placement.rect;
    if let Some(row) = frame
        .ui
        .interaction(ids::OUTLINE_ROW.id().nth(step.get()))
        .rect()
        && (row.top < view.top || row.bottom() > view.bottom())
    {
        let moved = (offset + (row.top - view.top) - view.height / 3).max(Px::ZERO);
        frame.push(Action::Scroll(base, moved));
    }
    frame.push(Action::OutlineShown(step));
}

pub(super) fn paths_window(model: &Model, frame: &mut Frame<'_>) {
    frame.start(Container::ToolbarSmall);
    frame.field(
        &model.fields,
        Which::PathFilter,
        &Label::new("filter"),
        FILTER_FIELD,
    );
    frame.finish();
    let forced = (!model
        .fields
        .get(Which::PathFilter)
        .text()
        .as_str()
        .is_empty())
    .then_some(Openness::Open);
    let diffs = model.diffs();
    let base = ids::paths();
    let scrolled = frame.scroll_column(base, model.scrolls.get(base), Scroller::Plain, None);
    let columns = columns(frame, scrolled.interaction.rect(), PATHS_GUESS, PATHS_LEAST);
    let reading = Label::new(
        model
            .nav
            .path()
            .and_then(|path| model.path(path))
            .and_then(|path| path.group())
            .map_or("", GroupName::as_str),
    );
    let mut closed_at: Option<Depth> = None;
    for row in model.listed_rows() {
        let depth = match &row {
            Row::Group { depth, .. } | Row::Path { depth, .. } => *depth,
        };
        if closed_at.is_some_and(|closed| depth > closed) {
            continue;
        }
        closed_at = None;
        let pad = Label::new("  ".repeat(usize::try_from(depth.value()).unwrap_or(0)));
        match row {
            Row::Group { group, paths, .. } => {
                if group_row(model, frame, &group, paths, &pad, &reading, forced)
                    == Openness::Closed
                {
                    closed_at = Some(depth);
                }
            }
            Row::Path { name, .. } => {
                let Some(slot) = model.find_path(&name) else {
                    continue;
                };
                if path_row(model, frame, slot, &diffs, &pad, columns) == Chosen::Chosen {
                    outline(model, frame, slot, &pad);
                    follow_outline(model, frame, scrolled.offset);
                }
            }
        }
    }
    for diff in diffs
        .iter()
        .filter(|diff| diff.change() == Change::Removed && model.lists(diff.name()))
    {
        frame.label(
            format!(
                "- {}  (removed, {} steps)",
                diff.name(),
                diff.removed().len()
            ),
            WEAK,
        );
    }
    frame.finish();
}

fn outline(model: &Model, frame: &mut Frame<'_>, path: PathSlot, pad: &Label) {
    let pad = pad.as_str();
    let mut hide_below: Option<Depth> = None;
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
        let hidden = if model.views.get(key).flags.has(ViewFlag::Folded) {
            hide_below = Some(numbered.depth);
            model.descendants(key)
        } else {
            Count::ZERO
        };
        let name = step.symbol().map_or("(lines)", SymbolName::as_str);
        let file = step.file().as_str().rsplit('/').next().unwrap_or("");
        let linked = step
            .link()
            .map_or_else(String::new, |link| format!("  \u{2192} {link}"));
        let indent = "  ".repeat(usize::try_from(numbered.depth.value()).unwrap_or(0));
        let more = if hidden.get() > 0 {
            format!("  +{hidden}")
        } else {
            String::new()
        };
        let line = format!(
            "{pad}  {indent}{}  {name}{linked}{more}",
            numbered.number.as_str()
        );
        let at_top = model.nav.top_step() == Some(numbered.step);
        let marked = if model.nav.step() == Some(numbered.step) {
            Some(SELECTED)
        } else if at_top {
            Some(OUTLINE_TOP)
        } else {
            None
        };
        let color = if step.is_stale() {
            RED
        } else if at_top {
            TEXT
        } else {
            WEAK
        };
        if frame
            .marked_row(
                vec![Run::new(line, color), Run::new(format!("  {file}"), FAINT)],
                ids::OUTLINE_ROW.nth(Count::new(numbered.step.get())),
                marked,
            )
            .clicked()
        {
            frame.push(Action::SelectStep(key, Scrolling::Scroll));
        }
    }
}

pub(super) fn symbols_window(model: &Model, frame: &mut Frame<'_>) {
    frame.start(Container::ToolbarSmall);
    frame.label("+ = in a path", WEAK);
    frame.field(
        &model.fields,
        Which::SymbolFilter,
        &Label::new("filter"),
        FILTER_FIELD,
    );
    frame.finish();
    let filter = model
        .fields
        .get(Which::SymbolFilter)
        .text()
        .as_str()
        .to_lowercase();
    let index = &model.index;
    let rows: Vec<domain::SymbolId> = index
        .symbol_ids()
        .filter(|symbol| {
            filter.is_empty()
                || index
                    .symbol(*symbol)
                    .is_some_and(|found| found.name().as_str().to_lowercase().contains(&filter))
                || index
                    .file(symbol.file())
                    .is_some_and(|file| file.path().as_str().to_lowercase().contains(&filter))
        })
        .collect();
    let id = ids::symbols();
    let scrolled = frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let row_height = frame.row_height() + ROW_EXTRA;
    let columns = columns(
        frame,
        scrolled.interaction.rect(),
        SYMBOLS_GUESS,
        SYMBOLS_LEAST,
    )
    .get();
    let count = Count::new(rows.len());
    let window = frame.rows_window(
        scrolled.offset,
        scrolled.interaction.rect(),
        count,
        row_height,
        Count::new(40),
    );
    for symbol_id in rows
        .iter()
        .skip(window.first.get())
        .take(window.visible.get())
    {
        let (Some(symbol), Some(file)) = (index.symbol(*symbol_id), index.file(symbol_id.file()))
        else {
            continue;
        };
        let covered = if model.map.covers(file.path(), symbol.span()) {
            "+"
        } else {
            " "
        };
        let indent = if symbol.depth().value() > 0 { "  " } else { "" };
        let place = format!("{}:{}", file.path(), symbol.span().start().number());
        let name_width = usize::try_from(SYMBOL_NAME.get()).unwrap_or(0);
        let kind_width = usize::try_from(SYMBOL_KIND.get()).unwrap_or(0);
        let fixed = usize::try_from(SYMBOL_FIXED.get()).unwrap_or(0);
        let line = format!(
            "{covered} {indent}{:<name_width$} {:<kind_width$} {}",
            Clipped::right(symbol.name().as_str(), name_width).to_string(),
            Clipped::right(symbol.kind().as_str(), kind_width).to_string(),
            Clipped::left(&place, columns.saturating_sub(fixed + indent.len()))
        );
        let color = if file.is_pending() { PENDING } else { TEXT };
        let chosen = if model.nav.focus() == Some(*symbol_id) {
            Chosen::Chosen
        } else {
            Chosen::Plain
        };
        let row_id = ids::SYMBOL_ROW.with(&Label::new(format!(
            "{}:{}",
            symbol_id.file(),
            symbol_id.symbol()
        )));
        if frame
            .row(vec![Run::new(line, color)], row_id, chosen)
            .clicked()
        {
            frame.push(Action::Focus(*symbol_id));
        }
    }
    frame.rows_after(count, &window, row_height, Px::ZERO);
    frame.finish();
}

struct Coverage {
    covered: Count,
    total: Count,
}

pub(super) fn files_window(model: &Model, frame: &mut Frame<'_>) {
    frame.start(Container::Header);
    frame.label("covered/total symbols", WEAK);
    frame.finish();
    let coverage: Vec<Coverage> = model
        .index
        .files()
        .map(|file| Coverage {
            covered: Count::new(
                file.symbols()
                    .filter(|symbol| model.map.covers(file.path(), symbol.span()))
                    .count(),
            ),
            total: Count::new(file.symbols().count()),
        })
        .collect();
    let id = ids::files();
    frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let all: Vec<FileId> = model.index.file_entries().map(|entry| entry.id).collect();
    files_tree(model, frame, &all, Count::ZERO, &coverage);
    frame.finish();
}

fn component(model: &Model, file: FileId, depth: Count) -> Label {
    Label::new(
        model
            .index
            .file(file)
            .and_then(|source| source.path().as_str().split('/').nth(depth.get()))
            .unwrap_or_default(),
    )
}

fn is_file(model: &Model, file: FileId, depth: Count) -> bool {
    model
        .index
        .file(file)
        .is_some_and(|source| source.path().as_str().split('/').count() == depth.get() + 1)
}

fn coverage_of(coverage: &[Coverage], file: FileId) -> Coverage {
    coverage.get(file.number()).map_or(
        Coverage {
            covered: Count::ZERO,
            total: Count::ZERO,
        },
        |found| Coverage {
            covered: found.covered,
            total: found.total,
        },
    )
}

fn files_tree(
    model: &Model,
    frame: &mut Frame<'_>,
    files: &[FileId],
    depth: Count,
    coverage: &[Coverage],
) {
    let indent = "  ".repeat(depth.get());
    let mut rest = files;
    while let Some((first, _)) = rest.split_first() {
        let first = *first;
        if is_file(model, first, depth) {
            let found = coverage_of(coverage, first);
            let (covered, total) = (found.covered.get(), found.total.get());
            let name = component(model, first, depth);
            let label = if total > 0 {
                format!("{indent}{}  {covered}/{total}", name.as_str())
            } else {
                format!("{indent}{}", name.as_str())
            };
            let color = if total > 0 && covered == 0 {
                WEAK
            } else {
                TEXT
            };
            let chosen = if model.nav.file() == Some(first) {
                Chosen::Chosen
            } else {
                Chosen::Plain
            };
            if frame
                .row(
                    vec![Run::new(label, color)],
                    ids::FILE_ROW.nth(Count::new(first.number())),
                    chosen,
                )
                .clicked()
            {
                frame.push(Action::GoTo(first, domain::Line::new(0)));
            }
            rest = rest.get(1..).unwrap_or(&[]);
            continue;
        }
        let directory = component(model, first, depth);
        let inside = rest
            .iter()
            .take_while(|file| {
                !is_file(model, **file, depth) && component(model, **file, depth) == directory
            })
            .count();
        let (group, after) = rest.split_at(inside);
        let (covered, total) = group.iter().fold((0, 0), |sums, file| {
            let found = coverage_of(coverage, *file);
            (sums.0 + found.covered.get(), sums.1 + found.total.get())
        });
        let prefix = model
            .index
            .file(first)
            .map(|source| {
                source
                    .path()
                    .as_str()
                    .split('/')
                    .take(depth.get() + 1)
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .unwrap_or_default();
        let open = (depth.get() == 0) ^ model.directories.contains(&Label::new(prefix.clone()));
        if frame
            .row(
                vec![Run::new(
                    format!(
                        "{indent}{} {}/  {covered}/{total}",
                        if open { Icon::Unfolded } else { Icon::Folded }
                            .glyph()
                            .get(),
                        directory.as_str()
                    ),
                    TEXT,
                )],
                ids::DIRECTORY_ROW.with(&Label::new(prefix.as_str())),
                Chosen::Plain,
            )
            .clicked()
        {
            frame.push(Action::ToggleDirectory(Label::new(prefix)));
        }
        if open {
            files_tree(model, frame, group, Count::new(depth.get() + 1), coverage);
        }
        rest = after;
    }
}
