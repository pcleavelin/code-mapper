# codemap

Read `design.md` first. It is the source of truth for what this tool is and why.

Every change ends with the `finish` skill. The Stop hook runs `cargo xtask gate` and keeps the
session going until it is green, and commits are refused while it is red; each failure says
what to do. Every feature request and bug fix starts with the `understand` skill, and every
new feature is chosen from several built by the `prototype` skill. The other skills
(`add-feature`, `fix-bug`, `add-cli-command`, `add-gui-element`, `add-domain-type`,
`add-data-source`, `add-test-scenario`, `refactor`, `tripped`) are the steps for each kind of
change.

## Axioms

Nothing tells an agent ahead of time how code must be shaped: a lint or a type rejects every
other shape and its message names the one way, and a skill carries every multi-step
procedure. An agent learns a rule by breaking it and reading the error, at the moment the
rule applies.

1. **One way.** Each piece of data has exactly one function that reads it from outside the
   program and one that writes it out, and each concept has exactly one type. A second way to
   get or send the same data does not compile or does not pass the gate.
2. **The architecture is checked, not described.** Layering, data ownership and the I/O
   boundary are enforced by the compiler where it can (crates, privacy) and by the
   architecture linter (`cargo xtask lint`) where it cannot. Nothing about the shape of
   the code is left to reading.
3. **No comments.** What a comment would say goes into a type, a name, a test, or the map.
   A comment is text the compiler does not check; when it drifts from the code, a reader
   trusts the wrong one.
4. **Types carry meaning.** Every value that means something has its own type, even when it
   is one integer. Data cannot be handed to a parameter that was not shaped for it.
5. **One description.** Each concept has one name in code, CLI, GUI and docs, and each kind of
   knowledge has one home.
6. **Nothing is done until the gate is green.** The rules are enforced after every agent
   turn and before every commit.

## Explore codebases through codemap, not grep

When you need to understand any repo (this one included), use the tool instead of
Grep/Read. Build once, then:

```
cargo build --release -p codemap
target/release/codemap <root> <command> [args]
```

1. `paths` to see what is already named. `path <name>` to read a code path as one
   document instead of opening files (`--expand` reads linked paths inline). `notes <regex>`
   to search what earlier sessions wrote. The `features/` groups hold one flow per thing a
   user can do; `tooling` explains the gate.
2. `tree <sym>`, `callers <sym>`, `callees <sym>` to move along the graph, `refs <sym>`
   for every reference. `roots` for entry points. `show <file> [start] [end]` only for
   lines no path covers. `uncovered` lists symbols in no path, largest first.

`codemap help` prints the full command list. A save writes only the paths its command
changed, so sessions working on different paths at once keep each other's work; on one path
the last writer wins.

## Indexing

Xrefs come from the language's server when it is on PATH (`rust-analyzer` for this repo),
otherwise from a tree-sitter resolver. Results live in `.codemap-cache`, never committed.
Nothing indexes the whole repo up front: a command asks the server for the files it
touches, and `index <path>` asks for everything under a path, e.g. the crate you are about
to map. `callers` and `refs` ask the server about the one symbol. Delete the cache to force
a full re-query.

## Build and test

`cargo build`, `cargo test`, `cargo xtask gate` (`--full` adds the CLI goldens). The GUI is
winit + wgpu + fontdue under the tool's own element tree (`crates/ui`); there is no UI
framework underneath.

`crates/codemap/tests` holds `cli.rs` (every command against a generated fixture, golden
transcripts), `gui.rs` (scripted scenarios in real windows, one at a time), `features.rs`
(every feature has its map path and a scenario that triggers it) and `parity.rs`. A change
that means to alter output reblesses with `CODEMAP_BLESS=1` and the golden diff is part of the
commit. A change that means not to (a refactor) proves it against the build before it:

```
CODEMAP_BASE_BIN=<old codemap> CODEMAP_PARITY_REV=<old rev> cargo test --release -p codemap --test parity -- --ignored
```

compares every transcript, GUI dump and screenshot byte for byte; `CODEMAP_PARITY_ONLY=<name>`
narrows it to one scenario. `CODEMAP_BIN=<binary>` runs the tests against another build.

## Testing the GUI without a hand on the mouse

Never claim a visual or interactive behaviour from reading the code; drive it and look.

- `CODEMAP_SHOT=<file.png> [CODEMAP_SHOT_TAB=path|graph|listing|diff] [CODEMAP_SHOT_SCROLL=n]
  target/release/codemap <root>` writes the first settled frame and quits.
- `CODEMAP_SCRIPT=<file> target/release/codemap <root>` plays a script, one command per
  line, as real input: `wait n` (frames), `pause ms` (wall time), `mouse x y`, `down`, `up`,
  `click x y [ctrl|alt|shift]`, `dblclick x y`, `drag x0 y0 x1 y1`, `wheel dy [ctrl|shift]`,
  `pinch n` (a trackpad pinch of n percent, negative to zoom out),
  `key <name> [ctrl] [alt]`, `text ...`, `quit`; app commands `tab <name>`,
  `open <file> [line]`, `scroll <panel> <n>`, `idle` (waits until every server request is
  answered, merged and re-indexed), `rect <id>` (an element's rectangle by id name),
  `click-id <id> [ctrl|alt|shift]`, `dblclick-id <id>`, `hover-id <id>` (the same gestures
  aimed at an element's centre), `shot <file.png>`, `dump`. Id names are the ones in
  `crates/gui/src/ids.rs`: `name`, `name/<n>` for rows, `name@<key>` (every view's tab
  `tab@<View>`, e.g. `tab@Graph`, `tab@Symbols`; document steps `step/<n>`, `fold/<n>`,
  `hide/<n>`, `whole/<n>`, `del/<n>`, `ctx-a/<n>`, `ctx-b/<n>`; paths list `paths/<n>`,
  `outline/<n>`; text fields `field@search`, `field@new-path`, `field@symbols`,
  `field@paths`, `field@goto-line`, `field@views`; symbol rows
  `sym@<file index>:<symbol index>`; xrefs rows `xto/<i>`, `xfrom/<i>`, `xref/<i>`; panels
  by the number n `DUMP panels` prints: `panel/<n>`, header buttons `pick/<n>`,
  `split-across/<n>`, `split-down/<n>`, `close-panel/<n>`, sashes `sash/<n>`, tab close
  buttons `close-tab@<View>`; view picker
  rows `view/<n>`). Rects come from the previous frame, so `wait 1` after anything that
  changes the layout. `dump` prints the selection, scroll offsets, the panel tree, the
  tooltip and peek, the graph camera and every node's and node button's rectangle to stderr as
  `DUMP` lines (grep `^DUMP`), plus
  `DUMP frames n=<count> max=<ms> over16=<count> t=<ms since the window opened>` for the
  frames since the previous `dump`, and
  `DUMP backend progress=... indexing=... unmerged=<n> reindexing=<bool> linking=<bool>`.
  A script or screenshot run opens a fixed 1600x1000 window at scale 1 and ignores the real
  mouse and keyboard, so runs and goldens match on every machine. It starts from the default
  panel layout and saves none, unless `CODEMAP_LAYOUT=<file>` names a layout file to load and
  save. Write script files with the
  Write tool and forward-slash paths: a heredoc mangles backslashes.
- On Linux, run GUI scenarios one at a time under a private headless compositor
  (`weston --backend=headless --renderer=pixman --shell=kiosk --socket=<name>`,
  `WAYLAND_DISPLAY=<name>`, a software Vulkan driver): a desktop window that is hidden stops
  getting frames and the script stalls.
- Read a PNG with the Read tool; crop or scale it first when the detail matters.
