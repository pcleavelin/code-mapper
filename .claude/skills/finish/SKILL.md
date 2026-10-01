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
   already names the same concept (`codemap . refs`, `api/*.api`, the Concepts table in
   CLAUDE.md) and report only real inconsistencies: one concept under two names, one name for
   two concepts, an abbreviation where the code spells the word out, a name that contradicts
   the glossary. It suggests renames with the reason for each and edits nothing. Apply the
   ones you agree with before the map step (a rename that reaches code outside the diff is a
   `refactor`), and say in your reply which you declined and why. Skip this step when the diff
   introduces no names.
3. Tests. Spawn one Sonnet subagent, in the same turn as the naming one, with the change and
   this brief: for each test the diff adds or changes (a unit test, a scenario step, an
   assertion), name one plausible mistake in the code under test that makes it fail. A test with
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
4. The case. When the diff adds a `Feature` variant, spawn one Sonnet subagent, in the same
   turn as the two above, with the feature's tour note and CLAUDE.md's "Every feature makes
   its case", and this brief: judge the note as the owner would before agreeing to build the
   feature. Report each part of the case that is missing, asserted without evidence, or
   already answered by something codemap has, and say whether the case convinces. It edits
   nothing. Rewrite the note until it does; a feature whose case cannot be made goes to the
   owner instead of into a commit.
5. The map. Build the binary once (`cargo build --release -p codemap`) and use
   `target/release/codemap .` below.
   1. `stale` lists every step whose text changed. `repin` follows each one from the parent
      revision (after a rebase: `repin <pre-rebase commit>` from `jj evolog` / `git reflog`).
   2. For every step `repin` printed with removed and added lines, reread its note against
      them and fix what no longer holds with `note-edit <tour> <index> <old> <new>`.
   3. Pin what is left by hand: `tour-pin <tour> <index> <file> <start> <end>`.
   4. Every new non-trivial symbol goes into a tour: `tour-add` to an existing tour (always
      give `under`; read the "does not call" note it prints), or `tour-new <name> <kind>
      <note> --group <group>` with the group its neighbours use (`groups`). For a flow,
      scaffold with `promote <sym> [depth]`, then trim and annotate. Code several tours call
      gets its own tour; link to it with `step-link`.
   5. A new user-facing function has its `features/` tour (see `add-feature`).
   6. `check` must print nothing.
   7. After a merge, a conflict in the map sits in `.codemap/<tour>.cmap` like any other; resolve
      it in the file (the edit guard allows a file with conflict markers); `check` refuses to
      read the map until no conflict is left.
   8. `uncovered <crate dir>` for the crates you touched lists only what you judge trivial;
      the owner audits that judgement.
6. `cargo xtask api` if a library crate's public items changed, and include the `api/` diff.
7. `cargo xtask parity`. A change meant not to alter output must list no scenario (the
   `refactor` skill). A change meant to alter output lists only the scenarios it meant to
   alter: read each `old.txt`/`new.txt` against the `understand` restatement, and treat any
   other listed scenario as a side effect to fix or to explain in the commit message.
8. `cargo xtask gate` once more, then commit (`jj commit -m ...` or `git commit`). The
   message says what changed and why, for someone who did not see the session.
