# codemap — conventions (proposal, rev 2)

Status: proposal for the owner to edit. Nothing here is enforced yet.

This file is scaffolding for building the enforcement, not a document agents read. When the
rollout is done it is deleted (section 11, last step). From then on nothing tells an agent how
code must be shaped: a lint or a type rejects every other shape and its message names the one
way, and a skill carries every multi-step procedure. An agent learns a rule by breaking it
and reading the error, at the moment the rule applies, and never has to read a rule ahead of
time.

## 1. Axioms

1. **One way.** Each piece of data has exactly one function that reads it from outside the
   program and one that writes it out, and each concept has exactly one type. A second way to
   get or send the same data does not compile or does not pass the gate.
2. **The architecture is checked, not described.** Layering, data ownership and the I/O
   boundary are enforced by the compiler where it can (crates, privacy) and by the
   architecture linter (section 3.2) where it cannot. Nothing about the shape of the code is
   left to reading.
3. **No comments.** What a comment would say goes into a type, a name, a test, or the map.
   A comment is text the compiler does not check; when it drifts from the code, a reader
   trusts the wrong one.
4. **Types carry meaning.** Every value that means something has its own type, even when it
   is one integer. Data cannot be handed to a parameter that was not shaped for it.
5. **One description.** Each concept has one name in code, CLI, GUI and docs, and each kind of
   knowledge has one home (section 7).
6. **Nothing is done until the gate is green.** The rules are enforced after every agent
   turn and before every commit, and the agent cannot edit the rules (section 8).

## 2. Architecture

### 2.1 Crates

The layers are crates in one Cargo workspace. A crate can use only what its `Cargo.toml`
names, so the dependency direction is the compiler's, not a convention.

```
crates/
  domain/      the model: Map, Path, Step, Anchor, Span, Line, FileText, Symbol, Xref, the anchor
               rules, diff, patience, staleness. No I/O, no threads, no external crates.
  io-process/  the one place a process is started                      -> (std only)
  io-store/    the one atomic file writer and file removal             -> domain
  io-source/   the walk and source file reads                          -> domain, ignore
  io-map/      .codemap/*.cmap read and write                          -> domain, io-store
  io-cache/    .codemap-cache read and write                           -> domain, io-store
  io-vcs/      jj / git: the parent revision, a file at a revision     -> domain, io-process
  io-lsp/      JSON-RPC over stdio                                     -> domain, io-process, serde_json
  index/       symbols and xrefs: tree-sitter resolvers, server orchestration
                                                                        -> domain, io-lsp, io-source, io-cache, tree-sitter*
  features/    the feature registry, section 2.4                       -> (std only)
  cli/         the command surface: clap wire types in, text out       -> domain, features, index, io-map, io-vcs, clap, regex
  ui/          the element tree, geometry, input, the draw list        -> (std only)
  platform/    wgpu renderer, fontdue glyphs, winit window, scripts    -> ui, winit, wgpu, fontdue, png
  gui/         the app                                                  -> domain, features, index, cli, ui, platform, io-*
  codemap/     the binary: main                                         -> cli, gui
xtask/         gate, archlint, hook handlers                            -> tree-sitter, tree-sitter-rust, serde_json, ignore
```

`ui` and the renderer depend on each other in the legacy code (`ui` draws through `Gfx`, `gfx`
measures through `ui::Measure`); the split makes `ui` produce a draw list that `platform`
renders.

The allowed edges are also written in `xtask/src/arch.rs` as data; the gate compares every
`Cargo.toml` against it, so adding a dependency is a rulebook change, not a code change.

### 2.2 One entry point per data source

| Data | Read | Write | Owner crate |
|---|---|---|---|
| source files | `Source::walk(root) -> Files` | never | io-source |
| the map | `MapStore::load(root) -> Result<Map, MapLoadError>` | `MapStore::save(&Map) -> Result<Saved, MapSaveError>` | io-map |
| the index cache | `CacheStore::load(root) -> Cache` | `CacheStore::save(&Cache)` | io-cache |
| a revision | `Vcs::detect(root)`, `Vcs::file_at(Rev, RelPath)`, `Vcs::map_at(Rev)` | never | io-vcs |
| a language server | `LspSession::request<R: Request>(R) -> Response<R>` | same | io-lsp |
| a process | `Process::run(Program, Args)`, `Process::spawn(Program, Args)` | same | io-process |
| user input (CLI) | `CliInput::parse(args) -> Command` | `CliOutput` (one writer) | cli |
| user input (GUI) | winit events → `Action` | frames | gui |
| test scripts | `ScriptLine::parse` | `DumpLine` | gui |

Each owner crate exports only these functions and the wire-free domain types they return.
`xtask` records every crate's public items in `api/<crate>.api`, for every crate in the
workspace; the gate regenerates them and fails on any difference, so every change to a crate's
surface is a visible line in the diff. For `io-*` crates the `.api` files are rulebook (section
8): a second way to read or write a data source needs the owner.

Inside `domain`, the same holds for state: `Map` has private fields, and every change is a
method returning `Changed`. There is one `Map::add_step`, one `Map::set_note`, and nothing else
can reach a path's steps.

### 2.3 The I/O boundary

Every I/O surface has its own **wire types**, which mirror the external format, and converts
to and from domain types in exactly one place:

```
io-map/src/wire.rs      CmapFile, CmapPath, CmapStep       (the text format, section 8 of design.md)
io-map/src/convert.rs   impl TryFrom<CmapFile> for Map, impl From<&Map> for CmapFile
```

- Domain types never derive or implement anything for a format: no `clap::Parser`, no
  `serde`, no `Display` used as output. The `Display` that prints a line number 1-based lives
  on the CLI's wire type `LineArg`, not on `domain::Line`.
- Wire types never leave their crate. The linter checks that no `pub` item of an `io-*` or
  `cli` crate names a type from its `wire` module.
- A change to an internal type changes one `convert.rs`. A change to a format changes one
  `wire.rs` and one `convert.rs`, and a golden.

Surfaces: CLI arguments and output, `.cmap`, `.codemap-cache`, LSP JSON-RPC, jj/git output,
the GUI script and dump protocol.

### 2.4 The feature map

Every function the app offers a user is one variant of one enum, and everything else about it
is derived from that variant or checked against it.

```
crates/features/   -> (nothing)
  enum Feature { AddStep, PinStep, ShowPath, OpenPath, JumpToDefinition, DockPanel, ... }
  struct Spec { name: FeatureName, summary: Summary, surface: Surface, triggers: &'static [Trigger] }
  enum Surface { Cli, Gui, Both }
  enum Trigger { Command(CommandName), Key(Chord), Pointer(Gesture, IdName), Script(ScriptWord) }
  fn spec(Feature) -> Spec          exhaustive match: a new variant does not compile without its spec
```

| Question | Answered by | Enforced by |
|---|---|---|
| What can the user do? | the `Feature` enum | it is the only list |
| How do they do it? | `Spec::triggers`: the command, key chord, click on which element, script word | CLI parsing, GUI key dispatch and GUI buttons are built from the registry: `widgets::button` takes a `Feature`, the key handler is a lookup `Chord -> Feature`, clap subcommands take their `about` from `Spec::summary`. A trigger outside the registry has no code path. |
| What does it do? | `Spec::summary` (one line: help text, button tooltip) and the feature's flow path in the map (the full account) | gate: every `Feature` has a `flow` path `feature-<name>` in group `features/<surface>` |
| Where is it? | the one handler: `fn run(Feature, …)` dispatches each variant to one function; the feature path's root step is anchored on it | gate: the path's root step anchors the handler the dispatch names; `check` catches it going stale |
| Is it tested? | a golden line per feature | gate: every `Feature` appears in a CLI golden or a GUI scenario |

`cargo xtask features` prints the whole map, one row per feature: name, surface, triggers,
summary, handler `file:line`, feature path. In the GUI, the `features/` group of the Paths tab
is the same map, readable step by step.

A command, key binding or button that is not a `Feature` cannot be written, since every
trigger is built from the registry; a `Feature` without its path, handler or golden fails the
gate. Adding a function to the app is always the same edit: variant, spec, handler, path, golden
(the `add-feature` skill).

## 3. Enforcement layers

Cheapest first. A rule goes in the first layer that can hold it.

| Layer | Where | Holds |
|---|---|---|
| Types | the code | wrong shapes cannot be written |
| Crates, privacy | `Cargo.toml`, `pub(crate)`, private fields | dependency direction, one entry point |
| rustfmt | `rustfmt.toml` (default settings), format-on-edit hook | layout |
| rustc / clippy lints | `[workspace.lints]`, `deny` | forms clippy knows |
| clippy bans | `clippy.toml` `disallowed-methods/types/macros` | "only this crate may call that" |
| archlint | `xtask/src/lint/*.rs` over tree-sitter-rust syntax trees | everything above cannot express |
| gate | `cargo xtask gate`, run by the Stop hook | all of the above, plus `codemap check` and the API lock |
| hooks | `.claude/settings.json` → `cargo xtask hook <event>` | process: rulebook, map files, goldens |

### 3.1 Why a linter of our own

Clippy lints expressions and items but cannot know that `index` must not read `.cmap` files,
that a field must not be a bare `u32`, or that an identifier uses a word from outside the
vocabulary. Custom clippy lints (dylint) need a pinned nightly and a compiler-plugin API that
changes every release. tree-sitter-rust is already in the lock file: a syntax tree per file is
enough for every rule below, and a rule is a short function with good/bad fixtures as tests.

The linter never guesses name resolution. A rule that would need it ("only X calls Y") is
turned into a privacy or crate boundary instead, so the compiler resolves it.

### 3.2 archlint rules

Each rule has an id, a sentence that states the one way, and fixtures. Its failure message is
`file:line: <id>: <the one way>`.

| id | The one way | What the linter matches |
|---|---|---|
| L1 | No comments. | any `line_comment` or `block_comment` node, doc comments included |
| L2 | No primitive or `String`/`&str` in a signature. Parameters, return types, fields and enum payloads use a named type. | `primitive_type`, `String`, `str`, `bool`, tuple types in `function_item` parameters and return, `field_declaration`, enum variant payloads, including inside generics (`Vec<u32>` fails, `Vec<Line>` passes); exempt: the single field of a newtype, `wire` modules, `impl` blocks of the newtype itself, trait impls the std defines (`Display::fmt`, `From`), closure parameters, and everything under `tests/` or `#[cfg(test)]` |
| L3 | A newtype's field is private. | `pub` on the field of a one-field tuple struct |
| L4 | No indexing or slicing. A collection is read through its interface. | `index_expression`, range inside `[]`; exempt: the collection module of `domain` (L4 has its one site there) |
| L5 | No `bool` parameters; a two-way choice is an enum. | `bool` in a parameter list (subsumed by L2, listed for its message) |
| L6 | Absence is `Option`. | comparisons with `""`, `-1`, `usize::MAX`, `u32::MAX` |
| L7 | Identifiers are made of vocabulary words (section 6). | every identifier split on `_` and case; each part in `vocabulary.txt`; a part listed as a synonym fails with its canonical word |
| L8 | Wire types stay in their crate. | a `pub` item outside `wire.rs` whose signature names a `wire::` type |
| L9 | Domain has no I/O. | in `crates/domain`: `std::fs`, `std::process`, `std::thread`, `std::net`, `std::io::{stdin,stdout,stderr}`, `print!`-family macros |
| L10 | Suppressions are `#[expect(lint, reason = "...")]`, and each is listed in `xtask/src/arch.rs` `EXPECTS` by file and lint. | every `#[expect` and `#[allow` attribute not in the list |
| L11 | Tests are registered once: a scenario table generates its `#[test]`s. | `#[test]` fns in `tests/` outside the generating macro |
| L12 | GUI elements come from widget helpers; colours and sizes from the theme. | `ui.leaf(`/`ui.open(` outside `gui::widgets`; numeric literals in `.pad(`, `.gap(`, colour arrays, `dim(` outside `gui::theme` |
| L13 | Element ids come from the id registry. | `Id::named(` with a string literal outside `gui::ids` |
| L14 | Every `Feature` has a handler, a `features/` flow path rooted on that handler, a golden line, and (for CLI features) a design.md §10 row. | cross-check of the `Feature` enum, the dispatch match, `.codemap/`, the goldens, and `design.md` |
| L15 | Triggers come from the registry. | `Chord`, `CommandName`, `Gesture` constructed outside `crates/features`; `widgets::button` called without a `Feature` |

### 3.3 Clippy

`[workspace.lints]`, all `deny`:

- Rust: `unsafe_code` (except `gfx`, listed in `EXPECTS`), `unreachable_pub`, `missing_debug_implementations`.
- Clippy groups: `clippy::all`, `clippy::pedantic`.
- Clippy lints that pick one form: `allow_attributes`, `allow_attributes_without_reason`,
  `indexing_slicing`, `string_slice`, `unwrap_used`, `expect_used`, `panic`, `todo`,
  `unimplemented`, `dbg_macro`, `print_stdout`, `print_stderr`, `str_to_string`,
  `string_to_string`, `iter_over_hash_type`, `absolute_paths`, `wildcard_imports`,
  `redundant_pub_crate`, `clone_on_ref_ptr`, `get_unwrap`, `map_err_ignore`, `let_underscore_must_use`, `if_then_some_else_none`,
  `semicolon_if_nothing_returned`, `shadow_unrelated`, `min_ident_chars`,
  `mod_module_files` (one module layout).
- Pedantic lints left off: `must_use_candidate`, `module_name_repetitions`,
  `missing_errors_doc`, `missing_panics_doc`, `cast_precision_loss` (graph layout only, via
  `EXPECTS`). Anything that only asks for docs is off, since there are none.
- `wildcard_imports` is on everywhere except `gui`'s `use super::*`, which is the one glob,
  listed in `EXPECTS`.

`clippy.toml` bans, each with the crate that is the one allowed site:

| Banned | Only in | Reason in the message |
|---|---|---|
| `std::fs::*` writes | io-map, io-cache (through their one atomic write) | "write through MapStore::save or CacheStore::save" |
| `std::fs::*` reads | io-source, io-map, io-cache, io-vcs | "read through the owning io crate" |
| `std::process::Command::new` | io-process | "start a process with Process::run / Process::spawn" |
| `std::thread::spawn` | gui `runtime` module | "start work with Runtime::job / Runtime::service" |
| `std::collections::HashMap` iteration | — | "iterate a BTreeMap or a sorted Vec" |

## 4. Types

### 4.1 Newtypes

Every value with a meaning gets a type. A bare integer, string or bool is only allowed inside
the newtype that wraps it and inside wire types.

| Concept | Type | Inside | Replaces |
|---|---|---|---|
| a 0-based line in a file | `Line` | `u32` | `usize`, `i32`, `u32`, `i64` line values |
| an inclusive run of lines | `Span { start: Line, end: Line }` | | `(usize, usize)`, `line_start`/`line_end`, `lo`/`hi` |
| a line offset from a symbol's start | `LineOffset` | `i32` | `off_start`/`off_end` |
| a repo-relative path | `RelPath` | `String`, `/` only | 6 separator normalisations |
| a path's name | `PathName` | `String`, validated per design.md §8 | `String` |
| a group | `GroupName` | `String`, normalised | `String`, `""` for none |
| a step id | `StepId` | `[u8; 6]` base36 | `String` |
| a step's place in the step list | `StepOrder` | `u32` | `u32` |
| a note | `Note` | `String` | `String`, `""` for none |
| a symbol's name | `SymbolName` | `String` | `String`, `""` for absolute anchors |
| a text hash | `TextHash` | `u64` | `u64` |
| a file in the index | `FileId` | `u32` | `usize` |
| a symbol in the index | `SymbolId { file: FileId, symbol: SymbolIndex }` | | `SymRef`, `(String, String)`, `file * 100_000 + sym` |
| a CLI step address | `StepIndex` (cli wire) | `u32` | `i64`, `-1` for root |
| a GUI element | `Id` | `u64` | `u64` alias |
| window pixels / graph cells | `Px`, `Cells` | `i32`, `f32` | mixed `f32`/`i32` |
| a revision | `Rev` | `String` | `&str` |
| a language | `Language` enum | | extension strings |

A newtype has a private field, a constructor that validates (or `From` when every value is
valid), and only the operations its meaning allows: `Line + LineOffset -> Line`,
`Line - Line -> LineCount`, never `Line + Line`.

### 4.2 Collections instead of indexing

A collection that is addressed by id is a typed collection:

```
IdVec<FileId, File>        get(FileId) -> Option<&File>, iter() -> (FileId, &File), push(File) -> FileId
File                       line(Line) -> Option<&str>, lines(Span) -> impl Iterator<Item = (Line, &str)>
Path                       steps() -> impl Iterator<Item = &Step>, step(StepId) -> Option<&Step>
```

Ids are created only by `push`, so an id cannot come from arithmetic. The one `[]` in the
program is inside `IdVec` (L4, `indexing_slicing` expect listed).

### 4.3 Shapes

- `Anchor` is the pinned slice (file, symbol, offsets, hash); `Step` holds an `Anchor` plus
  its id, order, parent, author, note and link. This matches design.md §4 word for word.
- Errors are one enum per crate (`MapLoadError`, `CliError`, …), each variant naming the
  thing that was missing. The message is written once, in the CLI's wire layer.
- A function that can fail says why: `Result`, never an `Option` that drops the reason.
- No tuples in signatures; a pair that means something is a struct.

## 5. No comments

- No `//`, `/* */`, `///` or `//!` anywhere in `crates/`, `xtask/` or `tests/` (L1).
- CLI help text is data: `#[command(about = "...")]` on the wire types.
- What a comment would have said goes, in this order of preference:
  1. into a **type** or **name** (`// offsets are relative to the symbol` becomes `LineOffset`);
  2. into a **test** (`// never leaves a half-written file` becomes a test that proves it);
  3. into the **map**: a step note on the path that covers the code (the "why" of a step), or
     the note of a `layer`/`type` path for facts true of the code wherever it is read;
  4. deleted, if it described code that is not there.
- design.md §4 currently says facts true of code regardless of path are "a code comment, not
  a note". That sentence changes to "a `layer` or `type` path note".
- `#[expect(..., reason = "...")]` keeps its reason: it is checked by the compiler for
  presence and by L10 for being listed.

## 6. One description

### 6.1 Vocabulary

`vocabulary.txt` at the root lists every word an identifier may use, and every banned
synonym with its canonical word:

```
step
anchor
span
line
symbol
sym -> symbol
pi -> path
idx -> index
...
```

L7 splits every identifier (`add_step`, `StepOrder`) into words and checks each. A new word
may be added by the agent; the gate prints added words in its summary for the owner. A synonym
line is rulebook. design.md §4's table is the glossary; every vocabulary word that names a
concept appears there.

### 6.2 One home per kind of knowledge

| Knowledge | Home | Not in |
|---|---|---|
| what the tool is and why | design.md | code, CLAUDE.md |
| what a user can do, how, and where it is handled | the `Feature` registry, printed by `cargo xtask features`; each feature's flow path for the full account | design.md prose lists, help text written by hand |
| how code must be shaped | the checks themselves: each lint's message states the one way | any prose: code comments, CLAUDE.md, rule files, this file once the rollout ends |
| what a workflow, layer or type does | the map (`.codemap/`) | code comments |
| what an agent does, step by step, for a kind of change | `.claude/skills/` | CLAUDE.md |
| the session contract (read map, gate, repin) | the `finish` skill and the Stop hook; CLAUDE.md shrinks to the build command and "use codemap to explore" | |
| open problems | TODO.md | code (`ponytail:`, `TODO`) |
| what the program outputs | goldens | prose |

## 7. Tests

| id | The one way |
|---|---|
| T1 | A scenario table generates its `#[test]`s (L11). |
| T2 | One `edit_file`, failing when the needle is absent. |
| T3 | One transcript format for CLI scenarios and GUI `after` commands. |
| T4 | A scenario needing a missing tool is skipped with a printed reason, never compared. |
| T5 | Each run has its own scratch directory under `CARGO_TARGET_TMPDIR`; parity keeps its fixed path. |
| T6 | Every error variant of `CliError` appears in a golden (L14 extends to it). |
| T7 | GUI scripts aim at element ids or a code line/column, never raw pixels. |
| T8 | Every archlint rule has a passing and a failing fixture. |
| T9 | Every wire type round-trips: `wire -> domain -> wire` is identity on every golden input. |

## 8. The gate and the rulebook

### 8.1 Rulebook

These files are written only by the owner:

`vocabulary.txt` synonym lines, `xtask/**`, `clippy.toml`, `rustfmt.toml`, `.gitattributes`,
`[workspace.lints]` and `[dependencies]` in every `Cargo.toml`, `api/io-*.api`,
`.claude/settings.json`, `.claude/skills/**`, and `conventions.md` while it exists.

Enforced three ways:

1. PreToolUse on Edit/Write denies the change and says to propose it in the reply instead.
2. PreToolUse on Bash denies commands that name a rulebook path with a writing verb
   (`>`, `sed -i`, `mv`, `rm`, `cp`, `tee`) — a best effort, backed by 3.
3. The gate hashes the rulebook against the last commit and fails if any of it changed in the
   working copy. The owner's own edits are committed before the next agent session.

### 8.2 `cargo xtask gate`

In order, stopping at the first failure:

1. `cargo fmt --check`
2. archlint over the workspace
3. `Cargo.toml` edges against `arch.rs`; `.api` files against the code
4. rulebook unchanged
5. `cargo clippy --workspace --all-targets` (lints are `deny`)
6. `cargo test --workspace --lib` and the archlint fixtures
7. `codemap . check`

`cargo xtask gate --full` adds the CLI goldens; the GUI goldens stay out of the gate (they
need a display) and are run by the `finish` skill.

### 8.3 Hooks

All hooks are `cargo xtask hook <event>`, so they are Rust and run on every platform the
workspace builds on; there are no shell scripts to port.

| Event | Does |
|---|---|
| PreToolUse Edit/Write | rulebook (8.1); `.codemap/**` only through `codemap` unless the file has a conflict marker; `tests/golden/**` only through `CODEMAP_BLESS` |
| PreToolUse Bash | rulebook writes; `git commit`, `jj commit`, `jj describe`, `jj new`, `jj squash` refused while the last gate is red |
| PostToolUse Edit/Write | `rustfmt` on the edited `.rs` file; archlint on that file, its failures returned to the agent at once |
| Stop, SubagentStop | the gate, if any source, map or rulebook file changed since the last green run. On failure the agent is blocked with the failures. After three identical failures in a row it may stop, and the hook leaves the gate red: commits stay refused (above), and the next session starts with the failures (SessionStart) |
| SessionStart | if the gate is red, its last failures, so the session starts by fixing them |

There is no path through the agent's tools to a commit that fails the gate, and no path to
changing the rules.

## 9. Skills

Rules and procedures are kept apart. A rule (what shape code must have) lives only in a
check. A skill (what to do, in what order, for a kind of change) holds steps and commands and
never restates a rule; where a step has rules, the skill says which command to run to hear
them (`cargo xtask lint <file>`), not what they are. So a rule changes in one place, the check,
and no skill goes stale.

Skills (`.claude/skills/`), each a numbered recipe ending in `finish`:

| Skill | For |
|---|---|
| `finish` | the gate; `stale` → `repin` → reread notes → `path-pin`; new symbols into paths; `check`; commit |
| `add-feature` | any new user-facing function: `Feature` variant and `Spec` (summary, triggers), handler, `features/` flow path rooted on the handler, goldens for success and each error; then the CLI or GUI steps below |
| `add-cli-command` | wire type, `Command` variant mapped to its `Feature`, domain method, `convert`, `CliError` variants, design.md §10 row |
| `add-data-source` | a new I/O surface: owner crate, wire, convert, one entry point, `.api` (needs the owner) |
| `add-domain-type` | newtype checklist (4.1): private field, validating constructor, allowed operations, vocabulary word, glossary row |
| `add-gui-element` | widget, theme, id registry, `Action` mapped to its `Feature`, `Dump`, a script scenario, screenshot check |
| `add-test-scenario` | T1–T9 |
| `refactor` | base build, parity run, what a diff means |
| `tripped` | turn lost time into a proposed check for the owner: the one way, the lint that rejects the other ways, its good and bad fixtures, as a patch to `xtask` the owner applies |

There are no path-scoped rule files (`.claude/rules/`). A rule in this proposal without a
check yet (error messages written once, section 4.3; "a refactor proves parity, a behaviour
change reblesses") is either redesigned until a check can hold it (an error enum whose
`Display` in the CLI wire layer is the only message site; the gate refusing a golden change in
a commit whose parity run passed) or dropped. Until its check exists, it sits in this file,
which agents are not pointed at.

## 10. Bugs found by the audit

Fixed in rollout step 3, each with a test:

1. `gui/work.rs:217` `poll_base` uses `try_recv().ok()`, not `landed()`: if the vcs thread
   panics, `base_rx` stays `Some`, the frame redraws every 50 ms forever and the script
   command `idle` never returns.
2. The flat step index shows in the GUI (`gui/mod.rs:302`, `gui/views.rs:128`), against
   design.md §6.
3. Id collisions: `gui/panels.rs:236` (`file * 100_000 + sym`), `gui/widgets.rs:256` (fields
   keyed on hint text).
4. `cache.rs:109` writes `imports` in hash order, so the same index writes different bytes.

## 11. Rollout

One commit each. Every refactor step runs parity against the commit before it. Map steps go
stale on most steps; each commit ends with `repin` and `path-pin` so `check` passes.

1. **xtask and hooks.** `cargo xtask gate` with only the rules the code already passes, the
   hooks, the rulebook guard, skills, `.gitattributes`. From here every session is gated.
2. **rustfmt**, default settings.
3. **Bugs** (section 10).
4. **Comments out.** Every comment triaged into type/name, test, map note, or deletion
   (section 5), file by file, with the map notes written in the same commit. L1 on.
5. **Workspace split** (2.1), moving code without changing it. Crate edges and L9 on.
6. **I/O boundary** (2.2, 2.3): wire and convert per surface, one entry point each. L8 and the
   API lock on.
7. **Feature map** (2.4): the `features` crate, one `Feature` per CLI command, GUI action
   and script command in use today; triggers routed through the registry; a `features/` flow
   path per feature, rooted on its handler. L14, L15 on.
8. **Types** (4.1–4.3): `Line`/`Span` first, then ids, then the rest. L2–L6 and
   `indexing_slicing` on.
9. **Vocabulary**: `vocabulary.txt` from the current identifiers, synonyms collapsed,
   renames. L7 on.
10. **GUI**: widgets, theme, ids, actions, dump. L12, L13 on.
11. **Tests**: T1–T9, L11 on.
12. **Scaffolding out.** Every rule in this file has a check, or has been dropped. This file is
    deleted; CLAUDE.md is cut to the build command and the pointer to codemap for exploring;
    design.md keeps what the tool is and loses anything about code shape.

Each step ends with its rules switched on, so a later session cannot bring the old way back.

## 12. Decisions

Resolved:

- Indexing is banned; collections are read through typed interfaces (4.2).
- Every I/O surface has wire types separate from domain types (2.3).
- One description: vocabulary and one home per kind of knowledge (6).
- CLI line arguments parse into the CLI's wire type `LineArg` (1-based, `u32`); `convert.rs`
  turns it into `domain::Line`.
- `Step` holds an `Anchor` (4.3).
- Lints as listed in 3.3.
- Windows-specific hook concerns are moot: hooks are `cargo xtask`.
- L2 applies inside generics; tests and closure parameters may use primitives.
- Agents may add vocabulary words; synonym lines are owner-only.
- The API lock covers every crate; for `io-*` crates it is also rulebook.
- `gui`'s `use super::*` is the one allowed glob import.
- Every user-facing function is a `Feature` in one registry, with its triggers, summary,
  handler, flow path and golden checked against it (2.4).
