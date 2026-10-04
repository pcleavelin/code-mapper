---
name: prototype
description: Build several working designs of a new feature side by side, run each, keep the best and scrap the rest. Use for every new feature, after `understand` and before the steps of `add-feature`.
---

# Prototype, then choose

A design is judged by running it, not by reading it. Every new feature gets at least two
prototypes that differ in shape, and exactly one survives.

1. Sketch the designs: at least two, usually three. They differ in how the user meets the
   feature (which surface, which trigger, what they see) or in where it lives in the code
   (which crate, which type owns it), not in naming or detail. Give each a name and two
   lines in the reply: what the user does and sees, and what code it adds or changes, plus
   one line on how it removes the cost the case from `understand` names. A design that
   breaks CLAUDE.md (a non-goal, a reason in "Why it is built this way", an axiom), or that
   shows something without removing that cost, is dropped here, with the reason.
2. One jj workspace per design, outside the repo (the session's scratch directory or the
   system temp directory), based on the current change:
   `jj workspace add <dir>/proto-<name> --name proto-<name> -r @`.
   Workspaces keep the prototypes apart without `jj new`, which the commit guard refuses
   while the gate is red. The change's own workspace stays untouched until step 6.
3. Build each prototype far enough to run the feature end to end in the real binary: the
   registry entry, the handler, the element. The map, scenarios, `api/` and the naming review
   are skipped, and the gate is not run on a prototype. The designs are independent: spawn
   one subagent per prototype in the same turn, each given the second restatement from
   `understand`, its design's sketch and its workspace directory, and asked to return the
   diff stat, what it had to change beyond the sketch, and the demonstration from step 4.
4. Demonstrate each with the prototype's own binary
   (`<dir>/proto-<name>/target/release/codemap`): the CLI command and its output, or a GUI
   script with `shot` and `dump`, and read the PNG. A prototype that cannot show the
   restated behaviour is scrapped here. If every one is scrapped, go back to step 1 with
   what the failures taught, or to the owner if they say the request itself cannot work.
5. Choose among the survivors, comparing in this order: the run removes the cost the case
   names; the behaviour matches the restatement; the user's steps (fewer, and triggered the
   way existing features are); fit with CLAUDE.md and the axioms (no second way, no new
   concept where one exists); the size of the code and the number of crates it touches.
   State the winner in the reply, and for every other design the reason it lost; the
   winner's reason completes the case.
6. Keep the winner, scrap the rest. In the change's own workspace (CLAUDE.md),
   `jj restore --from proto-<winner>@` brings the winner's files into the current change. Then `jj abandon` every prototype's change
   (`proto-<name>@`), `jj workspace forget` every prototype workspace, and delete their
   directories. Nothing from a scrapped design is carried over by hand.
7. Go on with `add-feature` step 3 on the restored code: every step the prototype skipped is
   now due.
