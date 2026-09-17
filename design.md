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

The IDA Pro affordances that carry over, and what each maps to here:

| IDA Pro | codemap |
|---|---|
| Functions window | **Symbols** window: a filterable table of every symbol with file, kind, line range, xref counts. Click to jump. |
| IDA View (listing) | **Listing**: the code view. Monospace, line numbers, anchor bars in the gutter. |
| Xrefs to / from | **Xrefs** window for the symbol under the cursor: callers on one side, callees on the other. Click to jump. Always visible, never a popup. |
| Graph view | **Graph** tab: the selected symbol in the centre, callers as a column on the left, callees as a column on the right, edges drawn between. Click a node to recentre. One hop each way; no layout engine. |
| Names / comments | **Paths** window: paths with note, author tag, and anchors. The note is the comment. |
| Output window + command line | **Output** panel at the bottom with a command line that runs the same commands as the CLI, output appended above it. |
| Segments / navigation band | Not carried over. |

Layout: symbols and paths stacked on the left; listing, graph, and results as tabs in
the centre; xrefs on the right; output at the bottom. Panels are resizable. Docking
and detachable windows are deferred (section 10); fixed positions first.

Interactions: click a line in the listing to select, shift-click to extend, "add
selection to path" puts it on the selected path. Selecting a path highlights its
anchors in the listing. Promote is a button on an Auto root and a command. Ctrl+S
saves; unsaved changes save on exit. Enter in the command line runs the command.

Deliberately absent for now: syntax highlighting, multi-hop graph layout, undo, drag
reordering of anchors, a folder picker (root comes from the command line).

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
