---
name: understand
description: Restate a feature request or bug report in your own words, then learn why it is needed and how it is meant to work, before any design or edit. Use first for every feature request and every bug fix.
---

# Understand the request

Nothing is designed or edited until this is done. A misreading caught here costs one
message; caught after the change, it costs the change.

1. Restate. Before reading any code, write in the reply, in your own words rather than the
   owner's:
   - a feature: who uses it (CLAUDE.md, opening: the agent through the CLI, the human
     through the GUI), what they do, what they see afterwards, and what stays as it is;
   - a bug: what happens, what should happen instead, and the steps that get there.
   Name every point where the request can be read two ways.
2. Why it is needed. Find the goal it serves and what it costs the user to go without it:
   - CLAUDE.md, opening (roles, goals, non-goals) and "Why it is built this way";
   - `TODO.md`: the open issues agent sessions and the owner logged because they cost them
     time, what is planned, and each deferred item with the trigger that pulls it in;
   - `codemap . notes <regex>` and `codemap . path feature-<name>` for the area;
   - the `jj log` descriptions of the commits that built the area.
   A request that serves no goal, or runs into a non-goal, goes to the owner before anything
   else.
3. How it is meant to work. The spec is, in this order: the owner's words, CLAUDE.md, the
   feature's summary in `crates/features/src/lib.rs`, its `features/` path, the scenarios in
   `crates/codemap/tests/common` that drive it. Then see the current behaviour yourself: run the CLI
   command, or a GUI script with `dump` and `shot` (CLAUDE.md, "Testing the GUI"), and read
   the PNG. A bug is reproduced here, with the exact command or script that shows it; one
   that does not reproduce goes back to the owner with what you ran.
4. Restate again with what steps 2 and 3 taught: the need in one sentence, the behaviour as
   the user will meet it, and each place the first restatement was wrong. If a question is
   left that only the owner can answer (which of two readings, a conflict with CLAUDE.md,
   a reported bug that is the spec's own behaviour), ask it and wait for the answer.
5. The need from step 4 has one home: for a feature, the note of its `feature-<name>` path
   (`add-feature` step 6); for a bug, the fix's commit message.
6. Go on with `prototype` for a new feature, `fix-bug` for a bug, or the skill for the kind
   of change.
