//! Text commands over the same index + map the GUI uses. Runs from the shell (the AI's way in)
//! and from the GUI's output panel. Output is plain lines, grep-style, written to a String.

use crate::index::{File, Index, SymRef};
use crate::map::{Author, Map};
use std::fmt::Write;

pub const HELP: &str = "\
codemap <root>                         open the GUI
codemap <root> <command> [args]        text mode (same commands work in the GUI output panel)

  files [filter]                       list indexed files (substring filter)
  symbols [filter]                     list symbols: file:start-end kind name (calls/callers)
  show <file> [start] [end]            print numbered lines (1-based, inclusive)
  grep <regex>                         file:line: text
  callers <symbol>                     who calls it (xrefs to)
  callees <symbol>                     what it calls (xrefs from)
  tree <symbol> [depth]                call tree from a symbol (default depth 4)
  roots [n]                            entry points: symbols nobody calls (default 30)
  paths                                the map: every path, note, anchors (! = stale, (ai) = AI-authored)
  path <name>                          print a path's note and anchored code
  path-new <name> [note]               create a path (no-op if it exists)
  path-note <name> <note>              set a path's note
  step-note <name> <index> <note>      set a note on one step (0-based)
  path-add <name> <file> <start> <end> anchor lines (1-based, inclusive) to a path
  path-add <name> <symbol>             anchor a whole symbol to a path
  path-rm <name> [anchor-index]        delete an anchor (0-based) or the whole path
  promote <symbol> [depth]             create a path from a symbol's call tree (default depth 1)
";

macro_rules! p {
    ($out:expr, $($t:tt)*) => { let _ = writeln!($out, $($t)*); };
}

/// Runs one command. Returns whether the map was mutated (caller saves).
pub fn exec(idx: &Index, map: &mut Map, args: &[String], author: Author, out: &mut String) -> Result<bool, String> {
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let arg = |i: usize| args.get(i).map(String::as_str).ok_or_else(|| format!("missing argument\n{HELP}"));
    let num = |i: usize| args.get(i).and_then(|s| s.parse::<usize>().ok());
    let mut dirty = false;

    match cmd {
        "help" => out.push_str(HELP),
        "files" => {
            let filter = args.get(1).map(String::as_str).unwrap_or("");
            for f in idx.files.iter().filter(|f| f.path.contains(filter)) {
                p!(out, "{} ({} lines, {} symbols)", f.path, f.lines.len(), f.symbols.len());
            }
        }
        "symbols" => {
            let filter = args.get(1).map(String::as_str).unwrap_or("");
            for f in &idx.files {
                for s in f.symbols.iter().filter(|s| s.name.contains(filter) || f.path.contains(filter)) {
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
        "show" => {
            let f = &idx.files[find_file(idx, arg(1)?)?];
            let start = num(2).unwrap_or(1).max(1);
            let end = num(3).unwrap_or(f.lines.len()).min(f.lines.len());
            print_lines(out, f, start - 1, end.saturating_sub(1));
        }
        "grep" => {
            let re = regex::Regex::new(arg(1)?).map_err(|e| e.to_string())?;
            for f in &idx.files {
                for (li, line) in f.lines.iter().enumerate() {
                    if re.is_match(line) {
                        p!(out, "{}:{}: {}", f.path, li + 1, line);
                    }
                }
            }
        }
        "callers" | "callees" => {
            for r in find_symbols(idx, arg(1)?)? {
                let s = idx.sym(r);
                p!(out, "{}", describe(idx, r));
                let list = if cmd == "callers" { &s.callers } else { &s.callees };
                for &t in list {
                    p!(out, "  {}", describe(idx, t));
                }
            }
        }
        "tree" => {
            let depth = num(2).unwrap_or(4);
            for r in find_symbols(idx, arg(1)?)? {
                for (node, d) in idx.call_tree(r, depth) {
                    p!(out, "{}{}", "  ".repeat(d), describe(idx, node));
                }
            }
        }
        "roots" => {
            let n = num(1).unwrap_or(30);
            for r in idx.roots().into_iter().take(n) {
                p!(out, "{} ({} calls)", describe(idx, r), idx.sym(r).callees.len());
            }
        }
        "paths" => {
            for path in &map.paths {
                let note = if path.note.is_empty() { String::new() } else { format!(": {}", path.note) };
                p!(out, "{}{} ({} anchors){}", path.name, path.author.tag(), path.anchors.len(), note);
                for (i, a) in path.anchors.iter().enumerate() {
                    p!(
                        out,
                        "  [{i}] {}{}:{}-{} {}{}{}",
                        if a.stale { "! " } else { "" },
                        a.file,
                        a.line_start + 1,
                        a.line_end + 1,
                        a.symbol,
                        a.author.tag(),
                        if a.note.is_empty() { String::new() } else { format!("  -- {}", a.note) }
                    );
                }
            }
        }
        "path" => {
            let path = &map.paths[find_path(map, arg(1)?)?];
            p!(out, "# {}{}", path.name, path.author.tag());
            if !path.note.is_empty() {
                p!(out, "{}", path.note);
            }
            for a in &path.anchors {
                p!(
                    out,
                    "\n== {}{}:{}-{} {}{}",
                    if a.stale { "STALE " } else { "" },
                    a.file,
                    a.line_start + 1,
                    a.line_end + 1,
                    a.symbol,
                    a.author.tag()
                );
                if !a.note.is_empty() {
                    p!(out, "-- {}", a.note);
                }
                if let Some(fi) = idx.find_file(&a.file) {
                    print_lines(out, &idx.files[fi], a.line_start, a.line_end);
                }
            }
        }
        "path-new" => {
            let pi = map.add_path(arg(1)?, author);
            if let Some(note) = args.get(2) {
                map.paths[pi].note = note.clone();
            }
            dirty = true;
        }
        "path-note" => {
            let pi = find_path(map, arg(1)?)?;
            map.paths[pi].note = arg(2)?.to_owned();
            dirty = true;
        }
        "step-note" => {
            let pi = find_path(map, arg(1)?)?;
            let ai = num(2).ok_or("bad step index")?;
            let a = map.paths[pi].anchors.get_mut(ai).ok_or("no such step")?;
            a.note = arg(3)?.to_owned();
            dirty = true;
        }
        "path-add" => {
            let pi = find_path(map, arg(1)?)?;
            if args.len() >= 5 {
                let fi = find_file(idx, arg(2)?)?;
                let (start, end) = (num(3).ok_or("bad start line")?, num(4).ok_or("bad end line")?);
                if start < 1 || end < start || end > idx.files[fi].lines.len() {
                    return Err("line range out of bounds".into());
                }
                map.add_anchor(idx, pi, fi, start - 1, end - 1, author);
            } else {
                for r in find_symbols(idx, arg(2)?)? {
                    let s = idx.sym(r);
                    map.add_anchor(idx, pi, r.file, s.start, s.end, author);
                }
            }
            dirty = true;
        }
        "path-rm" => {
            let pi = find_path(map, arg(1)?)?;
            match num(2) {
                Some(ai) if ai < map.paths[pi].anchors.len() => {
                    map.paths[pi].anchors.remove(ai);
                }
                Some(_) => return Err("no such anchor".into()),
                None => {
                    map.paths.remove(pi);
                }
            }
            dirty = true;
        }
        "promote" => {
            let depth = num(2).unwrap_or(1);
            for r in find_symbols(idx, arg(1)?)? {
                let pi = map.promote(idx, r, depth, author);
                p!(out, "path '{}' now has {} anchors", map.paths[pi].name, map.paths[pi].anchors.len());
            }
            dirty = true;
        }
        other => return Err(format!("unknown command '{other}'\n{HELP}")),
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

fn print_lines(out: &mut String, f: &File, start: usize, end: usize) {
    for li in start..=end.min(f.lines.len().saturating_sub(1)) {
        p!(out, "{:5} {}", li + 1, f.lines[li]);
    }
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

fn find_path(map: &Map, name: &str) -> Result<usize, String> {
    map.find(name).ok_or(format!("no such path: {name}"))
}
