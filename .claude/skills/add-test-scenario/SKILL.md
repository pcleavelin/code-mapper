---
name: add-test-scenario
description: Add or extend a CLI or GUI scenario in tests/. Use when a feature has no scenario that triggers it, or when a change's effect should show in parity's comparison.
---

# Add a test scenario

A scenario drives the real binary. It fails when codemap crashes or a GUI script reports an
error; its output is compared only by `cargo xtask parity`, build against build. So a scenario
line earns its place by making an effect visible to parity (a command's output, a dump after a
gesture), and the behaviour a change adds is stated by a test written before the code.

1. Prefer extending an existing scenario of the same area; a new one costs a fixture setup.
2. CLI: a function `fn <name>(bin: &Path, name: &str) -> Result<String, Missing>` in
   `tests/common/cli.rs` (copy the shape of `read`), then add `<name>` to `cli_scenarios!`.
   Every outcome of a command is a line (success and each error), followed by a read that
   shows the effect.
3. GUI: a function `pub fn <name>() -> Scenario` in `tests/common/gui.rs`, then add `<name>`
   to `gui_scenarios!`. The script aims at element ids (`click-id`, `hover-id`, `steps/<n>`
   to bring a step into view, `<<DUMP ...|dx,dy>>` inside an element, resolved against the
   frame the line runs in); the window is a fixed 1600x1000 at scale 1. `pause <ms>` waits for
   wall-time polls. A gesture at an element out of view is refused and fails the scenario; to
   check that something is not shown, use `absent <id>`. A `dump` after every gesture whose
   effect matters, and the effect must show in a `DUMP` line: if it does not, extend the
   component's `Dump` (`add-gui-element` step 6).
4. A scenario needing jj or git starts with `needs("jj")?`; it is skipped where the tool is
   missing.
5. Run it: `cargo test --release --test cli <name>`. For a GUI scenario, run
   `.cursor/skills/verify-codemap/bin/control-codemap parity <name>`. That passes
   `XDG_RUNTIME_DIR` to one headless weston, then runs `cargo xtask parity`. The gui test
   binary and the parity test each run one window at a time. A second window on a desktop
   display stops getting frames and the script stalls. Then read
   each `old.txt`/`new.txt` it lists against how `understand` says the behaviour is meant to
   work.
6. `finish`.
