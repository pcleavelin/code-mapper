---
name: tripped
description: Use when you lost time in this repo - guessed which of two ways was current, redid work, hit a rule that did not fit the case, or were corrected by the owner. Turns the friction into a check, so the next session cannot trip the same way.
---

# Turn friction into a check

The rules live only in checks: the lint rules in `xtask/src/lint/checks.rs`, the architecture
tables in `xtask/src/arch.rs`, the skills in `.claude/skills/`, and clippy's configuration
(`clippy.toml`, the lint table in `Cargo.toml`). You apply all of them yourself.

1. Say what happened in two sentences: what you did, what you expected, what it cost.
2. Classify it:
   - two ways to do one thing exist in the code: which one is right, and every place the other
     one is used (`grep`, `cargo xtask lint`);
   - a rule rejected something it should allow: the exact code and the finding;
   - a rule allowed something that bit you: the code and what went wrong;
   - a step of a skill is missing or wrong.
3. Make the check: the one-way sentence its message states, where it lives (a lint rule in
   `xtask/src/lint/checks.rs`, a type that makes the wrong shape unwritable, a skill step), and
   a passing and a failing fixture in `xtask/src/lint/tests.rs`. Apply it, convert every place
   it finds, and keep `cargo xtask lint` output otherwise unchanged. A lint only for a rule
   that is mechanical and unambiguous; a judgement such as naming or wording is a review step
   in a skill instead (`finish` step 2), because a judgement lint forces renames that cost
   more than it catches.
4. Say in your reply what you added and why: the rule's code, its one-way sentence,
   and the places it converted. Never weaken or bypass an existing check to get past the
   friction; a rule that is wrong is changed openly, with the reason, like any other check.
5. Go on with the task the one way the check names.
