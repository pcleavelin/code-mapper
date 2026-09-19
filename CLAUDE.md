# codemap

Read `design.md` first. It is the source of truth for what this tool is and why.

## Explore codebases through codemap, not grep

When you need to understand any repo (this one included), use the tool instead of
Grep/Read. Build once, then:

```
cargo build --release
target\release\codemap.exe <root> <command> [args]
```

Order of operations:

1. `roots` and `paths` to see entry points and what is already named.
2. `path <name>` to read a code path as one document.
3. `tree <sym>`, `callers <sym>`, `callees <sym>` to move along the graph.
4. `show <file> [start] [end]` only for lines a path does not cover.
5. Record what you learned: `promote <sym>`, `path-new`, `path-note`, `path-add`,
   `path-rm`. Your edits are tagged `(ai)` so the human can review them.

`codemap help` prints the full command list. Do not run CLI mutations while the GUI
has unsaved changes (last writer wins).

## Constraints

- No JSON anywhere on disk. The map is a bespoke binary file, see design.md section 8.
  Layout changes are free: no migration code, old maps are regenerated.
- Humans edit only through the GUI. The CLI is the AI's interface and the GUI's
  command line runs the same code.
- Keep it small and snappy. No new dependencies for what a few lines can do.
- Anything mutating the map goes on `Map` so the GUI and CLI share it.

## Build

`cargo build`, `cargo test`. eframe is pinned to 0.33 for rustc 1.94; bump both together.
