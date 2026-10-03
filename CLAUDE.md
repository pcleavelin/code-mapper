# codemap

The developer no longer writes the code; an AI does. The developer still has to understand
it: the workflows, the abstraction layers, the data structures. That understanding used to
be a side effect of writing the code, and it disappears when the writing is delegated.
codemap moves the building of that mental model from the human to the tool: the AI writes
the map as a side effect of writing the code, and the human reads the map instead of the
diff.

- **The agent** writes code and, in the same session, the map: which tours the change
  touched, what the new code is for, how the pieces relate. Its interface is the CLI.
- **The human** reads the map, browses the codebase through the same tool (files, search,
  symbols, references), and builds tours by hand where code is shown. Its interface is the GUI.

Goals: understanding an unfamiliar codebase, or a change you did not write, takes less
friction through codemap than through an editor with an LSP; the map is a second channel
beside the source, prose about workflows that overlap in the code they touch, grepped the way
source is; code no tour covers is visible, so the human can audit what the agent mapped; a
small, snappy native app.

Never: editing source from codemap (it reads code, it does not write it), any web or remote
surface, multi-root or workspaces. What is planned or deferred, and what would pull a
deferred item in, is in `TODO.md`.

**Every feature makes its case.** A feature is built only after a written case that would
convince the owner; that it looks good, or would be interesting to have, is not a case. The
case names the user (the agent or the human) and the moment they meet the problem; the goal
above that it serves; what going without it costs them today, seen in a run of the tool
rather than supposed; why nothing codemap already has answers it; and, once the prototypes
have run, why the kept design beats the others. It is the note of the feature's `features/`
tour. A request whose case cannot be written goes back to the owner with what is missing.

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
   knowledge has one home: what a feature does and why is the note of its `feature-<name>`
   tour, what a layer or data is for is its tour's note, the vocabulary and the reasons
   behind the design are below.
6. **Every test can fail.** A test names a mistake in the code it would catch. One whose
   expected value comes from the code under test, or that asserts what a type already
   guarantees, passes on every implementation and is deleted.
7. **Nothing is done until the gate is green.** The rules are enforced after every agent
   turn and before every commit.

## Concepts

| Term | Meaning |
|---|---|
| **Symbol** | A top-level declaration, plus one level of members of impl / mod / trait / class bodies. Name, kind, inclusive line range, depth 0 or 1. |
| **Xref** | Symbol A calls or references symbol B. Xrefs to a symbol are its callers, xrefs from it its callees. |
| **Anchor** | A pinned slice of lines in one file, stored relative to the innermost enclosing symbol (absolute when none), so it follows the symbol when code above it moves. Carries a hash of its text; when the hash no longer matches, it is **stale**. |
| **Tour** | A named tree of anchors (**steps**) with a kind, a group, a note, an author, and a note per step. The one unit of the mental model. Siblings show in the order the parent's code names them. |
| **Link** | A step naming another tour that documents what its lines call (shared code, or a queue another process reads). The step stays at the call site; the linked tour is read instead of copying its steps. |
| **Group** | Where a tour sits in the tours list, `/` nesting (`flows/http`). Orders the list, changes nothing else. |
| **Kind** | `flow`: what happens when X. `layer`: an abstraction boundary and the functions forming its surface (a module is a layer rooted at its file). `data`: a data structure and what mutates it. A tag only. |
| **Coverage** | A symbol is covered when a step's anchor overlaps it. Derived, never stored. |
| **Map** | All tours for one root: `.codemap/`, one text file per tour, committed with the code. The only thing persisted (the manual layer). Everything derived from source (files, symbols, xrefs, roots, call trees, coverage) is the auto layer, cached in `.codemap-cache`, never committed. |

A tour note describes the workflow as a whole; a step note says what the step does for this
tour. What is true of the code in every tour goes in the note of the `layer` or `data` tour
covering it: the code has no comments, so the map is the only prose about it.

## Why it is built this way

- **The map merges.** One file per tour, groups a field rather than directories or name
  prefixes, step ids a hash of what the step first pinned and never changed, steps written in
  id order, a stored `order` only `tour-swap` changes, no counts in the file: two branches that
  change different tours touch different files, two that change one tour touch different
  lines. The format has no version compatibility; the reader rejects any version it does not
  write and an old map is regenerated.
- **Nothing re-anchors on load.** The agent that changed the code has the diff and re-pins
  (`stale`, `repin`, `tour-pin`); the human never does. `repin` aligns the old slice against
  each symbol of the step's name with a patience diff and pins only when half the lines
  survive; it never vouches for a note.
- **One backend per language.** The language's server when on PATH, else a tree-sitter
  resolver written for that language. No language-agnostic resolver: its rules would be
  nobody's.
- **Index only what is asked.** `references` and `incomingCalls` are a search of the whole
  workspace each, so they are asked for one symbol when wanted, never while indexing. The CLI
  asks the server only about the files a command touches. The GUI merges server answers
  about once a second, since a merge re-resolves the map and drops every drawn grid.
- **Links go through a step**, not text in a note, so they are checked, renamed with their
  tour, and inlined. A tour that is linked to cannot be removed.
- **Roots are strictly "no callers".** **`promote` prunes by measured rules** (depth 2; tests,
  accessors and trivial bodies left out; shared, other-package and mapped callees kept as
  leaves), chosen by scoring against this repo's hand-written feature tours; it is still a
  scaffold the agent trims and annotates. **Coverage is observable, never a `check` failure.**
  **No review state on tours.**
- **The GUI is one selection** (a symbol, with a step behind it when reached through a tour);
  every view shows it, and the document, steps list and graph are three views of one thing that
  never disagree. The graph is derived (the selection plus an ordered list of reveals) and
  its camera moves only on explicit navigation.
- **The UI is its own library** (`crates/ui`, in the shape of odin_editor's): elements opened
  and closed each frame, layout once at frame end, input answered from the previous frame's
  rectangles, one monospace font at whole-pixel sizes in one atlas, icons as Codicons glyphs.

## Crates

`domain` (the model, no I/O), `io-*` (one crate per outside format or program: `io-map`
`.codemap/`, `io-cache`, `io-config` the user's layout and settings, `io-fonts` installed fonts,
`io-vcs` jj or git, `io-lsp`, `io-source`, `io-process`, `io-store`, `io-clipboard` the system
clipboard), `index` (tree-sitter resolvers, server orchestration), `features` (every function a
user can reach; CLI commands and help, GUI buttons and keys are built from it), `cli` (text
commands, also the GUI's Console), `ui` (the element tree), `platform` (wgpu, fonts, winit, the
script runner), `gui`, `codemap` (the binary and integration tests), `xtask` (the gate). The
allowed edges are in `xtask/src/arch.rs`. `cli` and `gui` are two front ends over one `Index`
and `Map`; anything that mutates the map is a method of `Map`.

## Explore codebases through codemap, not grep

When you need to understand any repo (this one included), use the tool instead of
Grep/Read. Build once, then:

```
cargo build --release -p codemap
target/release/codemap <root> <command> [args]
```

1. `tours` to see what is already named. `tour <name>` to read a code tour as one
   document instead of opening files (`--inline` reads linked tours inline). `notes <regex>`
   to search what earlier sessions wrote. The `features/` groups hold one flow per thing a
   user can do; `tooling` explains the gate.
2. `tree <sym>`, `callers <sym>`, `callees <sym>` to move along the graph, `refs <sym>`
   for every reference. `roots` for entry points. `show <file> [start] [end]` only for
   lines no tour covers. `uncovered` lists symbols in no tour, largest first.

`codemap help` prints the full command list. A save writes only the tours its command
changed, so sessions working on different tours at once keep each other's work; on one tour
the last writer wins. The same contract belongs in the `CLAUDE.md` of every mapped repo, with
the map upkeep the `finish` skill does here: `stale`, `repin`, every new non-trivial symbol
into a tour, `check` clean.

## Indexing

Xrefs come from the language's server when it is on PATH (`rust-analyzer` for this repo),
otherwise from a tree-sitter resolver. Results live in `.codemap-cache`, never committed.
Nothing indexes the whole repo up front: a command asks the server for the files it
touches, and `index <path>` asks for everything under a path, e.g. the crate you are about
to map. `callers` and `refs` ask the server about the one symbol. Delete the cache to force
a full re-query.

## Build and test

`cargo build`, `cargo test`, `cargo xtask gate` (`--full` adds the CLI scenarios). The GUI is
winit + wgpu + fontdue under the tool's own element tree (`crates/ui`); there is no UI
framework underneath.

`crates/codemap/tests` holds `cli.rs` (every command against a generated fixture), `gui.rs`
(scripted scenarios in real windows, one at a time), `features.rs` (every feature has its map
tour and a scenario that triggers it) and `parity.rs`. A scenario fails when codemap crashes or
a GUI script reports an error; nothing records its output in the repo. What a change does to
output is shown by `cargo xtask parity [scenario]`: it builds the parent revision (cached under
`target/parity`), plays every scenario on both builds, and lists each one whose transcript, GUI
dump or screenshot differs, keeping `old.txt` and `new.txt` beside it. A refactor lists none; a
change lists only the scenarios it meant to alter. Behaviour a change adds is pinned by a test
that states it, written before the code. `CODEMAP_BIN=<binary>` runs the tests against another
build.

## Testing the GUI without a hand on the mouse

Never claim a visual or interactive behaviour from reading the code; drive it and look.

- `CODEMAP_SHOT=<file.png> [CODEMAP_SHOT_TAB=tour|graph|source|diff] [CODEMAP_SHOT_SCROLL=n]
  target/release/codemap <root>` writes the first settled frame and quits.
- `CODEMAP_SCRIPT=<file> target/release/codemap <root>` plays a script, one command per
  line, as real input, on a clock of its own (8 ms a frame, plus each pause), so animations land
  the same on every run: `wait n` (frames), `pause ms` (waits that long in wall time, for the
  servers, and moves the clock by exactly that), `mouse x y`, `down`, `up`,
  `click x y [ctrl|alt|shift]`, `dblclick x y`, `drag x0 y0 x1 y1`, `wheel dy [ctrl|shift]`,
  `swipe dx dy` (a trackpad two-finger swipe, both axes in pixels),
  `pinch n` (a trackpad pinch of n percent, negative to zoom out),
  `key <name> [ctrl] [alt]`, `text ...`, `quit`; app commands `tab <name>`,
  `open <file> [line]`, `scroll <panel> <n>`, `idle` (waits until every server request is
  answered, merged and re-indexed), `rect <id>` (an element's rectangle by id name),
  `click-id <id> [ctrl|alt|shift]`, `dblclick-id <id>`, `hover-id <id>` (the same gestures
  aimed at an element's centre, refused when the centre lies outside the part of the screen the
  element shows in), `absent <id>` (fails only when the element is in view; passes when it is out of view or not drawn), `shot <file.png>`,
  `dump`. A `script:` line on stderr fails the scenario. Id names are the ones in
  `crates/gui/src/ids.rs`: `name`, `name/<n>` for rows, `name@<key>` (every view's tab
  `tab@<View>`, e.g. `tab@Graph`, `tab@Symbols`; document steps `step/<n>`, `collapse/<n>`,
  `inline/<n>`, `hide/<n>`, `whole/<n>`, `del/<n>`, `ctx-a/<n>`, `ctx-b/<n>`; tours list `tours/<n>`,
  `steps/<n>`; text fields `field@search`, `field@wizard-name`, `field@symbols`,
  `field@tours`, `field@goto-line`, `field@views`; symbol rows
  `sym@<file index>:<symbol index>`; references rows `xto/<i>`, `xfrom/<i>`, `xref/<i>`; panels
  by the number n `DUMP panels` prints: `panel/<n>`, header buttons `pick/<n>`,
  `split-right/<n>`, `split-down/<n>`, `close-panel/<n>`, dividers `divider/<n>`, tab close
  buttons `close-tab@<View>`; view picker
  rows `view/<n>`). Rects come from the previous frame, so `wait 1` after anything that
  changes the layout. `dump` prints the selection, scroll offsets, the panel tree, the
  tooltip and peek, the graph camera and every node's and node button's rectangle to stderr as
  `DUMP` lines (grep `^DUMP`), plus
  `DUMP frames n=<count> max=<ms> over16=<count> t=<ms since the window opened>` for the
  frames since the previous `dump`, and
  `DUMP backend progress=... indexing=... unmerged=<n> reindexing=<bool> linking=<bool>`.
  A script or screenshot run opens a fixed 1600x1000 window at scale 1 and ignores the real
  mouse and keyboard, so runs on every machine agree. It starts from the default
  panel layout and saves none, unless `CODEMAP_LAYOUT=<file>` names a layout file to load and
  save. Write script files with the
  Write tool and forward-slash paths: a heredoc mangles backslashes.
- On macOS a script or screenshot run starts with the activation policy `Prohibited`
  (`crates/platform/src/background.rs`), so its window renders without ever taking focus and
  runs can play while the machine is in use.
- On Linux, run GUI scenarios one at a time under a private headless compositor
  (`weston --backend=headless --renderer=pixman --shell=kiosk --socket=<name>`,
  `WAYLAND_DISPLAY=<name>`, a software Vulkan driver): a desktop window that is hidden stops
  getting frames and the script stalls.
- Read a PNG with the Read tool; crop or scale it first when the detail matters.
