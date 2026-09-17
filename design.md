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
| **Path** | A named **tree** of anchors ("steps"): each step records the step it is reached from, or none for a root. A path has a free-text note and an author (human or AI). This is the unit of the mental model: a code path is what one entry point does, which branches, not a line. |
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
`call`, takes the `function` field (or first named child), strips generics and
argument lists, and keeps the called name plus its qualifier: the segment before it.
`a.b.c()` is `c` via `b`; `Foo::new()` is `new` via `Foo`; `self.f()`, `Self::f()`,
`this.f()` are `f` via self. Members of impl / class bodies record their owner type
(`impl X for Y` gives `Y`). Import statements (`use`, `import`, `from ... import`) are
parsed loosely into a per-file table from imported name or alias to the module it came
from, the same string rules for every language.

Linking resolves each call in order, without types:

1. Via self: a member of the caller's owner type; else same file.
2. Via `Q`: a member of type `Q`; else a symbol in module `Q` (a file stem or directory
   named `Q`, so Odin packages and Rust modules both work); else via the import table.
   If `Q` is capitalised and none of that matched, it is an external type and the call
   links nowhere. If `Q` is a lowercase receiver variable, some type's member: the
   caller's own type first, else any member, else anything.
3. Unqualified: a free function in the same file; else the file the import table maps
   the name to; else a free function in the same directory (package). Only C goes on
   to any free function anywhere (headers); in other languages an unqualified name
   that is not local or imported is a builtin or a library call and links nowhere.

Struct, enum, and union declarations are never call targets, since `Foo{...}` parses as
a call in Odin. Still wrong when two of this repo's types share a method name and the
receiver is a variable of unknown type; the language-server route is the upgrade if
that matters in practice.

Derived queries: `callers`, `callees`, `roots`, and `call_tree` (pre-order,
depth-limited, each symbol once).

**Roots** are exactly the symbols with at least one callee and zero callers. A `main`
that something calls is not a root. No special cases.

## 5. Automatic mapping

An auto path is the call tree rooted at a root symbol, depth-limited. It is not stored;
the GUI lists roots in the Auto window as expandable trees, and the CLI prints them
with `roots` and `tree`.

**Promote** turns a call tree into a real path named after the root, with one step per
symbol and each step under the step it is called from, so the path has the shape of
the call tree. Trimming steps that don't belong, renaming, and writing the notes are
the human's or the AI's job. This is the "materialize, then correct" loop the tool
exists for. Default depth is 1 (the root and what it calls directly); `promote <sym>
<depth>` for more.

Why not persist auto paths: they would go stale on every edit and drown the manual
layer. Derived data stays derived.

## 6. Map file format

`.codemap` at the root. Little-endian. Strings are `u32 len` + UTF-8 bytes.

```
"CMAP"  u32 version (=4)
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
    str note        v3+: the step's own annotation
    i32 parent      v4+: index of the parent step in this list, -1 = root
```

Author is recorded on both the path and each anchor, so the human can review "paths
the AI created" and "anchors the AI added to my paths" separately. The GUI tags AI
items with `(ai)`; the CLI prints the same tag. A step note is the comment on one
step; the path note is the comment on the whole path.

Steps are stored in list order; tree order is derived (roots in list order, children
in list order, pre-order). Removing a step moves its children up to its parent.
Swapping two steps in the list reorders siblings and rewrites parent links so the tree
is unchanged.

A version bump is required for any layout change. Readers accept versions 2 to 4 (a
v2/v3 path was a chain, so each step's parent becomes the step before it) and reject
anything else rather than guessing. Hand-rolled reader and writer, no serialization
dependency.

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
| Graph view | **Graph**: a pan-and-zoom canvas (egui `Scene`). Every node is a symbol showing its syntax-coloured code: a 12-line preview, expandable to the whole body. Callers sit one column left, callees one column right; each node has buttons to open or close its own callers or callees, so the visible graph grows hop by hop along whatever the reader is following. Nodes drag by their title. A path is shown as a tree in the same view: its root at the focus, each step one column right of its parent, green numbered edges from parent to child, each step's note printed under its header, grey call edges between any other two visible nodes. The same expansion buttons work on steps, so the reader explores around a path without leaving it. |
| Functions window | **Symbols** window: a filterable table of every symbol with kind, file and line. Click to focus the graph on it. |
| IDA View (listing) | **Listing** tab: the plain code view with line numbers and anchor bars, for reading beyond a node or selecting arbitrary lines. Reached from a node's "listing" button. |
| Xrefs to / from | **Xrefs** window for the focused symbol: callers and callees as lists. Click to focus. |
| Names / comments | **Paths** window: each path with its note (the comment), author tag, and its steps as an indented tree, numbered in pre-order, with buttons to reorder siblings. Selecting a step opens its own note for editing and makes it the parent of the next step added; steps with a note are marked `*`. "graph" shows the path as a tree. |
| Output window + command line | **Output** panel at the bottom running the same commands as the CLI. |
| Segments / navigation band | Not carried over. |

Layout: symbols and paths stacked on the left; graph, listing, and results as tabs in
the centre; xrefs on the right; output at the bottom. Panels are resizable. Docking
and detachable windows are deferred (section 10); fixed positions first.

What the graph shows is derived, never accumulated: a base (the focused symbol, or a
path's tree of steps) plus an ordered list of expansions, each "callers of X" or
"callees of X". The buttons on a node toggle its expansion; closing one rebuilds the
node set from the list, so anything that was only reachable through it disappears
with it.

Layout is a forest of compact subtrees. Horizontally, a step's column is its depth in
the path tree and any other node's column is its hop distance from what it was
expanded from; columns share x, each as wide as its widest node, anchored at the
focus. Vertically, every node owns a block: its own height, or the stacked heights of
its children's blocks if that is taller. Children stack beside their parent (steps and
callee expansions to the right, caller expansions to the left) and the parent centres
on them. Sibling blocks never interleave, so a wide subtree only pushes its own
siblings, never its cousins. Roots (the focus, other path roots, anything without a
parent) stack top to bottom. A last per-column pass pushes apart the rare collision
between different subtrees (a caller's callees land in the focus column, say). Node
width comes from the longest shown line, clamped between a header-fitting minimum and
a maximum; lines longer than that are cut with an ellipsis so a frame never exceeds
its column. Layout reruns every frame until the user drags a node, and resumes after
any structural change or the "auto layout" button.

The view moves only on an explicit navigation: focusing a symbol, opening a path,
selecting a step. Editing the path (adding a step from a node, deleting, reordering,
a CLI edit picked up by reload) rebuilds the node set in place and leaves the camera
where it is.

Edges leave a caller's header on the right and arrive at the callee's header on the
left. A callee that sits left of or level with its caller gets a short leftward curve
in a second colour. Tree edges are green and labelled with the child's step number.

Interactions: focus a node by clicking its title; "+path" adds it as a step of the
selected path under the selected step (or under the focused step, or as a root) and
selects the new step, so repeated adds build a chain and clicking another step starts
a branch; up/down in the Paths window reorder siblings; "promote" on an entry point
creates a path shaped like its call tree. In the listing, click a line, shift-click to
extend, "add selection to path". Ctrl+S saves; unsaved changes save on exit.

Syntax colours come from each grammar's bundled tree-sitter highlight query, run once
per file at index time and stored as per-line spans; capture names map onto seven
colour classes (keyword, string, comment, function, type, constant, property).

The GUI polls once a second. If `.codemap` changed on disk and there are no unsaved
edits, it reloads the map and rebuilds the graph, so a note written by the CLI shows
up live. With unsaved edits it warns once and leaves the choice to the user. If any
indexed source file's modification time changed, it re-indexes and carries the graph
over by (file, symbol) identity; new or deleted files are only noticed on restart.

Deliberately absent for now: automatic edge routing, undo, a folder picker (root comes
from the command line).

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
step-note <name> <index> <note>       path-rm <name> [index]
path-add <name> <sym> [under]         path-add <name> <file> <start> <end> [under]
```

`path-add` places the new step under step `under`; by default under the last step
added, so consecutive adds build a chain and an explicit index starts a branch.

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
| Language-server xref resolution (rust-analyzer, ols) | the qualifier-based resolver's misses matter in practice |
| Click a call in a node's code to open that callee | reading a node and wanting one specific callee, not all of them |
| Documentation panel for the focused symbol | per language: doc comments first (tree-sitter), then external docs (rustdoc, odin docs) |
| Dockable / detachable windows (egui_tiles) | the fixed layout gets in the way of a real session |
| Background indexing with progress | startup on a real repo takes more than a second |
| Multi-threaded grep | a search takes more than 100 ms |
| Edges between paths | paths start referring to each other in their notes |
| Watch for new / deleted files | editing sessions add files often enough that restarting annoys |
| Undo | a mis-click deletes something that took effort to build |
| MCP server | the CLI is stable and the shell round trip is the bottleneck |

Done since the list was written: multi-hop layered graph layout, virtualized symbols
table, syntax highlighting, step notes, reload on map or source change, paths as
trees.

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

Known rough edges, accepted per sections 4 and 10: a method called on a variable of
unknown type binds to the caller's own type first, then the first type that has it;
platform-conditional duplicates (`font_darwin.odin` and `font_windows.odin` both
defining a proc) link to whichever file sorts first.
