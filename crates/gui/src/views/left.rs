use domain::{
    Change, Coverage, Depth, FileId, GroupName, PathCount, PathDiff, Row, Step, SymbolName,
};
use ui::{Count, Icon, Label, Px, Run};

use crate::action::Action;
use crate::authoring::{Authoring, Hang};
use crate::field::Which;
use crate::ids;
use crate::model::{Model, Openness, PathSlot, StepKey, Tab, ViewFlag};
use crate::nav::Scrolling;
use crate::text::{Clipped, Counted, Needle, Noun, Tag};
use crate::theme::{
    ACCENT, Cells, FAINT, FILTER_FIELD, GREEN, OUTLINE_TOP, PANEL_TEXT_ROOM, PATHS_GUESS,
    PATHS_LEAST, PENDING, PIXEL, RED, ROW_EXTRA, SELECTED, SYMBOL_FIXED, SYMBOL_NAME,
    SYMBOL_NAME_LEAST, SYMBOLS_GUESS, SYMBOLS_LEAST, TEXT, WEAK,
};
use crate::widgets::{Chosen, Container, Frame, RowAction, Scroller};

use super::authoring;

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
        "{}{} {}/",
        pad.as_str(),
        if open { Icon::Unfolded } else { Icon::Folded }
            .glyph()
            .get(),
        group.last_segment(),
    );
    let tally = format!(
        "  {}",
        Counted::new(
            Count::new(usize::try_from(paths.value()).unwrap_or(0)),
            Noun::Path
        )
    );
    let stale = model.map.paths().iter().any(|path| {
        path.group().is_some_and(|inside| {
            inside.as_str() == full || inside.as_str().starts_with(&format!("{full}/"))
        }) && path.steps().iter().any(Step::is_stale)
    });
    let id = ids::GROUP_ROW.with(&Label::new(full));
    let runs = vec![
        Run::new(label, if stale { RED } else { TEXT }),
        Run::new(tally, WEAK),
    ];
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
    let tally = format!(
        " [{}]{}  {}",
        Tag::kind(path.kind()),
        Tag::author(path.author()),
        Counted::new(Count::new(steps), Noun::Step)
    );
    let stale_note = if stale > 0 {
        format!("  {stale} stale")
    } else {
        String::new()
    };
    let room = columns
        .get()
        .saturating_sub(pad.len() + mark.len() + tally.chars().count() + stale_note.len());
    let line = format!("{pad}{mark}{}", Clipped::right(name.as_str(), room));
    if frame
        .row(
            vec![
                Run::new(line, color),
                Run::new(tally, WEAK),
                Run::new(stale_note, RED),
            ],
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
    let filter = model.fields.get(Which::PathFilter).text().as_str();
    let rows = model.listed_rows();
    frame.start(Container::ToolbarSmall);
    frame.field(
        &model.fields,
        Which::PathFilter,
        &Label::new("filter"),
        FILTER_FIELD,
    );
    authoring::new_path_button(frame);
    if !filter.is_empty() {
        let shown = rows
            .iter()
            .filter(|row| matches!(row, Row::Path { .. }))
            .count();
        frame.label(format!("{shown} of {}", model.map.paths().len()), WEAK);
    }
    frame.finish();
    authoring::new_path_form(model, frame);
    if rows.is_empty() {
        frame.label(
            if filter.is_empty() {
                "no paths yet".to_owned()
            } else {
                format!("no path name matches '{filter}'")
            },
            WEAK,
        );
    }
    let forced = (!filter.is_empty()).then_some(Openness::Open);
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
    for row in rows {
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
                "- {}  (removed, {})",
                diff.name(),
                Counted::new(Count::new(diff.removed().len()), Noun::Step)
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
        let mut runs = vec![Run::new(line, color), Run::new(format!("  {file}"), FAINT)];
        if let Some(mark) = authoring::target_mark(model, numbered.step) {
            runs.push(Run::new(mark, ACCENT));
        }
        if frame
            .marked_row(
                runs,
                ids::OUTLINE_ROW.nth(Count::new(numbered.step.get())),
                marked,
            )
            .clicked()
        {
            let mouse = frame.ui.pointer().mouse;
            frame.push(Action::SelectStep(key, Scrolling::Scroll));
            frame.push(Action::Authoring(Authoring::GrabStep(key, mouse)));
        }
    }
}

enum SymbolLine {
    File(FileId),
    Symbol(domain::SymbolId),
}

fn symbol_lines(model: &Model, needle: &Needle) -> Vec<SymbolLine> {
    let index = &model.index;
    let mut lines = Vec::new();
    let mut current: Option<(FileId, bool)> = None;
    for symbol in index.symbol_ids() {
        let file = symbol.file();
        let file_found = match current {
            Some((open, found)) if open == file => found,
            _ => {
                let found = index
                    .file(file)
                    .is_some_and(|source| needle.found_in(source.path().as_str()));
                current = Some((file, found));
                lines.push(SymbolLine::File(file));
                found
            }
        };
        if file_found
            || index
                .symbol(symbol)
                .is_some_and(|found| needle.found_in(found.name().as_str()))
        {
            lines.push(SymbolLine::Symbol(symbol));
        }
    }
    let mut kept: Vec<SymbolLine> = Vec::with_capacity(lines.len());
    for line in lines {
        if matches!(line, SymbolLine::File(_)) && matches!(kept.last(), Some(SymbolLine::File(_))) {
            kept.pop();
        }
        kept.push(line);
    }
    if matches!(kept.last(), Some(SymbolLine::File(_))) {
        kept.pop();
    }
    kept
}

pub(super) fn symbols_window(model: &Model, frame: &mut Frame<'_>) {
    let needle = Needle::new(model.fields.get(Which::SymbolFilter).text().as_str());
    let lines = symbol_lines(model, &needle);
    let shown = lines
        .iter()
        .filter(|line| matches!(line, SymbolLine::Symbol(_)))
        .count();
    let total = model.index.symbol_ids().count();
    frame.start(Container::ToolbarSmall);
    frame.field(
        &model.fields,
        Which::SymbolFilter,
        &Label::new("filter"),
        FILTER_FIELD,
    );
    frame.label(
        if needle.is_empty() {
            Counted::new(Count::new(total), Noun::Symbol).to_string()
        } else {
            format!("{shown} of {total}")
        },
        WEAK,
    );
    frame.caption(vec![
        Run::new(Icon::Check, GREEN),
        Run::new(" in a path", WEAK),
    ]);
    frame.finish();
    authoring::target_strip(model, frame);
    if lines.is_empty() {
        frame.label(
            if needle.is_empty() {
                "no symbols indexed".to_owned()
            } else {
                format!("no symbol or file matches '{}'", needle.as_str())
            },
            WEAK,
        );
        return;
    }
    let id = ids::symbols();
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
    let coverage = model.map.coverage();
    let columns = columns(
        frame,
        scrolled.interaction.rect(),
        SYMBOLS_GUESS,
        SYMBOLS_LEAST,
    );
    for line in lines
        .iter()
        .skip(window.first.get())
        .take(window.visible.get())
    {
        match line {
            SymbolLine::File(file) => symbol_file_row(model, frame, *file, &coverage, columns),
            SymbolLine::Symbol(symbol) => symbol_row(model, frame, *symbol, &coverage, columns),
        }
    }
    frame.rows_after(count, &window, row_height, Px::ZERO);
    frame.finish();
}

fn symbol_file_row(
    model: &Model,
    frame: &mut Frame<'_>,
    file: FileId,
    coverage: &Coverage<'_>,
    columns: Count,
) {
    let Some(source) = model.index.file(file) else {
        return;
    };
    let total = source.symbols().count();
    let covered = source
        .symbols()
        .filter(|symbol| coverage.covers(source.path(), symbol.span()))
        .count();
    let tally = format!("  {covered}/{total}");
    let room = columns.get().saturating_sub(tally.len());
    let runs = vec![
        Run::new(
            Clipped::left(source.path().as_str(), room).to_string(),
            if source.is_pending() { PENDING } else { TEXT },
        ),
        Run::new(tally, WEAK),
    ];
    if frame
        .row(
            runs,
            ids::SYMBOL_FILE_ROW.nth(Count::new(file.number())),
            Chosen::Plain,
        )
        .clicked()
    {
        frame.push(Action::GoTo(file, domain::Line::new(0)));
    }
}

fn symbol_row(
    model: &Model,
    frame: &mut Frame<'_>,
    symbol_id: domain::SymbolId,
    coverage: &Coverage<'_>,
    columns: Count,
) {
    let index = &model.index;
    let adding = model.nav.path().is_some();
    let (Some(symbol), Some(file)) = (index.symbol(symbol_id), index.file(symbol_id.file())) else {
        return;
    };
    let covered = coverage.covers(file.path(), symbol.span());
    let mark = if covered {
        Run::new(Icon::Check, GREEN)
    } else {
        Run::new("  ", WEAK)
    };
    let indent = if symbol.depth().value() > 0 {
        "    "
    } else {
        "  "
    };
    let fixed = usize::try_from(SYMBOL_FIXED.get()).unwrap_or(0);
    let name_width = columns.get().saturating_sub(fixed).clamp(
        usize::try_from(SYMBOL_NAME_LEAST.get()).unwrap_or(0),
        usize::try_from(SYMBOL_NAME.get()).unwrap_or(0),
    );
    let name = format!(
        "{indent}{:<name_width$} ",
        Clipped::right(symbol.name().as_str(), name_width).to_string()
    );
    let place = format!(
        "{:>5}  {}",
        symbol.span().start().number(),
        symbol.kind().as_str()
    );
    let pending = file.is_pending();
    let runs = vec![
        Run::new("  ", WEAK),
        mark,
        Run::new(name, if pending { PENDING } else { TEXT }),
        Run::new(place, if pending { PENDING } else { WEAK }),
    ];
    let chosen = if model.nav.focus() == Some(symbol_id) {
        Chosen::Chosen
    } else {
        Chosen::Plain
    };
    let key = Label::new(format!("{}:{}", symbol_id.file(), symbol_id.symbol()));
    let add = adding.then(|| RowAction::add_step(ids::ADD_SYMBOL.with(&key)));
    let clicks = frame.row_with_action(
        runs,
        ids::SYMBOL_ROW.with(&key),
        (chosen == Chosen::Chosen).then_some(SELECTED),
        add,
    );
    if clicks.acted() {
        frame.push(Action::Authoring(Authoring::AddSymbol(
            symbol_id,
            Hang::Target,
        )));
    } else if clicks.row.clicked() {
        frame.push(Action::Focus(symbol_id));
    }
}

struct Tally {
    covered: Count,
    total: Count,
}

pub(super) fn files_window(model: &Model, frame: &mut Frame<'_>) {
    let coverage = model.map.coverage();
    let tallies: Vec<Tally> = model
        .index
        .files()
        .map(|file| Tally {
            covered: Count::new(
                file.symbols()
                    .filter(|symbol| coverage.covers(file.path(), symbol.span()))
                    .count(),
            ),
            total: Count::new(file.symbols().count()),
        })
        .collect();
    let (covered, total) = tallies.iter().fold((0, 0), |sums, tally| {
        (sums.0 + tally.covered.get(), sums.1 + tally.total.get())
    });
    frame.start(Container::Header);
    frame.label(
        format!("{covered} of {total} symbols in a path; covered/total per file"),
        WEAK,
    );
    frame.finish();
    let id = ids::files();
    frame.scroll_column(id, model.scrolls.get(id), Scroller::Plain, None);
    let all: Vec<FileId> = model.index.file_entries().map(|entry| entry.id).collect();
    files_tree(model, frame, &all, Count::ZERO, &tallies);
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

fn tally_of(tallies: &[Tally], file: FileId) -> Tally {
    tallies.get(file.number()).map_or(
        Tally {
            covered: Count::ZERO,
            total: Count::ZERO,
        },
        |found| Tally {
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
    tallies: &[Tally],
) {
    let indent = "  ".repeat(depth.get());
    let mut rest = files;
    while let Some((first, _)) = rest.split_first() {
        let first = *first;
        if is_file(model, first, depth) {
            let found = tally_of(tallies, first);
            let (covered, total) = (found.covered.get(), found.total.get());
            let name = component(model, first, depth);
            let tally = if total > 0 {
                format!("  {covered}/{total}")
            } else {
                String::new()
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
                    vec![
                        Run::new(format!("{indent}{}", name.as_str()), color),
                        Run::new(tally, WEAK),
                    ],
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
            let found = tally_of(tallies, *file);
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
        let tally = if total > 0 {
            format!("  {covered}/{total}")
        } else {
            String::new()
        };
        if frame
            .row(
                vec![
                    Run::new(
                        format!(
                            "{indent}{} {}/",
                            if open { Icon::Unfolded } else { Icon::Folded }
                                .glyph()
                                .get(),
                            directory.as_str()
                        ),
                        TEXT,
                    ),
                    Run::new(tally, WEAK),
                ],
                ids::DIRECTORY_ROW.with(&Label::new(prefix.as_str())),
                Chosen::Plain,
            )
            .clicked()
        {
            frame.push(Action::ToggleDirectory(Label::new(prefix)));
        }
        if open {
            files_tree(model, frame, group, Count::new(depth.get() + 1), tallies);
        }
        rest = after;
    }
}
