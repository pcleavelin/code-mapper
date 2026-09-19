//! Text commands over the same index + map the GUI uses. Runs from the shell (the AI's way in)
//! and from the GUI's output panel. Output is plain lines, grep-style, written to a String.

use crate::index::{Backend, File, Index, SymRef};
use crate::map::{Anchor, Author, Change, Kind, Map, StepChange};
use clap::{CommandFactory, Parser};
use std::fmt::Write;

// One CLI command. Parsed by clap from the process arguments and from the GUI's command line,
// so `help` and argument errors read the same in both.
#[derive(Parser, Debug)]
#[command(
    name = "codemap",
    disable_help_flag = true,
    override_usage = "codemap <root>                     open the GUI\n       codemap <root> <command> [args]    text mode (same commands work in the GUI output panel)"
)]
pub enum Command {
    /// [filter]                         list indexed files (substring filter)
    Files { filter: Option<String> },
    /// [filter]                         list symbols: file:start-end kind name (calls/callers)
    Symbols { filter: Option<String> },
    /// <file> [start] [end]             print numbered lines (1-based, inclusive)
    Show { file: String, start: Option<usize>, end: Option<usize> },
    /// <regex>                          file:line: text
    Grep { regex: String },
    /// <regex>                          search path notes and step notes
    Notes { regex: String },
    /// <symbol>                         who calls it (xrefs to)
    Callers { symbol: String },
    /// <symbol>                         what it calls (xrefs from)
    Callees { symbol: String },
    /// <symbol>                         every reference to it, file:line: text (needs the language's server)
    Refs { symbol: String },
    /// <symbol> [depth]                 call tree from a symbol (default depth 4)
    Tree {
        symbol: String,
        #[arg(default_value_t = 4)]
        depth: usize,
    },
    /// [n]                              entry points: symbols nobody calls (default 30)
    Roots {
        #[arg(default_value_t = 30)]
        n: usize,
    },
    /// [name]                           the map: every path (or one) as a tree of steps (! = stale, (ai) = AI-authored)
    Paths { name: Option<String> },
    /// <name>                           print a path's note and every step's code, tree order
    Path { name: String },
    /// <name> <kind> [note]             create a path; kind = flow | layer | type (no-op if it exists)
    PathNew {
        name: String,
        #[arg(value_parser = parse_kind)]
        kind: Kind,
        note: Option<String>,
    },
    /// <name> <note>                    set a path's note
    PathNote { name: String, note: String },
    /// <name> <index> <note>            set a note on one step (index as shown by `paths`)
    StepNote { name: String, index: usize, note: String },
    /// <name> <index> <old> <new>       replace the first `old` in a note with `new` (index -1 = the path note)
    #[command(allow_negative_numbers = true)]
    NoteEdit { name: String, index: i64, old: String, new: String },
    /// <name> <new>                     rename a path
    PathRename { name: String, new: String },
    /// <name> <sym|file start end> [under]  add a step under step `under` (default: the last step; -1 = root)
    #[command(allow_negative_numbers = true)]
    PathAdd {
        name: String,
        target: String,
        #[arg(num_args = 0..=3)]
        nums: Vec<i64>,
    },
    /// <name> <index> <file> <start> <end>  re-anchor a step; its note and place in the tree stay
    PathPin { name: String, index: usize, file: String, start: usize, end: usize },
    /// <name> <index> <under>          move a step (with its subtree) under step `under` (-1 = root)
    #[command(allow_negative_numbers = true)]
    PathMove { name: String, index: usize, under: i64 },
    /// <name> <a> <b>                  swap two steps' places in the list, which orders siblings when the code does not
    PathSwap { name: String, a: usize, b: usize },
    /// <name> [index]                   delete a step (its children move up) or the whole path
    PathRm { name: String, index: Option<usize> },
    /// <symbol> [depth] [name]          create a path shaped like a symbol's call tree (default depth 1, named after the symbol)
    Promote {
        symbol: String,
        #[arg(default_value_t = 1)]
        depth: usize,
        name: Option<String>,
    },
    /// every step whose text no longer matches, or whose file or symbol is gone
    Stale,
    /// exit non-zero if any step is stale
    Check,
    /// [filter]                         symbols in no path, largest first
    Uncovered { filter: Option<String> },
    /// covered/total symbols per file
    Coverage,
    /// the map against the parent revision's (jj file show -r @- .codemap)
    Diff,
}

fn parse_kind(s: &str) -> Result<Kind, String> {
    Kind::parse(s).ok_or_else(|| format!("expected one of {}", Kind::NAMES.join(", ")))
}

/// Parses `args` (without the program name). The `Err` is clap's own message: help text for
/// `help`, or a usage error.
pub fn parse(args: &[String]) -> Result<Command, clap::Error> {
    Command::try_parse_from(std::iter::once("codemap").chain(args.iter().map(String::as_str)))
}

pub fn help() -> String {
    Command::command().render_help().to_string()
}

macro_rules! p {
    ($out:expr, $($t:tt)*) => { let _ = writeln!($out, $($t)*); };
}

/// Runs one command. Returns whether the map was mutated (caller saves).
pub fn exec(idx: &Index, map: &mut Map, cmd: Command, author: Author, out: &mut String) -> Result<bool, String> {
    let mut dirty = false;
    let (callers, check) = (matches!(cmd, Command::Callers { .. }), matches!(cmd, Command::Check));
    match cmd {
        Command::Files { filter } => {
            let filter = filter.unwrap_or_default();
            for f in idx.files.iter().filter(|f| f.path.contains(&filter)) {
                p!(out, "{} ({} lines, {} symbols)", f.path, f.lines.len(), f.symbols.len());
            }
        }
        Command::Symbols { filter } => {
            let filter = filter.unwrap_or_default();
            for f in &idx.files {
                for s in f.symbols.iter().filter(|s| s.name.contains(&filter) || f.path.contains(&filter)) {
                    p!(
                        out,
                        "{}:{}-{} {} {}{} ({} calls, {} callers)",
                        f.path,
                        s.start + 1,
                        s.end + 1,
                        s.kind,
                        if s.depth > 0 { "  " } else { "" },
                        s.name,
                        s.callees.len(),
                        s.callers.len()
                    );
                }
            }
        }
        Command::Show { file, start, end } => {
            let f = &idx.files[find_file(idx, &file)?];
            let start = start.unwrap_or(1).max(1);
            let end = end.unwrap_or(f.lines.len()).min(f.lines.len());
            print_lines(out, f, start - 1, end.saturating_sub(1));
        }
        Command::Grep { regex } => {
            let re = regex::Regex::new(&regex).map_err(|e| e.to_string())?;
            for f in &idx.files {
                for (li, line) in f.lines.iter().enumerate() {
                    if re.is_match(line) {
                        p!(out, "{}:{}: {}", f.path, li + 1, line);
                    }
                }
            }
        }
        Command::Notes { regex } => {
            let re = regex::Regex::new(&regex).map_err(|e| e.to_string())?;
            for path in &map.paths {
                for line in path.note.lines().filter(|l| re.is_match(l)) {
                    p!(out, "{}: {line}", path.name);
                }
                for (i, a) in path.anchors.iter().enumerate() {
                    for line in a.note.lines().filter(|l| re.is_match(l)) {
                        p!(out, "{}[{i}] {}:{}: {line}", path.name, a.file, a.line_start + 1);
                    }
                }
            }
        }
        Command::Callers { symbol } | Command::Callees { symbol } => {
            for r in find_symbols(idx, &symbol)? {
                let s = idx.sym(r);
                p!(out, "{}", describe(idx, r));
                for &t in if callers { &s.callers } else { &s.callees } {
                    p!(out, "  {}", describe(idx, t));
                }
            }
        }
        Command::Refs { symbol } => {
            for r in find_symbols(idx, &symbol)? {
                let s = idx.sym(r);
                p!(out, "{}", describe(idx, r));
                if idx.files[r.file].backend != Backend::Server {
                    let server = crate::index::lang_for(&idx.files[r.file].path).map_or("no server for this language", |l| l.server);
                    p!(out, "  (references need {server})");
                }
                for (path, line) in &s.refs {
                    let text = idx.find_file(path).and_then(|fi| idx.files[fi].lines.get(*line as usize)).map(|l| l.trim()).unwrap_or("");
                    p!(out, "  {path}:{}: {text}", line + 1);
                }
            }
        }
        Command::Tree { symbol, depth } => {
            for r in find_symbols(idx, &symbol)? {
                for (node, d) in idx.call_tree(r, depth) {
                    p!(out, "{}{}", "  ".repeat(d), describe(idx, node));
                }
            }
        }
        Command::Roots { n } => {
            for r in idx.roots().into_iter().take(n) {
                p!(out, "{} ({} calls)", describe(idx, r), idx.sym(r).callees.len());
            }
        }
        Command::Paths { name } => {
            if let Some(n) = &name {
                find_path(map, n)?;
            }
            for (pi, path) in map.paths.iter().enumerate().filter(|(_, p)| name.as_ref().is_none_or(|n| &p.name == n)) {
                let note = if path.note.is_empty() { String::new() } else { format!(": {}", path.note) };
                p!(out, "{} [{}]{} ({} steps){}", path.name, path.kind.name(), path.author.tag(), path.anchors.len(), note);
                for (i, depth) in map.tree_order(pi) {
                    let a = &path.anchors[i];
                    p!(
                        out,
                        "  {}[{i}] {}{} {}{}{}",
                        "  ".repeat(depth),
                        if a.stale { "! " } else { "" },
                        where_is(idx, a),
                        a.symbol,
                        a.author.tag(),
                        if a.note.is_empty() { String::new() } else { format!("  -- {}", a.note) }
                    );
                }
            }
        }
        Command::Path { name } => {
            let pi = find_path(map, &name)?;
            let path = &map.paths[pi];
            p!(out, "# {} [{}]{}", path.name, path.kind.name(), path.author.tag());
            if !path.note.is_empty() {
                p!(out, "{}", path.note);
            }
            let mut prev_depth = 0;
            for (i, depth, number) in map.numbered(idx, pi) {
                let a = &path.anchors[i];
                if depth < prev_depth {
                    p!(out, "\n{}-- back in {} --", "  ".repeat(depth), map.parent_name(pi, i));
                }
                prev_depth = depth;
                p!(out, "\n== {}{number} [{i}] {}{} {}{}", "  ".repeat(depth), if a.stale { "STALE " } else { "" }, where_is(idx, a), a.symbol, a.author.tag());
                if !a.note.is_empty() {
                    p!(out, "-- {}", a.note);
                }
                if let Some(fi) = idx.find_file(&a.file) {
                    print_lines(out, &idx.files[fi], a.line_start, a.line_end);
                }
            }
        }
        Command::PathNew { name, kind, note } => {
            let pi = map.add_path(&name, kind, author);
            if let Some(note) = note {
                map.paths[pi].note = note;
            }
            dirty = true;
        }
        Command::PathNote { name, note } => {
            let pi = find_path(map, &name)?;
            map.paths[pi].note = note;
            dirty = true;
        }
        Command::StepNote { name, index, note } => {
            let pi = find_path(map, &name)?;
            map.paths[pi].anchors.get_mut(index).ok_or("no such step")?.note = note;
            dirty = true;
        }
        Command::NoteEdit { name, index, old, new } => {
            let pi = find_path(map, &name)?;
            let note = if index < 0 { &mut map.paths[pi].note } else { &mut map.paths[pi].anchors.get_mut(index as usize).ok_or("no such step")?.note };
            if !note.contains(&old) {
                return Err(format!("the note does not contain '{old}'"));
            }
            *note = note.replacen(&old, &new, 1);
            p!(out, "{note}");
            dirty = true;
        }
        Command::PathRename { name, new } => {
            let pi = find_path(map, &name)?;
            map.rename(pi, &new)?;
            dirty = true;
        }
        Command::PathAdd { name, target, nums } => {
            let pi = find_path(map, &name)?;
            let last = map.paths[pi].anchors.len() as i64 - 1;
            let (lines, under) = match nums[..] {
                [] => (None, last),
                [under] => (None, under),
                [start, end] => (Some((start, end)), last),
                [start, end, under] => (Some((start, end)), under),
                _ => unreachable!(),
            };
            let n = map.paths[pi].anchors.len() as i64;
            if under < -1 || under >= n {
                return Err(format!("no step [{under}] to go under: the path has {n} steps (-1 = root)"));
            }
            let under = under as i32;
            match lines {
                Some((start, end)) => {
                    let fi = find_file(idx, &target)?;
                    let (start, end) = check_range(&idx.files[fi], start, end)?;
                    let ai = map.add_anchor(idx, pi, fi, start, end, author, under);
                    p!(out, "step [{ai}] added under [{}]", map.paths[pi].anchors[ai].parent);
                    absolute_warning(out, &map.paths[pi].anchors[ai]);
                    call_warning(out, idx, map, pi, ai);
                }
                None => {
                    let r = find_symbol(idx, &target)?;
                    let s = idx.sym(r);
                    let ai = map.add_anchor(idx, pi, r.file, s.start, s.end, author, under);
                    p!(out, "step [{ai}] {} added under [{}]", s.name, map.paths[pi].anchors[ai].parent);
                    call_warning(out, idx, map, pi, ai);
                }
            }
            dirty = true;
        }
        Command::PathSwap { name, a, b } => {
            let pi = find_path(map, &name)?;
            let n = map.paths[pi].anchors.len();
            if a >= n || b >= n {
                return Err("no such step".into());
            }
            map.swap_anchors(pi, a, b);
            dirty = true;
        }
        Command::PathMove { name, index, under } => {
            let pi = find_path(map, &name)?;
            if index >= map.paths[pi].anchors.len() {
                return Err("no such step".into());
            }
            map.reparent(pi, index, under as i32)?;
            p!(out, "step [{index}] now under [{under}]");
            call_warning(out, idx, map, pi, index);
            dirty = true;
        }
        Command::PathPin { name, index, file, start, end } => {
            let pi = find_path(map, &name)?;
            if index >= map.paths[pi].anchors.len() {
                return Err("no such step".into());
            }
            let fi = find_file(idx, &file)?;
            let (start, end) = check_range(&idx.files[fi], start as i64, end as i64)?;
            map.pin_anchor(idx, pi, index, fi, start, end, author);
            p!(out, "step [{index}] pinned to {}", where_is(idx, &map.paths[pi].anchors[index]));
            absolute_warning(out, &map.paths[pi].anchors[index]);
            dirty = true;
        }
        Command::PathRm { name, index } => {
            let pi = find_path(map, &name)?;
            match index {
                Some(ai) if ai < map.paths[pi].anchors.len() => map.remove_anchor(pi, ai),
                Some(_) => return Err("no such step".into()),
                None => {
                    map.paths.remove(pi);
                }
            }
            dirty = true;
        }
        Command::Promote { symbol, depth, name } => {
            let pi = map.promote(idx, find_symbol(idx, &symbol)?, depth, name.as_deref(), author);
            p!(out, "path '{}' now has {} steps", map.paths[pi].name, map.paths[pi].anchors.len());
            dirty = true;
        }
        Command::Stale | Command::Check => {
            let mut n = 0;
            for path in &map.paths {
                for (i, a) in path.anchors.iter().enumerate().filter(|(_, a)| a.stale) {
                    n += 1;
                    p!(out, "{}[{i}] {} {}", path.name, where_is(idx, a), a.symbol);
                    if let Some((ls, le)) = idx.find_file(&a.file).and_then(|fi| moved_to(&idx.files[fi], a)) {
                        p!(out, "  same text at {}:{}-{}   path-pin {} {i} {} {} {}", a.file, ls + 1, le + 1, path.name, a.file, ls + 1, le + 1);
                    } else if a.sym.is_none() && !a.symbol.is_empty() {
                        // the symbol is gone from its file: the same name elsewhere is the likely home
                        for r in idx.find_symbols(&a.symbol) {
                            let (s, f) = (idx.sym(r), &idx.files[r.file]);
                            p!(out, "  same name at {}:{}-{}   path-pin {} {i} {} {} {}", f.path, s.start + 1, s.end + 1, path.name, f.path, s.start + 1, s.end + 1);
                        }
                    }
                }
            }
            if check {
                if n > 0 {
                    return Err(format!("{n} stale steps"));
                }
                p!(out, "ok");
            }
        }
        Command::Uncovered { filter } => {
            let filter = filter.unwrap_or_default();
            let mut list: Vec<(usize, SymRef)> = Vec::new();
            for (fi, f) in idx.files.iter().enumerate() {
                for (si, s) in f.symbols.iter().enumerate() {
                    if (s.name.contains(&filter) || f.path.contains(&filter)) && !map.covers(&f.path, s.start, s.end) {
                        list.push((s.end - s.start + 1, SymRef { file: fi, sym: si }));
                    }
                }
            }
            list.sort_by_key(|&(n, _)| std::cmp::Reverse(n));
            for (n, r) in list {
                let s = idx.sym(r);
                p!(out, "{}:{}-{} {} {} ({n} lines)", idx.files[r.file].path, s.start + 1, s.end + 1, s.kind, s.name);
            }
        }
        Command::Diff => {
            let base = Map::base_from_vcs(&idx.root).ok_or("no map in the parent revision (needs a jj repo with a committed .codemap)")?;
            for d in map.diff(&base) {
                match d.change {
                    Change::Same => continue,
                    Change::Added => {
                        p!(out, "+ {} ({} steps)", d.name, d.steps.len());
                    }
                    Change::Removed => {
                        p!(out, "- {} ({} steps)", d.name, d.removed.len());
                    }
                    Change::Changed => {
                        p!(out, "~ {}{}", d.name, if d.note_changed { "  (note or kind changed)" } else { "" });
                    }
                }
                if d.change == Change::Added || d.change == Change::Removed {
                    continue;
                }
                let pi = find_path(map, &d.name)?;
                for (i, c) in d.steps.iter().enumerate() {
                    if let Some(c) = c {
                        let a = &map.paths[pi].anchors[i];
                        let mark = if *c == StepChange::Added { '+' } else { '~' };
                        p!(out, "    {mark} [{i}] {} {}  {}", where_is(idx, a), a.symbol, c.tag());
                    }
                }
                for a in &d.removed {
                    p!(out, "    - {} {} (removed)", a.file, a.symbol);
                }
            }
        }
        Command::Coverage => {
            let (mut tc, mut tt) = (0, 0);
            for f in idx.files.iter().filter(|f| !f.symbols.is_empty()) {
                let c = f.symbols.iter().filter(|s| map.covers(&f.path, s.start, s.end)).count();
                p!(out, "{}: {c}/{}", f.path, f.symbols.len());
                tc += c;
                tt += f.symbols.len();
            }
            p!(out, "total: {tc}/{tt}");
        }
    }
    Ok(dirty)
}

/// Shell-style split with double quotes, for the GUI command line.
pub fn tokenize(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn describe(idx: &Index, r: SymRef) -> String {
    let s = idx.sym(r);
    format!("{} {}:{}-{}", s.name, idx.files[r.file].path, s.start + 1, s.end + 1)
}

/// In a flow, a step belongs under the step that calls it. Says so when it does not.
fn call_warning(out: &mut String, idx: &Index, map: &Map, pi: usize, ai: usize) {
    let path = &map.paths[pi];
    if path.kind != Kind::Flow {
        return;
    }
    let a = &path.anchors[ai];
    let Some(parent) = usize::try_from(a.parent).ok().and_then(|p| path.anchors.get(p)) else { return };
    let sym_of = |x: &Anchor| Some(SymRef { file: idx.find_file(&x.file)?, sym: x.sym? });
    let callable = |k: &str| ["function", "method", "macro", "constructor", "proc"].iter().any(|w| k.contains(w));
    if let (Some(p), Some(c)) = (sym_of(parent), sym_of(a)) {
        // data under the function that works on it is a normal step; only a misplaced call is noted
        if p != c && callable(&idx.sym(c).kind) && !idx.sym(p).callees.contains(&c) {
            p!(out, "note: {} does not call {}; in a flow a step goes under the step that calls it (path-move <name> {ai} <under>)", parent.symbol, a.symbol);
        }
    }
}

/// Where a stale step's unchanged text now sits in its file, if it moved rather than changed:
/// the same number of lines with the same hash. The re-pin stays the agent's explicit call.
fn moved_to(f: &File, a: &Anchor) -> Option<(usize, usize)> {
    let len = (a.off_end - a.off_start) as usize;
    (0..f.lines.len().checked_sub(len)?).map(|ls| (ls, ls + len)).find(|&(ls, le)| (ls, le) != (a.line_start, a.line_end) && crate::map::slice_hash(&f.lines, ls, le) == a.hash)
}

/// A slice that no single symbol contains only survives edits below it.
fn absolute_warning(out: &mut String, a: &Anchor) {
    if a.symbol.is_empty() {
        p!(out, "note: lines {}-{} are not inside one symbol; pinned as absolute lines, which go stale with any edit above them", a.line_start + 1, a.line_end + 1);
    }
}

/// `file:start-end` for a resolved anchor; says what is gone otherwise.
fn where_is(idx: &Index, a: &Anchor) -> String {
    match idx.find_file(&a.file) {
        None => format!("{} (file gone)", a.file),
        Some(_) if !a.symbol.is_empty() && a.sym.is_none() => format!("{} (symbol gone)", a.file),
        Some(_) => format!("{}:{}-{}", a.file, a.line_start + 1, a.line_end + 1),
    }
}

fn print_lines(out: &mut String, f: &File, start: usize, end: usize) {
    for li in start..=end.min(f.lines.len().saturating_sub(1)) {
        p!(out, "{:5} {}", li + 1, f.lines[li]);
    }
}

/// 1-based inclusive user range -> 0-based, checked against the file.
fn check_range(f: &File, start: i64, end: i64) -> Result<(usize, usize), String> {
    if start < 1 || end < start || end > f.lines.len() as i64 {
        return Err("line range out of bounds".into());
    }
    Ok((start as usize - 1, end as usize - 1))
}

fn find_file(idx: &Index, path: &str) -> Result<usize, String> {
    let path = path.replace('\\', "/");
    idx.find_file(&path)
        .or_else(|| idx.files.iter().position(|f| f.path.ends_with(&path)))
        .ok_or(format!("no such file: {path}"))
}

fn find_symbols(idx: &Index, name: &str) -> Result<Vec<SymRef>, String> {
    let found = idx.find_symbols(name);
    if found.is_empty() { Err(format!("no such symbol: {name}")) } else { Ok(found) }
}

/// Exactly one symbol; an ambiguous name lists each candidate with the qualified name that
/// selects it.
fn find_symbol(idx: &Index, name: &str) -> Result<SymRef, String> {
    let found = find_symbols(idx, name)?;
    if found.len() > 1 {
        let list: Vec<String> = found.iter().map(|&r| format!("{:<40} {}", unique_name(idx, r), describe(idx, r))).collect();
        return Err(format!("ambiguous: {name}; use one of\n  {}", list.join("\n  ")));
    }
    Ok(found[0])
}

/// The shortest qualified name that selects exactly `r`: `Owner::name`, `stem:name`,
/// `stem:Owner::name`, or the full path forms.
pub fn unique_name(idx: &Index, r: SymRef) -> String {
    let (s, f) = (idx.sym(r), &idx.files[r.file]);
    let mut tries = Vec::new();
    if let Some(o) = &s.owner {
        tries.push(format!("{o}::{}", s.name));
    }
    tries.push(format!("{}:{}", f.stem(), s.name));
    if let Some(o) = &s.owner {
        tries.push(format!("{}:{o}::{}", f.stem(), s.name));
    }
    tries.push(format!("{}:{}", f.path, s.name));
    if let Some(o) = &s.owner {
        tries.push(format!("{}:{o}::{}", f.path, s.name));
    }
    tries.into_iter().find(|t| idx.find_symbols(t) == [r]).unwrap_or_else(|| describe(idx, r))
}

fn find_path(map: &Map, name: &str) -> Result<usize, String> {
    map.find(name).ok_or(format!("no such path: {name}"))
}
