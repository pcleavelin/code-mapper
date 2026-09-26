---
name: add-test-scenario
description: Add or extend a CLI or GUI golden scenario in tests/. Use when a change needs its output pinned, or when a feature has no scenario that triggers it.
---

# Add a test scenario

1. Prefer extending an existing scenario of the same area; a new one costs a fixture setup.
2. CLI: a function `fn <name>(bin: &Path, name: &str) -> Result<String, Missing>` in
   `tests/common/cli.rs` (copy the shape of `read`), then add `<name>` to `cli_scenarios!`.
   The golden is `tests/golden/cli-<name>.txt`.
3. GUI: a function `pub fn <name>() -> Scenario` in `tests/common/gui.rs`, then add `<name>`
   to `gui_scenarios!`. The script aims at element ids (`click-id`, `hover-id`, `outline/<n>`
   to bring a step into view, `<<DUMP ...|dx,dy>>` inside an element); the window is a fixed
   1600x1000 at scale 1. `pause <ms>` waits for wall-time polls.
4. A scenario needing jj or git starts with `needs("jj")?`; it is skipped where the tool is
   missing.
5. Bless: `CODEMAP_BLESS=1 cargo test --release --test cli <name>` (GUI: `--test gui`; on
   Linux run GUI tests under a headless compositor, one at a time). Read the new golden in
   full; it is the specification of the behaviour.
6. `finish`.
