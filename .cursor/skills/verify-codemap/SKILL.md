---
name: verify-codemap
description: Drive the codemap GUI (scripted headless window) and CLI to prove user-facing behavior, and run archlint. Use when verifying a change against the real app, capturing screenshots/DUMP transcripts, checking mapped features, or proving code shape against xtask archlint.
---

# Verify codemap

codemap is a native desktop app (winit + wgpu) plus a CLI. Humans browse the map in the GUI; agents use the CLI. Shape of the code is enforced by archlint (`cargo xtask lint`), not by prose. This skill drives the app through a private headless compositor and runs that linter. Never drive the developer's interactive window.

Read `features/README.md`, then the feature file for the behavior you are proving. Proof that hits one convenient entry point is incomplete when the map lists others.

## Launch

Binary (once per machine / after code changes):

```bash
rustup default stable   # needs Rust 1.85+ (edition 2024)
cargo build --release -p codemap
```

One command starts the compositor and proves the window. Do not export `XDG_RUNTIME_DIR` yourself. The helper passes it to weston. Weston exits with `fatal: environment variable XDG_RUNTIME_DIR is not set` when that variable is missing from the weston process. Exporting it after `start_compositor` is too late.

```bash
export PATH="$PWD/.cursor/skills/verify-codemap/bin:$PATH"
export CODEMAP_VERIFY_RUN="vfy-$$"
check-compositor
control-codemap smoke
```

`check-compositor` exits 0 only when launch works with `XDG_RUNTIME_DIR` unset in the parent, two GUI scripts in one run both open the default layout, and the GUI stderr has no `ZINK` line. `smoke` prints `smoke=ok` and a `DUMP` line.

Parity, including GUI scenarios, is the same compositor:

```bash
control-codemap parity
control-codemap parity gui-graph
```

`cargo xtask parity` on Linux exits immediately when the Wayland socket is missing and names `control-codemap parity`. `cargo xtask parity cli` does not open a window and does not need weston.

Ready when `smoke` prints `smoke=ok`, or when stdout of `launch` contains `ready=yes` and `control-codemap doctor` reports `compositor=up` and `compositor_owned=yes`.

Default root is the repo (`CODEMAP_VERIFY_ROOT`). Scripted/shot GUI runs open a fixed 1600×1000 window at scale 1, ignore the real mouse and keyboard, and quit themselves. CLI and lint commands are short-lived processes.

Teardown: `control-codemap cleanup` (stops only the weston this run started; keeps artifacts).

## Doctor

```bash
control-codemap doctor
```

Exit 0 means the instance is worth driving. Require `bin_ok=yes`, `xtask_ok=yes`, and for GUI `compositor_owned=yes`. `compositor_owned=no` means refuse to drive.

Doctor does not run archlint. Shape proof is `control-codemap lint`.

## Architecture (archlint)

The custom linter is `cargo xtask lint` (tour `archlint`). Each finding is `file:line: L#: <one way> (<match>)`. The sentence is the fix. Rules live in `xtask/src/lint.rs` and `xtask/src/lint/checks.rs`. The crate graph is `xtask/src/arch.rs` DEPENDENCIES. Clippy + `clippy.toml` cover unwrap, print, process/thread/fs one-ways the AST linter does not.

```bash
control-codemap lint              # whole workspace; report in artifacts/archlint/lint.txt
control-codemap lint crates/gui/src/app.rs
control-codemap check-map         # stale steps; artifacts/archlint/check.txt
```

Exit 0 and `lint_ok=yes` / `archlint: clean` before claiming a code-shape change is done. A red lint is fixed by writing the one way the finding names, not by weakening the rule. New shape rules go in archlint fixtures (`xtask/src/lint/tests.rs`), not in this skill's prose.

Gate (format, archlint, crate graph, API lock, clippy, tests, map check): `cargo xtask gate`. Use when finishing a change. GUI scenarios need the compositor setup from Launch.

## Drive

Helper: `.cursor/skills/verify-codemap/bin/control-codemap`.

**GUI** — write a script file (forward-slash paths; one command per line), then:

```bash
control-codemap gui --script /path/to/script.txt --shot "$ART/after.png"
```

Script commands: `wait n`, `pause ms`, `mouse x y`, `down`, `up`, `click x y`, `dblclick x y`, `drag …`, `wheel dy`, `pinch n`, `key <name> [ctrl] [alt]`, `text …`, `quit`; app commands `tab <name>`, `open <file> [line]`, `scroll <panel> <n>`, `idle`, `rect <id>`, `click-id <id>`, `dblclick-id <id>`, `hover-id <id>`, `absent <id>`, `shot <file.png>`, `dump`.

Stable ids (`crates/gui/src/ids.rs`, `CLAUDE.md`): `tours/<n>`, `group/<n>`, `steps/<n>`, `step/<n>`, `tab@Tour` `tab@Graph` `tab@Symbols` `tab@Files` `tab@Diff` `tab@Source` `tab@Search` `tab@References` `tab@Console`, `field@search` `field@tours` `field@symbols` `field@goto-line` `field@new-tour`, `doc-graph`, `save`, `panel/<n>`. After layout changes, `wait 1` before the next `click-id`. Grep stderr for `^DUMP`.

**CLI**

```bash
control-codemap cli -- tours
control-codemap cli -- tour anchor
control-codemap cli -- notes 'anchor'
```

Exit codes: `0` ok, `1` save/window failure, `2` refused.

## Evidence

`.cursor/skills/verify-codemap/artifacts/<feature-id>/` (override with `CODEMAP_VERIFY_ARTIFACTS`). Survives cleanup.

1. Exercise the real user path (GUI click-id / CLI / `lint`), not internal test hooks.
2. GUI: DUMP lines plus a `shot` PNG. CLI: stdout, stderr, exit code. Lint: `artifacts/archlint/lint.txt` with `lint_ok=yes`.
3. Mutations: second read via `cli -- tour <name>` or reopen in the GUI.
4. Record feature id and entry point in filenames or `meta.txt`.

Do not claim visual behavior from reading code. Drive it and Read the PNG.

## Cleanup

```bash
control-codemap cleanup
```

Kills only this run's weston PID file. Removes `$CODEMAP_VERIFY_DIR/state`. Never deletes `artifacts/`. Never `pkill weston` by name.

## Isolate

- One private Wayland socket per `CODEMAP_VERIFY_RUN`.
- `CODEMAP_LAYOUT` is unset unless you set it. A scripted run then opens the default panels and does not save a layout. A default file under the run state is loaded and saved, so the next script in that run id inherits the previous panels.
- Map mutations: disposable `CODEMAP_VERIFY_ROOT`, not the live `.codemap`, unless the recipe is read-only.
- The gui test binary and `cargo xtask parity` each run one window at a time. On a desktop display, a hidden window stops getting frames and the script stalls. Use this helper's weston, not the desktop `DISPLAY`.
- Host packages: `weston`, `mesa-vulkan-drivers` (lavapipe at `/usr/share/vulkan/icd.d/lvp_icd.json`). The helper sets `LIBGL_ALWAYS_SOFTWARE=1` so Mesa does not probe ZINK when the machine has no DRM device.

## Helpers

| Invocation | Purpose |
|---|---|
| `control-codemap doctor` | Health check |
| `check-compositor` | Launch order, default layout, no ZINK line |
| `control-codemap launch` | Start owned headless weston |
| `control-codemap smoke` | Launch and one dump script |
| `control-codemap parity [scenario]` | Launch and `cargo xtask parity` |
| `control-codemap gui --script F [--shot P]` | Scripted GUI / shot |
| `control-codemap cli -- <args>` | CLI against the verify root |
| `control-codemap lint [file...]` | archlint |
| `control-codemap check-map` | `codemap check` |
| `control-codemap cleanup` | Tear down compositor + scratch state |

Script path: `.cursor/skills/verify-codemap/bin/control-codemap`.
