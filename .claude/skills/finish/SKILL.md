---
name: finish
description: Finish any change to this repo before stopping or committing. Runs the gate, repairs the map (stale steps, new symbols, notes), and commits. Use at the end of every task that edited code, tests or the map, and whenever the Stop hook reports the gate red.
---

# Finish a change

The Stop hook runs the gate and blocks until it is green; commits are refused while it is red.
This is the order that gets it green in one pass.

1. `cargo xtask gate`. It stops at the first failing step and prints what failed. Fix that,
   run it again. Each lint message states the rule and the one way to satisfy it; do not look
   for a way around a message, do what it says. If a rule itself looks wrong for the case,
   stop and use the `tripped` skill instead of working around it.
2. The map. Build the binary once (`cargo build --release -p codemap`) and use
   `target/release/codemap .` below.
   1. `stale` lists every step whose text changed. `repin` follows each one from the parent
      revision (after a rebase: `repin <pre-rebase commit>` from `jj evolog` / `git reflog`).
   2. For every step `repin` printed with removed and added lines, reread its note against
      them and fix what no longer holds with `note-edit <path> <index> <old> <new>`.
   3. Pin what is left by hand: `path-pin <path> <index> <file> <start> <end>`.
   4. Every new non-trivial symbol goes into a path: `path-add` to an existing path (always
      give `under`; read the "does not call" note it prints), or `path-new <name> <kind>
      <note> --group <group>` with the group its neighbours use (`groups`). For a flow,
      scaffold with `promote <sym> [depth]`, then trim and annotate. Code several paths call
      gets its own path; link to it with `step-link`.
   5. A new user-facing function has its `features/` path (see `add-feature`).
   6. `check` must print nothing.
3. `cargo xtask api` if a library crate's public items changed, and include the `api/` diff.
4. If the change was meant to alter output, rebless: `CODEMAP_BLESS=1 cargo test --release
   --test cli` (and `--test gui` for GUI output), and read the golden diff before committing
   it. If it was meant not to, run the `refactor` skill's parity check instead.
5. `cargo xtask gate` once more, then commit (`jj commit -m ...` or `git commit`). The
   message says what changed and why, for someone who did not see the session.
