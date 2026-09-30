---
name: add-cli-command
description: Add or change a codemap CLI command (the agent's interface, also run from the GUI's output panel). Use after the feature exists in the registry (add-feature).
---

# Add a CLI command

1. The feature: `add-feature` step 3 (name = command name, summary = help line).
2. `crates/cli/src/wire.rs`: the clap variant with its arguments (1-based lines and list
   positions are wire types here), the exhaustive `Command -> Feature` match, and every
   message the command prints, success and each error. Messages exist only in this file.
3. `crates/cli/src/convert.rs`: wire arguments to domain values (lines to `domain::Line`,
   list positions to `StepId`).
4. The command's arm in `exec`: read and change the model only through `domain` methods; if
   one is missing, add it to `domain` with a unit test.
5. `design.md` section 10: the command's line in the command table.
6. `tests/golden`: add the command, every error it can give, and a follow-up read that shows
   the effect to the fitting scenario in `tests/common/cli.rs`, then
   `CODEMAP_BLESS=1 cargo test --release --test cli` and read the diff.
7. `finish`.
