# codemap — design

The developer no longer writes the code; an AI does. The developer still has to
understand it: the workflows, the abstraction layers, the data structures. Today that
understanding is a side effect of writing the code, and it disappears when the writing
is delegated. codemap moves the building of that mental model from the human to the
tool: the AI writes the map as a side effect of writing the code, and the human reads
the map instead of the diff.

This document is the source of truth. It is written to be read by an agent at the
start of a session and by the owner when deciding what to build next.

## 1. Users and roles

- **The agent** writes code and, as part of the same session, writes the map: which
  paths the change touched, what the new code is for, how the pieces relate. Its
  interface is the CLI.
- **The human** reads the map to understand what was built, and uses the same tool to
  browse the codebase in general (files, grep, symbols, xrefs). Hand-authoring paths is
  possible but rare. Its interface is the GUI.

## 2. Goals

- Understanding an unfamiliar codebase, or a change you did not write, takes less
  friction through codemap than through an editor with an LSP.
- The map is a second channel beside the source: high-level notes about workflows that
  do not fit as code comments because workflows overlap in the code they touch. The
  agent greps it the way it greps source.
- Code that no path covers is visible, so the human can audit whether the agent mapped
  what it built.
- Small and snappy native app.

## 3. Non-goals

Permanent: editing source code from codemap; any web or remote surface; multi-root or
workspaces. The tool reads code, it does not write it.

Deferred items and their triggers are in section 11.

## 4. Concepts

| Term | Meaning |
|---|---|
| **File** | A text file under the root, held in memory as lines. Tabs are expanded to 4 spaces on load, everywhere, so display and hashing agree. |
| **Symbol** | A top-level declaration (functions, structs, consts, impl blocks), plus one level of nesting for members of impl / mod / trait / class bodies. Has a name, kind, inclusive line range, and depth 0 or 1. Found by the language's backend, section 7. |
| **Xref** | Symbol A calls symbol B, or symbol A references symbol B. From the language's backend, section 7. "Xrefs to" = callers, "xrefs from" = callees. |
| **Anchor** | A pinned slice of lines in one file, stored relative to the enclosing symbol so it follows the symbol when code above it moves. Carries a hash of its text; if the hash no longer matches, the anchor is **stale**. |
| **Path** | A named tree of anchors (**steps**), with a **kind**, a path note, an author, and a step note per step. The one unit of the mental model. |
| **Kind** | `flow`: a workflow, what happens when X. `layer`: an abstraction boundary, the functions that form its surface. `type`: a data structure and what mutates it. A module is a layer whose root is the file. Kinds are a tag: listed and filterable, no rendering difference. |
| **Coverage** | A symbol is covered if any step's anchor overlaps its line range. Derived, never stored. |
| **Map** | All paths for one root. One file, `.codemap`, at the root, committed with the code. |
| **Auto layer** | Everything derived from source: files, symbols, xrefs, roots, call trees, coverage. Cached on disk for startup speed, never committed. |
| **Manual layer** | The map. Written by the agent through the CLI, occasionally by the human through the GUI. The only thing persisted. |

What goes in a note: the path note describes the workflow as a whole; a step note
says what this step does for this path. Anything true of the code regardless of which
path you read it in is a code comment, not a note. The same function can be a step in
several paths and play a different role in each.

## 5. The agent contract

This goes in the `CLAUDE.md` of every mapped repo. It is what makes the map exist
without anyone asking for it.

Before working on an area:

1. `paths` to see what is already named. `path <name>` to read a code path as one
   document instead of opening files. `notes <regex>` to search what earlier sessions
   wrote.
2. `tree`, `callers`, `callees` to move along the graph. `show` only for lines no path
   covers.

After changing code, before the commit:

3. `stale` lists every step whose text no longer matches. Re-pin each with `path-pin`.
   The agent broke it and has the diff; the human never re-pins.
4. Every new non-trivial symbol goes into a path: `path-add` to an existing one, or
   `path-new <name> <kind>` with a note written for someone who did not see the diff.
   "Trivial" is the agent's judgement; the human audits it through `uncovered`.
5. `check` must pass. It exits non-zero on any stale step.

After a rebase the same rule applies: the map is stale, run `stale`, re-pin. There is
no merge story and none is needed.

Coverage is observable, not enforced. `uncovered` lists symbols in no path, largest
first, so what remains after a session is either pre-existing or something the agent
judged trivial, and size says which. `coverage` prints covered/total per file.

## 6. The human surface (GUI)

Reader first. One **selection** for the whole app: a symbol, with a step behind it when
it was reached through a path. Steps are selected from the outline, the document, the
breadcrumb, or a graph node that is a step; symbols alone from the Symbols tab, xrefs,
jumps and off-path graph nodes. Every view shows the selection; nothing selected means
every view is empty. The document, outline and graph are three views of one thing and
never disagree.

| Panel | Contents |
|---|---|
| Left, tabs | **Paths**: every path with kind, author tag and stale count; the selected path expanded into its **outline**, one row per step with hierarchical number (1, 1.2, 1.2.3), symbol and file, a hidden count on folded subtrees. The topmost step visible in the document is highlighted and the outline scrolls to keep it in view. Clicking a row selects the step. **Symbols**: filterable table with kind, file, line, covered. **Files**: a tree of the indexed files with covered/total per file. |
| Centre, tabs | **Path** document, below. **Diff**: the map against the parent revision's, every added, removed or changed path, click to read. **Graph**: the selection as a left-to-right tree, below. **Listing**: the file viewer with line numbers, anchor bars, and go-to-line. **Results**: grep output. In the document and the listing, double-click or ctrl-click an identifier to jump to its definition. |
| Right | **Xrefs** for the selected symbol. |
| Bottom | **Output**: runs the same commands as the CLI. |

**The document.** A sticky breadcrumb of the topmost visible step's ancestors, name and
file per crumb, each clickable. Then the steps in tree order: a header with the
hierarchical number, symbol, file:lines and tags; the note as text, double-click to
edit; the anchored lines inline, syntax coloured, full length. Per step: collapse the
code, fold the subtree (the header shows how many steps are hidden), and a whole-symbol
toggle that shows the enclosing symbol with the slice highlighted inside it. Path-wide
collapse all and expand all. Stale steps are red. The selected step carries an accent
bar; selecting it from outside the document scrolls its header to the top, selecting it
inside does not scroll. Selecting an off-path symbol leaves the document and outline on
the path with no step highlighted; back returns to the step. The flat step index the
CLI uses is not shown.

**Peek.** In any code view, hovering an identifier that is defined in the repo shows its
definition; alt-click pins it in the right panel above the xrefs.

**The graph.** The selected path drawn as a left-to-right tree: root left, children to
the right, siblings stacked in call order, every node showing its code in full (a node
can be cut to a preview). Edges leave a node level with the line that makes the call,
and that line is tinted; a slice node's callees are the ones its lines name. Callers and
callees expansions per node reveal off-path nodes, drawn distinctly, hanging left or
right of the step; a revealed symbol that is also a step is one node with an extra
edge. Green step edges, grey expansion edges, orange back-edges, no edge labels. With no
path selected the graph is the symbol and its expansions; a symbol selected off the path
while one is being read joins the tree as its own root, callers and callees open. The graph is
derived, never accumulated: the selection plus an ordered list of expansions. Layout
reruns every frame until the user drags a node. The camera moves only on explicit
navigation, never on an edit.

Editing in the GUI is limited to what a reader needs: edit a note, delete a step or a
path, and pin a selection from the listing when hand-authoring. `roots` and `promote`
are CLI commands and run from the output panel.

If `.codemap` changed on disk and there are no unsaved edits, it reloads. If any
indexed source file changed, it re-indexes and carries the graph over by
(file, symbol) identity. New or deleted files need a restart.

## 7. Indexing (auto layer)

Walk: the `ignore` crate, so `.gitignore` and hidden directories are respected. Files
over 4 MB or with a NUL byte in the first 1 KB are skipped. Other text files are
indexed for viewing and grep but have no symbols.

**One backend per language.** Each language is indexed by its language server when
one is on PATH, otherwise by a hand-written tree-sitter resolver for that language.
There is no language-agnostic resolver: a language's rules are the server's rules, or
a resolver written for that language when a target repo needs it and has no server.

| Language | Server (on PATH) |
|---|---|
| Rust | `rust-analyzer` |
| Odin | `ols` |
| C | `clangd` |
| Python | `pyright-langserver` |
| JavaScript / TypeScript / TSX | `typescript-language-server` |

No config file. A different server goes on PATH under that name. A language whose
server is missing says so once in the output and uses its resolver; a language with
neither has symbols only if its grammar is bundled, and no xrefs.

**What the server provides.** `textDocument/documentSymbol` for symbols, filtered to
top level plus one level of nesting so the shape matches section 4 and anchors are
unaffected. `callHierarchy/outgoingCalls` per symbol for xrefs.
`textDocument/references` for references to a symbol: what a `type` path means by
"what touches this struct", and what the GUI uses to jump from an identifier to its
definition. A tree-sitter resolver provides the same three things for its language,
as well as it can.

**The cache.** Servers take seconds to warm up. Every result is written to
`.codemap-cache` at the root, a bespoke binary file, per source file, keyed by the
file's content hash. It is derived from the backend and never committed; it exists so
the app opens at once and only files that changed are re-queried. The GUI opens on the
cache and greys out what the cache cannot answer until the server has answered;
progress is shown. The CLI answers from the cache when the files involved are current,
otherwise waits for the server.

Syntax highlighting is tree-sitter for every language regardless of backend, run once
per file and cached the same way.

**Derived queries.** Callers, callees, references. **Roots** are exactly the symbols
with at least one callee and zero callers. **Call trees** are pre-order,
depth-limited, each symbol once. **Promote** turns a call tree into a path shaped like
it, default depth 1; it is a scaffold the agent then trims and annotates. Auto paths
are never persisted.

## 8. Map file format

`.codemap` at the root. Little-endian. Strings are `u32 len` + UTF-8 bytes. Hand-rolled
reader and writer, no serialization dependency, no JSON anywhere on disk.

```
"CMAP"  u32 version
u32 npaths
  str name
  u8  kind          0 = flow, 1 = layer, 2 = type
  str note
  u8  author        0 = human (GUI), 1 = AI (CLI)
  u32 nsteps
    str file        relative path, forward slashes
    str symbol      enclosing symbol name, "" = absolute lines
    i32 off_start   line offset from symbol start (or absolute line)
    i32 off_end     inclusive
    u64 hash        FNV-1a of the anchored lines joined with \n
    u8  author
    str note
    i32 parent      index of the parent step in this list, -1 = root
```

Steps are stored in list order; tree order is derived (roots in list order, children
in list order, pre-order). Removing a step moves its children up to its parent.

The tool is in development: the layout changes whenever it needs to, the reader
rejects any version it does not write, and an old map is regenerated rather than
migrated. No backward-compatibility code.

## 9. Anchor semantics

Creating an anchor from lines `[ls, le]` in file F: pick the innermost symbol whose
range contains the slice, store offsets relative to its start, hash the slice text. If
no symbol contains it, offsets are absolute.

Resolving on load: find the file, find the symbol by name, add offsets, bounds-check,
rehash. Any failure marks the anchor stale. A stale anchor whose symbol still exists
keeps its resolved lines so the reader can see where it was. A stale anchor whose file
or symbol is gone resolves to nothing and must be re-pinned or deleted.

There is no automatic re-anchoring. The agent that changed the code re-pins; `stale` says
where a step's unchanged text now sits when it merely moved, so that re-pin is one command.

## 10. Architecture and the CLI

```
src/
  index.rs   walk, per-language backends, cache, derived queries  (auto layer)
  lsp.rs     a minimal language-server client: JSON-RPC over stdio, used by index.rs
  map.rs     paths, anchors, binary format, staleness             (manual layer)
  cli.rs     text commands over index + map                       (agent interface, and the GUI's output panel)
  main.rs    egui app over index + map                            (human interface)
```

`index` and `map` know nothing about the UI. `cli` and `main` are two front ends over
the same two structs. Any operation that mutates the map lives on `Map` so both call
the same code. `cli::run` writes to a `String` so the output panel and stdout share one
code path.

`codemap <root>` opens the GUI. `codemap <root> <command> [args]` runs one command,
saves if it mutated the map, and exits. One root per process. Output is plain text,
one item per line, in the `file:line` shapes grep users already parse. Every CLI
mutation is tagged author = AI.

Arguments are parsed with `clap` (derive). The same `Command` enum is parsed from the
process arguments and from the output panel's input line, so `help` and argument
errors read identically in both.

```
files [filter]                        symbols [filter]
show <file> [start] [end]             grep <regex>          notes <regex>
callers <sym>   callees <sym>         refs <sym>
tree <sym> [depth]                    roots [n]
paths [name]    path <name>           promote <sym> [depth] [name]
path-new <name> <kind> [note]         path-note <name> <note>       path-rename <name> <new>
step-note <name> <index> <note>       note-edit <name> <index> <old> <new>
path-rm <name> [index]
path-add <name> <sym> [under]         path-add <name> <file> <start> <end> [under]
path-pin <name> <index> <file> <start> <end>
path-move <name> <index> <under>
stale           check                 uncovered [filter]    coverage
diff
```

`path-add` places the new step under `under`; by default under the last step added, so
consecutive adds build a chain and an explicit index starts a branch.

Concurrency: the GUI holds the map in memory and saves whole; the CLI loads, mutates,
saves whole. Last writer wins. Do not run the CLI while the GUI has unsaved changes.

## 11. Roadmap

Each milestone has a done-when that can be checked without a judgement call. Work
outside the current milestone needs a reason written here first.

**M1 — the agent loop.** CLI parsing moved to `clap`, `kind` on paths,
`path-new <name> <kind>`, `notes`, `stale`, `path-pin`, `check`, `uncovered`,
`coverage`, and the contract of section 5 in this repo's `CLAUDE.md`. Done when: after an agent session on this repo and on
odin_editor, `check` passes, nothing the session added appears in `uncovered`, and the
owner can say what the session changed from `paths` and `path <name>` output alone,
without reading the diff.

**M2 — language-server indexing.** Per-language backends, the server table, symbols
+ outgoing calls + references from the server, the `.codemap-cache`, `refs`, and the
GUI opening on the cache with greyed-out queries and progress until the server
answers. The grammar-agnostic resolver is retired language by language as each gets
a server or its own resolver. Done when: on this repo with `rust-analyzer` on PATH,
`callees update` and `refs Map` match what the same server shows in an editor; with it
off PATH, the Rust resolver's output is what it is today; and a second launch opens
without waiting on the server.

**M3 — the reader GUI.** Path document as the landing view, Files tree, coverage in
Symbols and Files, click an identifier to jump to its definition, Auto window and
graph-side authoring buttons removed. Done when: the M1 test passes in the GUI
without a terminal.

**M4 — map diff in the GUI.** The working map against the map at the parent revision
(`jj file show -r @- .codemap`; the GUI shells out, no VCS library). Done when: the
owner reviews an agent session's map changes without reading `path` output.

Deferred, with the trigger that would pull each in:

| Item | Trigger |
|---|---|
| A daemon holding the server sessions for the CLI | cold CLI runs on changed files are the bottleneck of an agent session |
| Hand-written resolver for a language | a target repo uses it and has no server |
| "Intentionally unmapped" marker on symbols | `uncovered` is mostly things already decided not to matter |
| Step-level review state | a long path gets one re-pinned step and rereading it all is a cost |
| Kind-specific rendering | a list of 50 mixed-kind paths is unreadable |
| Documentation panel for the focused symbol | per language: doc comments first, then external docs |
| Dockable / detachable windows | the fixed layout gets in the way of a real session |
| Multi-threaded grep | a search takes more than 100 ms |
| Watch for new / deleted files | restarting for new files annoys |
| Undo | a mis-click deletes something that took effort to build |
| MCP server | the CLI is stable and the shell round trip is the bottleneck |
| More languages | a target repo needs one |

## 12. Resolved

- The map is the product; the GUI and CLI are two editors for it.
- One concept (paths) with a kind tag, not separate concepts for layers and types.
- No review state on paths. If the human feels unfamiliar with something they know it
  and read it.
- Author is recorded per path and per step, shown as `(ai)`.
- Promote default depth is 1; the agent asks for more.
- Roots are strictly "no callers". One root per process.
- The map file stays binary. Reviewing map changes is a GUI feature (M3), not a format
  property.
- Coverage is observable, never a `check` failure.
- Indexing is per language: the language server when present, a resolver written for
  that language otherwise. No language-agnostic resolver.
- The index cache is derived from the backend and exists only for startup speed. It
  is never committed and never the source of truth.
