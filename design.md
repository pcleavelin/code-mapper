# codemap — design

A tool that turns the mental map a programmer carries of a codebase into a file that
can be seen, edited, and queried. Used by humans through a native GUI and by AI agents
through a text interface over the same engine. The GUI model is IDA Pro: a listing you
read, a functions window you navigate from, cross-references everywhere, a graph of the
neighbourhood, and an output window with a command line.

## 1. Goals and constraints

Goals

- Every line of the project is one click (or one command) away.
- Named **code paths**: an ordered list of code slices, each smaller than a file and
  possibly smaller than a function, that together are what people mean by "the login
  flow" or "the render loop".
- An **automatic first pass** that proposes paths from the call graph, which the human
  or the AI then corrects, names, and annotates.
- **Structural queries** (who calls what, what is in this path, entry points) beside
  ripgrep-style text queries.
- A **machine interface** so an AI agent reads the codebase through the map instead of
  through grep and file reads.

Constraints (from the owner)

- Small and snappy native app. Rust + egui, single executable, no web stack.
- No JSON on disk. Bespoke binary formats only.
- Humans never hand-edit the data files. All editing goes through the GUI. The CLI is
  the AI's GUI, and the same commands are available inside the GUI's output window.

## 2. Core concepts

| Term | Meaning |
|---|---|
| **File** | A text file under the root, held in memory as lines. Tabs are expanded to 4 spaces on load, everywhere, so display and hashing agree. |
| **Symbol** | A top-level declaration found by tree-sitter (functions, structs, consts, impl blocks), plus one level of nesting for members of impl / mod / trait / class bodies. Has a name, kind, inclusive line range, and depth 0 or 1. IDA's "function". |
| **Xref** | Symbol A calls symbol B. Derived by name: every `*call*` node inside A whose callee's last path segment matches a symbol name. "Xrefs to" = callers, "xrefs from" = callees. |
| **Anchor** | A pinned slice of lines in one file. Stored relative to the enclosing symbol so it follows the symbol when code above it moves. Carries a hash of its text; if the hash no longer matches, the anchor is **stale** and shown in red. |
| **Path** | A named, ordered list of anchors with a free-text note and an author (human or AI). This is the unit of the mental model. |
| **Map** | All paths for one root. One file, `.codemap`, at the root. |
| **Auto layer** | Everything derived from source at startup: files, symbols, xrefs, roots, call trees. Never persisted; recomputed every launch. |
| **Manual layer** | The map. Authored by the human in the GUI or by the AI through the CLI. The only thing persisted. |

The two layers meet in one operation: **promote**. A call tree from the auto layer
becomes a path in the manual layer, and from then on it is the human's or the AI's to
trim, reorder, and annotate.

## 3. Architecture

```
src/
  index.rs   walk + parse + symbols + xrefs              (auto layer)
  map.rs     paths, anchors, binary format, staleness   (manual layer)
  cli.rs     text commands over index + map             (AI interface, and the GUI's command line)
  main.rs    egui app over index + map                  (human interface)
```

`index` and `map` know nothing about the UI. `cli` and `main` are two front ends over
the same two structs. Any operation that mutates the map lives on `Map` so both call
the same code. `cli::run` writes its output to a `String` so the GUI's output window
and the process's stdout are the same code path.

Startup, both front ends: build the index (walk, parse, link), load the map, resolve
every anchor against the index. The GUI then loops; the CLI runs one command, saves if
it mutated the map, and exits.

Invocation: `codemap <root>` opens the GUI. `codemap <root> <command> [args]` runs one
text command. The executable keeps a console subsystem so CLI output is visible; the
cost is a console window behind the GUI when launched from Explorer. One root per
process; open two windows for two projects.

## 4. Indexing (auto layer)

Walk: the `ignore` crate, so `.gitignore` and hidden directories are respected without
configuration. Files over 4 MB or with a NUL byte in the first 1 KB are skipped.

Languages: Rust, Odin, C, Python, JavaScript, TypeScript, TSX, chosen by extension.
Other text files are indexed for viewing and grep but have no symbols. Adding a
language is one line in `language_for` plus a grammar crate.

Symbol extraction is deliberately grammar-agnostic so it survives across languages:

1. Every named child of the root node is a candidate. Comments, imports, package and
   use declarations are skipped by node kind.
2. The name is the grammar's `name` field when present. Otherwise the first line of the
   node, split at `::` (Odin) and stripped of a trailing `{` (Rust impl, C functions).
3. Nodes whose kind contains impl, mod, trait, or class recurse one level into their
   `body` field with depth 1. These container nodes have no calls of their own.

Xref extraction walks each non-container symbol's subtree for nodes whose kind contains
`call`, takes the `function` field (or first named child), and keeps the last segment
after `.` or `::`, stripped of generics, macro bangs, and arguments.

Linking is name-based with no type information. For each raw callee name, the target is
the symbol of that name in the same file, else the first definition in path order.
Overloaded names like `new` will link wrong. This is accepted for v1; a per-language
scope resolver is the upgrade path and the data model does not change.

Derived queries: `callers`, `callees`, `roots`, and `call_tree` (pre-order,
depth-limited, each symbol once).

**Roots** are exactly the symbols with at least one callee and zero callers. A `main`
that something calls is not a root. No special cases.

## 5. Automatic mapping

An auto path is the call tree rooted at a root symbol, depth-limited. It is not stored;
the GUI lists roots in the Auto window as expandable trees, and the CLI prints them
with `roots` and `tree`.

**Promote** turns a call tree into a real path named after the root, with one anchor
per symbol in pre-order. Trimming anchors that don't belong, renaming, and writing the
note are the human's or the AI's job. This is the "materialize, then correct" loop the
tool exists for. The default depth is an open question (section 11).

Why not persist auto paths: they would go stale on every edit and drown the manual
layer. Derived data stays derived.

## 6. Map file format

`.codemap` at the root. Little-endian. Strings are `u32 len` + UTF-8 bytes.

```
"CMAP"  u32 version (=2)
u32 npaths
  str name
  str note
  u8  author        0 = human (GUI), 1 = AI (CLI)
  u32 nanchors
    str file        relative path, forward slashes
    str symbol      enclosing symbol name, "" = absolute lines
    i32 off_start   line offset from symbol start (or absolute line)
    i32 off_end     inclusive
    u64 hash        FNV-1a of the anchored lines joined with \n
    u8  author      0 = human, 1 = AI
```

Author is recorded on both the path and each anchor, so the human can review "paths
the AI created" and "anchors the AI added to my paths" separately. The GUI tags AI
items with `(ai)`; the CLI prints the same tag.

A version bump is required for any layout change; readers reject unknown versions
rather than guessing. Version 1 files (no author bytes) are not read; none exist
outside this repo's test run. Hand-rolled reader and writer, about 80 lines, no
serialization dependency.

## 7. Anchor semantics

Creating an anchor from lines `[ls, le]` in file F: pick the innermost symbol whose range
contains the slice, store offsets relative to its start, hash the slice text. If no
symbol contains it, offsets are absolute.

Resolving on load: find the file, find the symbol by name, add offsets, bounds-check,
rehash. Any failure marks the anchor stale. A stale anchor whose symbol still exists
keeps its resolved lines so the user can see where it was and re-pin it. A stale anchor
whose file or symbol is gone resolves to nothing and must be deleted or re-pinned.

There is no automatic re-anchoring. Red bar, human decides. Cheaper and more honest
than heuristics.

## 8. GUI (IDA Pro model)

The graph is the main view. Everything else navigates it or annotates it.

| IDA Pro | codemap |
|---|---|
| Graph view | **Graph**: a pan-and-zoom canvas (egui `Scene`). Every node is a symbol showing its code: a 12-line preview, expandable to the whole body. Callers sit one column left, callees one column right; each node has buttons to expand its own callers or callees, so the visible graph grows hop by hop along whatever the reader is following. Nodes drag by their title. A path is shown as a chain: step 1 leftmost, green numbered edges between consecutive steps, grey call edges between any two visible nodes. |
| Functions window | **Symbols** window: a filterable table of every symbol with kind, file and line. Click to focus the graph on it. |
| IDA View (listing) | **Listing** tab: the plain code view with line numbers and anchor bars, for reading beyond a node or selecting arbitrary lines. Reached from a node's "listing" button. |
| Xrefs to / from | **Xrefs** window for the focused symbol: callers and callees as lists. Click to focus. |
| Names / comments | **Paths** window: each path with its note (the comment), author tag, and its steps as a numbered ordered list with reorder buttons. "graph" shows the path as a chain. |
| Output window + command line | **Output** panel at the bottom running the same commands as the CLI. |
| Segments / navigation band | Not carried over. |

Layout: symbols and paths stacked on the left; graph, listing, and results as tabs in
the centre; xrefs on the right; output at the bottom. Panels are resizable. Docking
and detachable windows are deferred (section 10); fixed positions first.

Graph layout is layered: a node's column is its hop distance from the focus (or its
step index in a chain). Columns are laid out from the focus outward; within a column,
nodes are ordered by the average y of their neighbours in the column nearer the focus
(barycenter), then stacked with measured heights and a fixed gap, and the column is
centred on the focus row. Node width comes from the longest shown line, clamped so the
header buttons always fit. Layout reruns every frame until the user drags a node, and
resumes after any structural change or the "auto layout" button. Focusing a symbol not
on screen rebuilds the graph around it and centres the view at readable zoom.

Known rough edge: a call edge that points backwards (callee to the left of its caller,
which happens in chains) is drawn as the same forward S-curve and loops across the
canvas behind the nodes. Route or style backward edges when it bothers someone.

Interactions: focus a node by clicking its title; "+path" adds it as the next step of
the selected path; ▲▼ in the Paths window reorder steps; "promote" on an entry point
creates a path and shows it as a chain. In the listing, click a line, shift-click to
extend, "add selection to path". Ctrl+S saves; unsaved changes save on exit.

Deliberately absent for now: syntax highlighting, automatic edge routing, undo, a
folder picker (root comes from the command line).

## 9. AI interface

The CLI is the AI's way in, and the same commands run in the GUI's output panel.
Output is plain text, one item per line, in the `file:line` shapes grep users already
parse. Every mutation through the CLI saves immediately and is tagged author = AI.

```
files [filter]                        symbols [filter]
show <file> [start] [end]             grep <regex>
callers <sym>   callees <sym>         tree <sym> [depth]   roots [n]
paths           path <name>           promote <sym> [depth]
path-new <name> [note]                path-note <name> <note>
path-add <name> <file> <start> <end>  path-add <name> <sym>
path-rm <name> [anchor-index]
```

Intended agent workflow, which `CLAUDE.md` in this repo will state:

1. `roots` and `paths` first, to see the entry points and what has already been named.
2. `path <name>` to read a code path as one document instead of opening files.
3. `tree` and `callers` to move along the graph instead of grepping for a name.
4. `show` only for the lines a path doesn't cover.
5. After understanding something, record it: `promote`, `path-new`, `path-note`,
   `path-add`, `path-rm`. The map is the agent's notes, kept in the repo for the next
   session and for the human, who can filter on the `(ai)` tag to review them.

Concurrency: the GUI holds the map in memory and saves whole. The CLI loads, mutates,
saves whole. If both run at once, last writer wins. Rule for now: don't run the CLI
while the GUI has unsaved changes. A file-watch reload in the GUI is the fix if this
bites.

An MCP server is a later thin wrapper over the same functions once the CLI shape has
settled. Not before.

## 10. Deferred, and what would trigger each

| Item | Trigger |
|---|---|
| Dockable / detachable windows (egui_tiles) | the fixed layout gets in the way of a real session |
| Multi-hop graph with a layout engine | the one-hop neighbourhood graph is not enough to see a path |
| Background indexing with progress | startup on a real repo takes more than a second |
| Multi-threaded grep | a search takes more than 100 ms |
| Virtualized symbols table | more than ~20k symbols |
| Type-aware xref resolution | wrong `new`/`get` links make auto paths misleading in practice |
| Anchor-level notes | a path note stops being enough to explain a slice |
| Edges between paths | paths start referring to each other in their notes |
| Syntax highlighting | reading code in the tool feels worse than in an editor |
| GUI reload on file change | last-writer-wins actually loses work |
| MCP server | the CLI is stable and the shell round trip is the bottleneck |

## 11. Open questions

- **Promote depth.** When you promote `main`, the new path can contain either `main`
  plus the functions it calls directly (maybe 6 anchors, you add more by hand), or
  `main` plus everything reachable within N calls (maybe 80 anchors, you delete the
  extras). Which default? Proposed: direct callees only, with `promote <sym> <depth>`
  for more.

Resolved: roots are strictly "no callers". Author is recorded per path and per anchor.
One root per process.

## 12. Status

Everything in sections 3 to 9 is implemented and verified: index with xrefs, roots and
call trees (unit-tested), map format v2 with author bytes (round-trip and staleness
unit-tested), the CLI, the GUI in the IDA layout with the output panel running CLI
commands, and `CLAUDE.md`.

Verified on two repos. The odin_editor repo: 94 files, `promote main` produced a
29-anchor path, xrefs and trees matched a manual reading. This repo, mapped by the
CLI itself: `update` and `cli_main` are promoted and annotated with `(ai)` tags, and
editing `update` after promoting it correctly flagged that anchor stale.

Known rough edges, all accepted per section 4 and 10: `new`, `get` and similar names
link to the first definition; `Key` shows as a callee of `main` in Odin because a
struct literal parses as a call-shaped node.
