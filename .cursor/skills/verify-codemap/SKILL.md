---
name: verify-codemap
description: Drive the codemap GUI (scripted headless window) and CLI to prove user-facing behavior. Use when verifying a change against the real app, capturing screenshots/DUMP transcripts, or checking a mapped feature end to end.
---

# Verify codemap

codemap is a native desktop app (winit + wgpu) plus a CLI. Humans browse the map in the GUI; agents use the CLI. This skill drives both through a private headless compositor — never the developer's interactive window.

Read `features/README.md`, then the feature file for the behavior you are proving. Proof that hits one convenient entry point is incomplete when the map lists others.

## Launch

Binary (once per machine / after code changes):

```bash
rustup default stable   # needs Rust 1.85+ (edition 2024)
cargo build --release -p codemap
```

Start a private compositor for this verification run (required before any GUI drive):

```bash
export PATH="$PWD/.cursor/skills/verify-codemap/bin:$PATH"
export CODEMAP_VERIFY_RUN="vfy-$$"
control-codemap launch
```

Ready when stdout contains `ready=yes` and `control-codemap doctor` reports `compositor=up` and `compositor_owned=yes`.

Default root is the repo (`CODEMAP_VERIFY_ROOT`). Scripted/shot GUI runs open a fixed 1600×1000 window at scale 1, ignore the real mouse and keyboard, and quit themselves — there is no long-lived GUI server. CLI commands are short-lived processes.

Teardown: `control-codemap cleanup` (stops only the weston this run started; keeps artifacts).

## Doctor

```bash
control-codemap doctor
```

Exit 0 means this instance is worth driving. Check `bin_ok`, `weston_ok`, `vulkan_icd_ok`, and — before GUI — `compositor_owned=yes`. If a Wayland socket exists but this run did not start it (`compositor_owned=no`), refuse to drive: another session owns that compositor.

## Drive

Helper: `.cursor/skills/verify-codemap/bin/control-codemap` (put it on `PATH` as above).

**GUI** — write a script file (forward-slash paths; one command per line), then:

```bash
control-codemap gui --script /path/to/script.txt --shot "$ART/after.png"
```

Script commands (real input into the fixed window): `wait n`, `pause ms`, `mouse x y`, `down`, `up`, `click x y`, `dblclick x y`, `drag …`, `wheel dy`, `pinch n`, `key <name> [ctrl] [alt]`, `text …`, `quit`; app commands `tab <name>`, `open <file> [line]`, `scroll <panel> <n>`, `idle` (wait until indexing/merging settles), `rect <id>`, `click-id <id>`, `dblclick-id <id>`, `hover-id <id>`, `absent <id>`, `shot <file.png>`, `dump`.

Stable element ids (from `crates/gui/src/ids.rs` and `CLAUDE.md`): tour rows `tours/<n>`, group rows `group/<n>`, outline `steps/<n>`, document steps `step/<n>`, tabs `tab@Tour` `tab@Graph` `tab@Symbols` `tab@Files` `tab@Diff` `tab@Source` `tab@Search` `tab@References` `tab@Console`, fields `field@search` `field@tours` `field@symbols` `field@goto-line` `field@new-tour`, graph button `doc-graph`, save `save`, panels `panel/<n>`. After anything that changes layout, `wait 1` before `rect` / `click-id` (rects come from the previous frame). Grep stderr for `^DUMP`.

Prefer `click-id` over coordinates. Prefer `idle` before the first click when the root may still be indexing.

**CLI** — same binary, text mode:

```bash
control-codemap cli -- tours
control-codemap cli -- tour anchor
control-codemap cli -- notes 'anchor'
```

Exit codes: `0` ok, `1` save/window failure, `2` refused (usage, bad map, command Failure).

## Evidence

Named proof directory (survives cleanup):

`.cursor/skills/verify-codemap/artifacts/<feature-id>/`

Override with `CODEMAP_VERIFY_ARTIFACTS`. For each proof:

1. Exercise the real user path (GUI click-id / CLI command), not internal test hooks.
2. Capture the action and the resulting state: stderr `DUMP` lines (tab, tour, panels, tours listed, backend) plus a `shot` PNG; for CLI, stdout + stderr + exit code in a `.txt` transcript.
3. Verify side effects when the feature mutates the map: re-read with `cli -- tours` / `cli -- tour <name>`, or reopen in the GUI. Read-only features need DUMP/screenshot of the opened view.
4. Record the feature id and entry point in the artifact filenames or a sibling `meta.txt`.

Proof standards: mocks only at production boundaries (language servers already isolate themselves). Do not claim visual behavior from reading code — drive it and look (Read the PNG).

## Cleanup

```bash
control-codemap cleanup
```

Kills only the weston PID this run wrote under `$CODEMAP_VERIFY_DIR/state/weston.pid`. Removes `$CODEMAP_VERIFY_DIR/state`. Never deletes `artifacts/`. Never `pkill weston` by name.

## Isolate

- One private Wayland socket per `CODEMAP_VERIFY_RUN`. Do not share sockets across agents.
- GUI scripts load/save layout only via `CODEMAP_LAYOUT` (default: the run's `state/layout.bin`), so they do not touch the developer's layout.
- Mutating map edits: set `CODEMAP_VERIFY_ROOT` to a disposable copy of a fixture (or a throwaway clone), never the developer's live `.codemap` unless the feature under test is read-only and the script does not save edits.
- On Linux, never run two GUI windows on one compositor for concurrent scripts — one scripted window at a time under this run's weston.
- Required host packages for GUI: `weston`, `mesa-vulkan-drivers` (lavapipe ICD at `/usr/share/vulkan/icd.d/lvp_icd.json`). Set `VK_DRIVER_FILES` to that ICD.

## Helpers

| Invocation | Purpose |
|---|---|
| `control-codemap doctor` | Health check |
| `control-codemap launch` | Start owned headless weston |
| `control-codemap gui --script F [--shot P]` | Run scripted GUI / single shot |
| `control-codemap cli -- <args>` | Run CLI against the verify root |
| `control-codemap cleanup` | Tear down compositor + scratch state |

Script path: `.cursor/skills/verify-codemap/bin/control-codemap`.
