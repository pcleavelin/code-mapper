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
| **Path** | A named tree of anchors (**steps**), with a **kind**, a path note, an author, and a step note per step. The one unit of the mental model. Siblings are shown in the order the parent's code names them. |
| **Link** | A step can name another path that documents what the step's lines call: the call into shared code, or the write to a queue that another process reads. The step stays anchored at the call site; the linked path is read in place of repeating its steps. A path cannot link to itself. Renaming a path renames every link to it, and a path that something links to cannot be removed. |
| **Group** | Where a path sits in the paths list: a name, with `/` nesting one group in another (`flows/http`). One group per path, or none. A group exists while a path is in it. Groups order the list and change nothing else. |
| **Kind** | `flow`: a workflow, what happens when X. `layer`: an abstraction boundary, the functions that form its surface. `type`: a data structure and what mutates it. A module is a layer whose root is the file. Kinds are a tag: listed and filterable, no rendering difference. |
| **Coverage** | A symbol is covered if any step's anchor overlaps its line range. Derived, never stored. |
| **Map** | All paths for one root. A directory, `.codemap/`, at the root, one text file per path, committed with the code. |
| **Auto layer** | Everything derived from source: files, symbols, xrefs, roots, call trees, coverage. Cached on disk for startup speed, never committed. |
| **Manual layer** | The map. Written by the agent through the CLI, occasionally by the human through the GUI. The only thing persisted. |

What goes in a note: the path note describes the workflow as a whole; a step note
says what this step does for this path. Anything true of the code regardless of which
path you read it in goes in the note of the `layer` or `type` path covering that code: the
code itself carries no comments, so the map is the only prose about it. The same function can
be a step in several paths and play a different role in each.

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

3. `stale` lists every step whose text no longer matches. `repin` places what it can
   and prints each changed step for its note to be reread; `path-pin` places the rest.
   The agent broke it and has the diff; the human never re-pins.
4. Every new non-trivial symbol goes into a path: `path-add` to an existing one, or
   `path-new <name> <kind>` with a note written for someone who did not see the diff.
   "Trivial" is the agent's judgement; the human audits it through `uncovered`.
   Code that many paths call is mapped once, in its own path, and each step that calls
   it gets `step-link <name> <index> <target>` instead of a copy of its steps.
5. `check` must pass. It exits non-zero on any stale step or any link to a missing path.

After a rebase the same rule applies: the map is stale, run `stale`, then `repin <rev>` from
the pre-rebase commit, then re-pin the rest.

Merging a branch merges its map with the rest of the code (section 8). A conflict is
confined to the field both sides changed; resolve it in the file like any other conflict,
then run `check`, which refuses to read a map with a conflict left in it.

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

Below the top bar the window is a tree of **panels**. A split divides its area in two, side
by side or one above the other, at a ratio a sash between the halves changes; a leaf is a
panel holding a stack of **views** as tabs, one of them shown. There are ten views and each
is in at most one panel. The default tree:

| Panel | Views |
|---|---|
| Left, tabs | **Paths**: the groups as folders, each with the number of paths under it and red when one of them is stale, then the paths outside any group; every path with kind, author tag and stale count. A group is closed until the path being read is in it, and a click opens or closes it. The selected path expanded into its **outline**, one row per step with hierarchical number (1, 1.2, 1.2.3), symbol and file, a hidden count on folded subtrees. The topmost step visible in the document is highlighted and the outline scrolls to keep it in view. Clicking a row selects the step. **Symbols**: filterable table with kind, file, line, covered. **Files**: a tree of the indexed files with covered/total per file. |
| Centre, tabs | **Path** document, below. **Diff**: the map against the parent revision's, every added, removed or changed path, click to read. **Graph**: the selection as a left-to-right tree, below. **Listing**: the file viewer with line numbers, anchor bars, and go-to-line. **Results**: grep output. In the document and the listing, double-click or ctrl-click an identifier to jump to its definition. |
| Right | **Xrefs** for the selected symbol. |
| Bottom | **Output**: runs the same commands as the CLI. |

Each panel's header holds its tabs and four buttons: **+** opens a list of every view with a
search field (typing narrows it, enter or a click puts that view in this panel, moving it from
wherever it was), **|** and **-** split the panel side by side or one above the other and open
the list on the new empty half, and **x** closes the panel, its sibling taking its place (the
last panel cannot close). Dragging a tab onto another panel's middle adds the view to that
panel's tabs; onto the outer quarter of an edge, it splits that panel and the view takes the
half on that side; anywhere else cancels. Each tab has its own **x**, which takes that view
out of the layout. A panel whose last view is dragged away or closed closes too, except the
last panel, which stays empty. Navigation that means another centre view (opening a path
shows Path, a symbol off the path shows Listing, a search shows Results) brings that view's
tab to the front wherever it is, and puts it back next to the last one navigation showed if it
was closed.

The layout is saved whenever it changes (once a drag is let go), to one file per user:
`$XDG_CONFIG_HOME/codemap/layout`, else `~/.config/codemap/layout`, else
`%APPDATA%\codemap\layout`, or wherever `CODEMAP_LAYOUT` points. A launch starts from it,
and from the default when it is missing or unreadable. The file is indented text, one split
(`across <share>` or `down <share>`, the share in thousandths) or `panel <View> <View>*` per
line, `*` marking the tab in front. Script and screenshot runs use the file only when
`CODEMAP_LAYOUT` names one, so goldens never depend on a user's layout.

**The document.** A sticky breadcrumb of the topmost visible step's ancestors, name and
file per crumb, each clickable. Then the steps in tree order: a header with the
hierarchical number, symbol, file:lines and tags; the note as text (editing notes is the
agent's, through the CLI, until the UI grows a text editor); the anchored lines inline, syntax coloured, full length. Per step: collapse the
code, fold the subtree (the header shows how many steps are hidden), a whole-symbol
toggle that shows the enclosing symbol with the slice highlighted inside it, and context
buttons at the top and bottom of the code that show ten more lines of the file each
press, the way a diff hunk expands; whenever anything beyond the slice shows, the slice
is highlighted. A step with a link shows the linked path's name, which opens that path, and
an expand toggle that shows the linked path's steps inline under the step, indented one
level and numbered after it (`1.2 › 1`, `1.2 › 1.1`), with their own view toggles. Clicking
one of those steps opens it in its own path. A link to a path already open further up
is marked "expanded above" and does not expand again. Under the path note, "linked from"
lists every step that links to this path, each clickable. The outline shows a link as
`→ name` after the step. Path-wide
hide all code, show all, fold all, unfold all. Up and down walk the steps when no field
has the keyboard. Stale steps are red. The selected step carries an accent
bar; selecting it from outside the document scrolls its header to the top, selecting it
inside does not scroll. Selecting an off-path symbol leaves the document and outline on
the path with no step highlighted; back returns to the step. The flat step index the
CLI uses is not shown.

**Peek.** In any code view, hovering an identifier shows what the language's server
knows about it: signature, type, docs, for anything the server resolves, a local, a
field, a macro, an item of a dependency. Ctrl-click or double-click goes to its
definition; alt-click pins the definition in the right panel above the xrefs. A
definition inside the repo lands on the symbol, or on the line when it is inside one
(a field, a local); one outside the repo is shown from the file on disk in the panel
and cannot be jumped to. A language with a grammar but no server falls back to the
symbol of that name; prose files have neither, so hovering a word in them shows nothing.

**The graph.** The selected path drawn as a left-to-right tree: root left, children to
the right, siblings stacked in call order, every node showing its code in full (a node
can be cut to a preview, or widened with the same context buttons as the document, its
own lines highlighted). Edges leave a node level with the line that makes the call,
and that line is tinted; a slice node's callees are the ones its lines name. Callers and
callees expansions per node reveal off-path nodes, drawn distinctly, hanging left or
right of the step; a revealed symbol that is also a step is one node with an extra
edge. Green step edges, grey expansion edges, orange back-edges, no edge labels. With no
path selected the graph is the symbol and its expansions; a symbol selected off the path
while one is being read joins the tree as its own root, callers and callees open. The graph is
derived, never accumulated: the selection plus an ordered list of expansions. Layout
reruns every frame until the user drags a node. The camera moves only on explicit
navigation, never on an edit.

Editing in the GUI is limited to what a reader needs: delete a step or a path, and pin
a selection from the listing when hand-authoring. `roots` and `promote`
are CLI commands and run from the output panel.

If a file in `.codemap/` changed on disk and there are no unsaved edits, it reloads. If any
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
unaffected. `callHierarchy/outgoingCalls` per symbol for xrefs; callers are those
inverted. Each stage (a batch of files' symbols, every symbol's call-hierarchy item, every
item's outgoing calls) is sent whole, with up to 64 requests in flight, so the server
works on them on its own threads. `textDocument/references` and
`callHierarchy/incomingCalls` are asked for one symbol when they are wanted (`refs`,
`callers`, the focused symbol in the GUI) and never while indexing: each is a search of
the whole workspace, and asking it for every symbol costs the number of symbols times the
occurrences of their names. rust-analyzer starts with cache priming off, so it is ready
once the workspace is loaded and computes what it is asked when it is asked. A tree-sitter
resolver provides symbols and calls for its language, as well as it can.

**The live session.** The GUI keeps one server per language running for the life of
the window, on its own thread. It indexes the files it is sent a batch at a time and
answers `textDocument/hover` and `textDocument/definition` for the pointer between
files, so hovering never waits behind a batch. Answers are keyed by file hash and
position and kept for the session. Answers
are merged into the index about once a second rather than as they arrive, since a merge
re-resolves the map and drops every drawn grid; the re-link that follows, the watch for source
changes and the rebuild after one each run on their own thread, so no frame waits on the size
of the repo.

**The cache.** Servers take seconds to warm up. Every result is written to
`.codemap-cache` at the root, a bespoke binary file, per source file, keyed by the
file's content hash. It is derived from the backend and never committed; it exists so
the app opens at once and only files that changed are re-queried. The GUI opens on the
cache and greys out what the cache cannot answer until the server has answered;
progress is shown.

**The CLI indexes what a command touches.** No command indexes the repo up front. One that
needs a file's calls (`callees`, `tree`, `promote`, and `path-add` and `path-move` for the
does-not-call note) asks the server for just those files, one depth of a call tree at a
time, and caches the answers; `callers` and `refs` ask about the one symbol; everything else
answers from the cache and tree-sitter without starting a server. A server is started at
most once per command. `index [filter]` asks for every file under a path at once, which is
how a session readies the crate it is about to map.

Syntax highlighting is tree-sitter for every language regardless of backend, run once
per file and cached the same way.

**Derived queries.** Callers, callees, references. **Roots** are exactly the symbols
with at least one callee and zero callers. **Call trees** are pre-order,
depth-limited, each symbol once. **Promote** turns a call tree into a path shaped like
it, default depth 1; it is a scaffold the agent then trims and annotates. Auto paths
are never persisted.

## 8. Map file format

`.codemap/` at the root: one text file per path, `<name>.cmap`, committed with the code.
Hand-rolled reader and writer, no serialization dependency, no JSON anywhere on disk. The
layout exists so that the map merges: two branches that change different paths never touch
the same file, and two that change one path touch different lines unless they edit the same
field of the same step.

```
codemap 8
path <name>
kind flow | layer | type
author ai | human        human = GUI, ai = CLI
group <group>            optional; / between nested groups
note <text>              optional

step <id>
order <n>                the step's place in the step list
parent <id>              optional; a root has none
author ai | human
file <path>              relative, forward slashes
symbol <name>            optional; none = absolute lines
lines <start> <end>      offsets from the symbol's first line, or absolute; inclusive
hash <16 hex digits>     FNV-1a of the anchored lines joined with \n
link <path name>         optional
note <text>              optional
```

One field per line, `key value`; the value runs to the end of the line, with `\\`, `\n` and
`\r` escaping a backslash, a newline and a carriage return. A blank line ends each block.
Empty optional fields are left out, and nothing in the file is a count.

A step's id is six base36 digits, a hash of what the step pins when it is created, and it
never changes: a step names its parent by id, so adding or removing a step changes no other
step's lines. `order` is set once, one past the highest in the path, and only
`path-swap` changes it; the step list is the steps sorted by order, ties by id. Steps are
written in id order, so the steps two branches add to one path land at unrelated places in
the file and merge without a conflict, and two branches that add the same step add the same
lines. Paths come back in file name order. The CLI's step indices are places in the step
list, shown for addressing and never stored.

A path name is also a file name: letters, digits, `.`, `_` and `-`, not starting with a dot,
and never the name of another path in other letter case. A name `promote` takes from a symbol
has every other character turned into `-`.

Saving writes each path's file only when its text changed, and removes the files of paths
that are gone; an unchanged map saves to identical bytes. A file with a merge conflict in it,
or any file that does not read, stops every command with `file:line` and the reason, and the
GUI does not save over it, since a save would rewrite what did not read.

Tree order is derived (roots in list order, children in list order, pre-order). Removing a
step moves its children up to its parent.

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

Nothing re-anchors on load. The agent that changed the code re-pins; `stale` says where a
step's unchanged text now sits when it merely moved, so that re-pin is one command.

`repin [rev]` does the mechanical part for every stale step at once. It reads the step's
file from `rev` (default: the parent revision) through jj or git, finds the old slice by its
hash, and aligns it,
with three lines of context, against each symbol of the step's name in its file, or in every
file once the file or the symbol is gone from it, so code moved between files is followed.
The alignment is a patience diff: lines unique to both sides pair first, in the order both
agree on, and pairs grow into equal neighbours only outward from a pair. Scoping to one
symbol means a moved function is followed. Pairing on unique lines keeps neighbouring
functions from interleaving the way a minimal line diff can. The best alignment wins. A step
is pinned only when at least half its lines survive; otherwise it stays stale with the reason
printed. Every pinned step whose text changed is printed with its removed and added lines,
so the note gets reread against them. `repin` never vouches for a note.

Replaying the tool's own audit refactor (152 stale steps) placed 123, all where a hand
re-pin put them. The other 29 were gone symbols or rewrites.

## 10. Architecture and the CLI

```
crates/
  domain/       the model, no I/O: positions and text, the index model and its queries,
                the map model (paths, steps, anchors, groups, diff, following moved text),
                the panel layout as a value
  io-process/   starting a program (PATH lookup included)
  io-store/     writing a file whole (temporary file, rename over)
  io-source/    the walk and reading source files
  io-map/       .codemap/*.cmap: the text format and the store that loads and saves it
  io-cache/     .codemap-cache: the binary format
  io-layout/    the user's saved panel layout: the text format and the store
  io-vcs/       jj or git: the parent revision, a file or the map directory at a revision
  io-lsp/       a minimal language-server client: JSON-RPC over stdio
  index/        building the index: tree-sitter resolvers and highlighting, server
                orchestration, call sites resolved into callees and callers
  features/     every function the app offers a user: name, summary, triggers
  cli/          text commands over index + map (agent interface, and the GUI's output panel)
  ui/           the element tree: open/close, Exact/Fit/Grow, layout passes, one-frame-late
                input, geometry, a draw list
  platform/     the GPU (wgpu), fonts, the window (winit), the event loop, the test-script
                runner, screenshots
  gui/          the app (human interface): widgets, theme, ids, keys, actions, navigation,
                the document, peek, graph, the panel tree, the background runtime
  codemap/      the binary: CLI or GUI; the integration tests (cli, gui, parity)
xtask/          the gate: archlint, the crate graph, the API lock, the hooks
```

Each I/O surface has wire types that mirror the external format and one `convert` into the
domain types, so the program's internals change without changing a format. Every function a
user can reach is a `Feature` in `crates/features`; the CLI's commands and help, and the GUI's
buttons and key bindings, are built from it.

The UI is its own library, in the shape of odin_editor's `ui`: every frame the app opens
and closes elements (nothing, text, or custom drawing) whose sizes are exact, fit their
content, or grow; layout runs once at the end of the frame; input answers from the
previous frame's rectangles. One monospace font at whole-pixel sizes, every glyph in
one GPU atlas, so text is never scaled. Icons are glyphs too: `ui::Icon` names a
Private Use Area codepoint of Codicons (`assets/codicon.ttf`, CC BY 4.0), so an icon is a
character in an ordinary label and sits in the same run as text; it is two cells wide,
rasterised from the icon font into the same atlas, centred in its two cells, and dumps
spell it `[name]`. No button draws its symbol with a text character. `CODEMAP_SHOT=<file.png>` writes the first
settled frame to a file and quits, and `CODEMAP_SCRIPT=<file>` plays mouse and keyboard
input from a script, waits for the servers when told to, and dumps state and element
rectangles on request, so rendering and interaction claims are checked against pixels
and numbers, never against the code alone.

`domain` knows nothing about the UI or I/O. `cli` and `gui` are two front ends over the same
`Index` and `Map`. Any operation that mutates the map is a method of `Map` so both call the
same code. `cli::exec` writes into a buffer so the output panel and stdout share one code
path.

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
paths [name]    path <name> [--expand]                promote <sym> [depth] [name]
path-new <name> <kind> [note]         path-note <name> <note>       path-rename <name> <new>
step-note <name> <index> <note>       note-edit <name> <index> <old> <new>
step-link <name> <index> <target>     step-unlink <name> <index>
path-group <name> <group>             groups                group-rename <old> <new>
path-rm <name> [index]
path-add <name> <sym> [under]         path-add <name> <file> <start> <end> [under]
path-pin <name> <index> <file> <start> <end>
path-move <name> <index> <under>      path-swap <name> <a> <b>
stale           check                 uncovered [filter]    coverage
diff
```

`path-add` places the new step under `under`; by default under the last step added, so
consecutive adds build a chain and an explicit index starts a branch.

`path <name> --expand` prints each linked path inline under the step that links to it, the
same way the document expands it; `paths` and `path` mark a link as `→ name`, and `path`
lists the steps that link to the path it prints.

`paths` lists the paths in the same order as the Paths tab, with a `== group` line (or
`== (top level)`) wherever the group changes; `groups` prints the group tree with counts;
`path-new --group` places a new path directly.

Concurrency: the GUI holds the map in memory; the CLI loads, mutates and saves. A save
writes only the paths its process changed and removes only the ones it removed, against
what it read, so processes that change different paths (agents mapping different areas at
once, or the CLI beside the GUI) keep each other's work. On one path the last writer wins.

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
(`@-` in jj, `HEAD` in git; the GUI shells out, no VCS library). Done when: the
owner reviews an agent session's map changes without reading `path` output.

**M5 — prompts in the code.** The owner leaves a marker comment in a source file where
a question or a request for the map belongs (`// codemap: why does this retry three
times?`). `prompts` lists every marker with file:line and text; the agent contract tells
a session to answer each one in the map (a note, a step, a path) and remove the marker
in the same commit; the GUI shows the open markers in the listing and counts them in the
status bar. Done when: the owner writes a question in a comment, the next agent
session's map answers it, and the marker is gone from the diff.

Deferred, with the trigger that would pull each in:

| Item | Trigger |
|---|---|
| A daemon holding the server sessions for the CLI | cold CLI runs on changed files are the bottleneck of an agent session |
| Hand-written resolver for a language | a target repo uses it and has no server |
| Links in the graph: a linked step expands the linked path's tree from its node | reading a journey across processes in the document is not enough |
| "Intentionally unmapped" marker on symbols | `uncovered` is mostly things already decided not to matter |
| Step-level review state | a long path gets one re-pinned step and rereading it all is a cost |
| Kind-specific rendering | a list of 50 mixed-kind paths is unreadable |
| Documentation panel for the focused symbol | per language: doc comments first, then external docs |
| Detachable windows | one window's panel tree is not enough for a real session |
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
- Groups are a field of the path, not a prefix of its name, so moving a path between
  groups leaves its name and every link to it alone.
- Paths link to each other through a step, not through text in a note, so a link can be
  checked, renamed with its path, and expanded. Asked for by the owner for mapping
  pilot-api-rs, where shared code (history, locks, validation) and work handed between
  processes through queues would otherwise be copied into every path that reaches it.
- Promote default depth is 1; the agent asks for more.
- Roots are strictly "no callers". One root per process.
- The map is text, one file per path, because it has to merge once more than one person
  maps a repo. Reviewing map changes is still a GUI feature (M4); the text makes them
  readable in a jj or git diff too.
- The VCS is jj when a `.jj` directory is at or above the root, else git. The parent
  revision is `@-` in jj and `HEAD` in git; both are reached through their command line.
- Groups are a field in the path's file, not directories: moving a path to another group
  is then a one-line change that merges with anything else, where a file move on one branch
  conflicts with an edit on another.
- Coverage is observable, never a `check` failure.
- Indexing is per language: the language server when present, a resolver written for
  that language otherwise. No language-agnostic resolver.
- The index cache is derived from the backend and exists only for startup speed. It
  is never committed and never the source of truth.
