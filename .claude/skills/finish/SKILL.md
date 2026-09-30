---
name: finish
description: Finish any change to this repo before stopping or committing. Runs the gate, repairs the map (stale steps, new symbols, notes), and commits. Use at the end of every task that edited code, tests or the map, and whenever the Stop hook reports the gate red.
---

# Finish a change

The Stop hook runs the gate and blocks until it is green; commits are refused while it is red.
A stop while background tasks are still running, and a subagent's stop, are not gated: the
session's first stop with nothing in flight is. This is the order that gets it green in one pass.

1. `cargo xtask gate`. It stops at the first failing step and prints what failed. Fix that,
   run it again. Each lint message states the rule and the one way to satisfy it; do not look
   for a way around a message, do what it says. If a rule itself looks wrong for the case,
   stop and use the `tripped` skill instead of working around it.
2. Naming. Spawn one Sonnet subagent with the change (`jj diff --git`) and this brief: for each
   name the diff introduces (types, variants, functions, fields, modules), find how the code
   already names the same concept (`codemap . refs`, `api/*.api`, the glossary in design.md
   section 4) and report only real inconsistencies: one concept under two names, one name for
   two concepts, an abbreviation where the code spells the word out, a name that contradicts
   the glossary. It suggests renames with the reason for each and edits nothing. Apply the
   ones you agree with before the map step (a rename that reaches code outside the diff is a
   `refactor`), and say in your reply which you declined and why. Skip this step when the diff
   introduces no names.
3. Tests. Spawn one Sonnet subagent, in the same turn as the naming one, with the change and
   this brief: for each test the diff adds or changes (a unit test, a scenario step, a golden
   line), name one plausible mistake in the code under test that makes it fail. A test with
   no such mistake is tautological:
   - its expected value comes from the code under test, or from a copy of its logic;
   - it asserts a literal, a constant, or what a type already guarantees (a constructor's
     value read back through its accessor, a list's length restated as a number);
   - it checks a stub or fixture the test built, not what the code did with it;
   - it runs the code and asserts nothing about the result.
   It reports each tautological test with the reason and edits nothing. Rewrite each one to
   pin behaviour the code could get wrong, or delete it; a tautological test is never kept.
   For each test that survives the review, break the line it covers (flip the condition,
   return the default), watch it fail, and restore the line. Skip this step when the diff
   touches no tests.
4. The map. Build the binary once (`cargo build --release -p codemap`) and use
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
   7. After a merge, a conflict in the map sits in `.codemap/<path>.cmap` like any other; resolve
      it in the file (the edit guard allows a file with conflict markers); `check` refuses to
      read the map until no conflict is left.
   8. `uncovered <crate dir>` for the crates you touched lists only what you judge trivial;
      the owner audits that judgement.
5. `cargo xtask api` if a library crate's public items changed, and include the `api/` diff.
6. If the change was meant to alter output, rebless: `CODEMAP_BLESS=1 cargo test --release
   --test cli` (and `--test gui` for GUI output), and read the golden diff before committing
   it. If it was meant not to, run the `refactor` skill's parity check instead.
7. `cargo xtask gate` once more, then commit (`jj commit -m ...` or `git commit`). The
   message says what changed and why, for someone who did not see the session.
