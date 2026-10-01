# Archlint

Archlint is the repo's architecture linter. It rejects code that is not the one way a concept is written. Agents prove shape with it before claiming a structural change is done.

## Sub-features

- `archlint-workspace` runs every rule over the workspace.
- `archlint-file` runs the same rules on one or more edited files.
- `archlint-map-check` runs `codemap check` for stale map steps.
- `archlint-doctor` confirms xtask is available before linting.

## How to get to it (user POV)

- Run `cargo xtask lint` (or `cargo xtask lint <file>...`) from the repo root.
- Run `cargo xtask gate` which includes archlint among other checks.
- After a Claude Code edit hook, findings are printed on the edited `.rs` file automatically.
- Through this skill: `control-codemap lint` and `control-codemap check-map`.

## Driving it with control-codemap

Preconditions:

- `control-codemap doctor` reports `xtask_ok=yes` and `bin_ok=yes`.
- Compositor not required.
- Artifacts: `.cursor/skills/verify-codemap/artifacts/archlint/`.

- **Doctor.** Run `control-codemap doctor`. Output includes `xtask_ok=yes`.
- **Workspace lint.** Run `control-codemap lint`. Exit `0`, stdout contains `lint_ok=yes` and `archlint: clean`, report file `artifacts/archlint/lint.txt` exists.
- **File lint.** After editing a Rust file, run `control-codemap lint crates/cli/src/exec.rs` (path as edited). Exit `0` or fix each finding by writing the one way in the message.
- **Map check.** Run `control-codemap check-map`. Exit `0` and `map_check_ok=yes` (or fix stale steps with `tour-pin` / `repin`).
- **Proof.** Keep `lint.txt` and `check.txt` under `artifacts/archlint/` with `meta.txt` naming the entry point (`workspace` or file paths).

## Gotchas

- Findings name the fix. Do not add `#[allow]` or delete a rule to clear them. Suppressions are `#[expect(..., reason = "...")]` listed in `xtask/src/arch.rs` EXPECTS only.
- Test zones (`tests/`, `tests.rs`) run a smaller rule set. A pass in a test file does not prove the same code in strict code.
- `cargo xtask gate` is slower and includes clippy and tests. Prefer `lint` while iterating; run gate before finish/commit.
- New rules need a failing and a passing fixture in `xtask/src/lint/tests.rs`. Do not document a shape rule only in this skill.
