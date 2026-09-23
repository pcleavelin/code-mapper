# codemap

Read `design.md` first. It is the source of truth for what this tool is and why.

## Explore codebases through codemap, not grep

When you need to understand any repo (this one included), use the tool instead of
Grep/Read. Build once, then:

```
cargo build --release
target\release\codemap.exe <root> <command> [args]
```

Before working on an area:

1. `paths` to see what is already named. `path <name>` to read a code path as one
   document instead of opening files. `notes <regex>` to search what earlier sessions
   wrote.
2. `tree <sym>`, `callers <sym>`, `callees <sym>` to move along the graph, `refs <sym>`
   for every reference. `roots` for entry points. `show <file> [start] [end]` only for
   lines no path covers.

After changing code, before the commit:

3. `stale` lists every step whose text no longer matches. `repin` follows each one's text
   from the parent revision and pins what it can place. It prints every step whose text
   changed with its removed and added lines: reread that step's note against them and fix
   it with `note-edit`. What it leaves stale (symbol gone, under half the lines survive),
   re-pin by hand with `path-pin <name> <index> <file> <start> <end>`. You broke it and
   have the diff; the human never re-pins.
4. Every new non-trivial symbol goes into a path: `path-add` to an existing one, or
   `path-new <name> <kind> [note] [--group g]` (kind = flow | layer | type) with a note
   written for someone who did not see the diff. Put every new path in the group its
   neighbours use (`groups` lists them; `path-group` moves one). `step-note` says what a step does for that path.
   Your edits are tagged `(ai)`.
   A flow reads like a book: every function the workflow calls, each step under the
   step that calls it, in call order. Scaffold it with `promote <sym> [depth]`, then trim
   and annotate; when adding by hand, always give `under` explicitly and read the
   "does not call" note `path-add` prints. `path-move <name> <index> <under>` fixes a
   misplaced step, `path-swap` orders siblings the code does not order (layers, types),
   `note-edit` fixes one clause of a note, `paths <name>` shows one path's step indices. `path <name>` shows the result; check that it reads top-down.
   Code that several paths call gets its own path, mapped once. In each path that
   reaches it, the step at the call site links to it with `step-link <name> <index>
   <target>` instead of repeating its steps; `path <name> --expand` reads the result
   with every linked path inline.
5. `check` must pass. It exits non-zero on any stale step or any link to a missing path.

After a rebase the same applies: run `stale`, then `repin <rev>` with the change's
pre-rebase commit (from `jj evolog`), then re-pin the rest.

`uncovered` lists symbols in no path, largest first, so the human can audit what you
judged trivial. `coverage` prints covered/total per file. `codemap help` prints the
full command list. Do not run CLI mutations while the GUI has unsaved changes (last
writer wins).

## Constraints

- No JSON anywhere on disk. The map is a bespoke binary file, see design.md section 8.
  Layout changes are free: no migration code, old maps are regenerated.
- Humans edit only through the GUI. The CLI is the AI's interface and the GUI's
  command line runs the same code.
- Keep it small and snappy. No new dependencies for what a few lines can do.
- Anything mutating the map goes on `Map` so the GUI and CLI share it.

## Indexing

Xrefs come from the language's server when it is on PATH (`rust-analyzer` for this repo),
otherwise from a tree-sitter resolver. Results live in `.codemap-cache`, never committed;
a cold run waits for the server, a warm one answers from the cache. Delete the cache to
force a full re-query.

## Build

`cargo build`, `cargo test`. The GUI is winit + wgpu + fontdue with the tool's own element
tree (`src/ui.rs`); there is no UI framework underneath.

`cargo test` includes `tests/cli.rs` (every command against a generated fixture, golden
transcripts) and `tests/gui.rs` (scripted scenarios in real windows, one at a time). A change
that means to alter output reblesses with `CODEMAP_BLESS=1` and the golden diff is part of the
commit. A change that means not to (a refactor) proves it against the build before it:

```
CODEMAP_BASE_BIN=<old codemap.exe> CODEMAP_PARITY_REV=<old rev> cargo test --release --test parity -- --ignored
```

compares every transcript, GUI dump and screenshot byte for byte; `CODEMAP_PARITY_ONLY=<name>`
narrows it to one scenario.

## Testing the GUI without a hand on the mouse

Never claim a visual or interactive behaviour from reading the code; drive it and look.

- `CODEMAP_SHOT=<file.png> [CODEMAP_SHOT_TAB=path|graph|listing|diff] [CODEMAP_SHOT_SCROLL=n]
  target\release\codemap.exe <root>` writes the first settled frame and quits.
- `CODEMAP_SCRIPT=<file> target\release\codemap.exe <root>` plays a script, one command per
  line, as real input: `wait n`, `mouse x y`, `down`, `up`, `click x y [ctrl|alt|shift]`,
  `dblclick x y`, `drag x0 y0 x1 y1`, `wheel dy [ctrl|shift]`, `key <name> [ctrl] [alt]`,
  `text ...`, `quit`; app commands `tab <name>`, `open <file> [line]`, `scroll <panel> <n>`,
  `idle` (waits until every server request is answered, merged and re-indexed), `rect <id>` (an element's
  rectangle by id name), `click-id <id> [ctrl|alt|shift]`, `dblclick-id <id>`, `hover-id <id>`
  (the same gestures aimed at an element's centre), `shot <file.png>`, `dump`. An id is the
  name passed to `ui::id`, `name/<n>` for `id_n`, `name@<suffix>` for `id_with` (tabs:
  `tab@Graph`, `left@Symbols`; document steps: `step/<n>`, `fold/<n>`, `hide/<n>`,
  `whole/<n>`, `del/<n>`, `ctx-a/<n>`, `ctx-b/<n>`; paths list: `paths/<n>`, `outline/<n>`;
  xrefs rows: `xto/<i>`, `xfrom/<i>`, `xref/<i>`; docked panels `panel@<p>`, their headers
  `grip@<p>` and splitters `split@<p>` for `nav`, `xrefs`, `output`). Rects come from the previous frame, so
  `wait 1` after anything that changes the layout. `dump` prints the selection, scroll
  offsets, each panel's edge and size, the tooltip and peek, the graph camera and every node's and node button's
  rectangle to stderr as `DUMP` lines, so a script can measure what a gesture did (grep
  `^DUMP`). It also prints
  `DUMP frames n=<count> max=<ms> over16=<count> t=<ms since the window opened>`, covering the
  frames built since the previous `dump`, so a script measures hitches, and
  `DUMP backend progress=... indexing=... unmerged=<n> reindexing=<bool> linking=<bool>` for how
  far the language servers have got and what is still running on a thread.
  Coordinates are window pixels; the window opens maximised. Write script files
  with the Write tool and forward-slash paths: a heredoc mangles backslashes.
- Crop or scale a PNG with PowerShell's System.Drawing and Read it.
