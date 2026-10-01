# Codemap verification map

This directory is the maintained source for verifying user-facing behavior of codemap. Read the index before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- Build `target/release/codemap` (`cargo build --release -p codemap`, Rust 1.85+).
- Put `.cursor/skills/verify-codemap/bin` on `PATH`.
- Set `CODEMAP_VERIFY_RUN` to a unique id (for example `vfy-$$`).
- Run `control-codemap launch`, then `control-codemap doctor` and require `bin_ok=yes`, `xtask_ok=yes`, `weston_ok=yes`, `vulkan_icd_ok=yes`, `compositor=up`, `compositor_owned=yes`, and `map_ok=yes` when the feature needs a map. Shape features use `control-codemap lint` and do not need the compositor.
- Default root is the repo. For map mutations, point `CODEMAP_VERIFY_ROOT` at a disposable tree instead.
- Never drive a Wayland socket this run did not start. Never drive the developer's interactive GUI.

## Driving conventions

- Start every recipe from a freshly launched compositor unless the feature says otherwise.
- Prefer element ids (`click-id tours/0`, `click-id tab@Graph`) over coordinates.
- Treat every command as literal. Keep quoted tour names and flags unchanged.
- Run GUI scripts through `control-codemap gui --script …`.
- Run CLI through `control-codemap cli -- <command>`.
- After layout-changing clicks, `wait 1` (or more) before the next `click-id` / `dump` / `shot`.
- Call `idle` once near the start when the root may still be indexing.
- Cleanup with `control-codemap cleanup`. Do not remove proof artifacts.

## Proof and skip reporting

- Capture the user action and the resulting state, not only the final screen.
- GUI proof includes stderr `DUMP` lines (`DUMP tab=`, `DUMP tours`, `DUMP panels`, `DUMP backend`) and a PNG from `shot`.
- CLI proof includes the command, stdout, stderr, and exit code.
- Mutation proof includes a second read of the map (`cli -- tour <name>` or reopen in the GUI).
- Record the feature ID and entry point with every artifact (filename prefix or `meta.txt`).
- Report an unreachable path with the attempted command and the unmet precondition.
- Do not report a skipped entry point as verified through a different path.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features` lists short IDs with one line for each behavior.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with control-codemap` starts with `Preconditions:` and uses labeled bullets that pair each user action with an exact command and observable result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation details out of the map. Name only user paths, stable handles, required state, commands, and observable proof.

## Features

- [Archlint](./archlint.md) covers `cargo xtask lint`, file-scoped lint, and map `check`.
- [Open a tour](./open-tour.md) covers Tours-list open, step selection, and CLI `tour` / `tours`.
- [Switch tab](./switch-tab.md) covers switching Tour, Graph, Symbols, and Files views by tab id.
- [Show graph](./show-graph.md) covers opening the call graph for the tour being read.
- [Browse files](./browse-files.md) covers the Files tab directory and file open.
- [CLI read map](./cli-read-map.md) covers agent-facing `tours`, `tour`, and `notes` without a window.
