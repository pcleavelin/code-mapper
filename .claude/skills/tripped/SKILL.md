---
name: tripped
description: Use when you lost time in this repo - guessed which of two ways was current, redid work, hit a rule that did not fit the case, or were corrected by the owner. Turns the friction into a proposed check for the owner instead of a workaround.
---

# Turn friction into a check

The rules live only in checks (`xtask/src/lint/checks.rs`, clippy's lint table in
`Cargo.toml`, `clippy.toml`, `xtask/src/arch.rs`), and those are the owner's. You propose;
the owner applies.

1. Say what happened in two sentences: what you did, what you expected, what it cost.
2. Classify it:
   - two ways to do one thing exist in the code: which one is right, and every place the other
     one is used (`grep`, `cargo xtask lint`);
   - a rule rejected something it should allow: the exact code and the finding;
   - a rule allowed something that bit you: the code and what went wrong;
   - a step of a skill is missing or wrong.
3. Propose the check: the one-way sentence its message will state, where it lives (a lint
   rule in `xtask/src/lint/checks.rs`, a clippy lint, a type that makes the wrong shape
   unwritable), and a passing and a failing fixture in the form of
   `xtask/src/lint/tests.rs`. Write it as a patch in your reply; do not apply it.
4. Go on with the task the one way the proposal names, without weakening or bypassing any
   existing check.
