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

3. `stale` lists every step whose text no longer matches. Re-pin each with
   `path-pin <name> <index> <file> <start> <end>`. You broke it and have the diff; the
   human never re-pins.
4. Every new non-trivial symbol goes into a path: `path-add` to an existing one, or
   `path-new <name> <kind> [note]` (kind = flow | layer | type) with a note written for
   someone who did not see the diff. `step-note` says what a step does for that path.
   Your edits are tagged `(ai)`.
   A flow reads like a book: every function the workflow calls, each step under the
   step that calls it, in call order. Scaffold it with `promote <sym> [depth]`, then trim
   and annotate; when adding by hand, always give `under` explicitly and read the
   "does not call" note `path-add` prints. `path-move <name> <index> <under>` fixes a
   misplaced step. `path <name>` shows the result; check that it reads top-down.
5. `check` must pass. It exits non-zero on any stale step.

After a rebase the same applies: run `stale`, re-pin.

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

`cargo build`, `cargo test`. eframe is pinned to 0.33 for rustc 1.94; bump both together.
