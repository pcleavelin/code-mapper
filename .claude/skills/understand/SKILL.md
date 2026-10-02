---
name: understand
description: Restate a feature request or bug report in your own words, lock the goal, and mark each follow-up as a separate backlog or the same deliverable, before any design or edit. Use first for every feature request and every bug fix.
---

# Understand the request

Nothing is designed or edited until this is done. A misreading caught here costs one
message; caught after the change, it costs the change.

1. Restate. Before reading any code, write in the reply, in your own words rather than the
   owner's:
   - a feature: who uses it (CLAUDE.md, opening: the agent through the CLI, the human
     through the GUI), what they do, what they see afterwards, and what stays as it is;
   - a bug: what happens, what should happen instead, and the steps that get there.
   Name every point where the request can be read two ways. Lock the goal in the reply
   before any design, as these lines:
   - Goal. One sentence in the owner's words, then one sentence for what the user can do
     when the work is done.
   - Reading. When two readings would build different things, name both. When the owner is
     in the thread, ask which one and wait. Otherwise pick one, and name the one word that
     reverses the pick.
   - Follow-ups. Mark each later ask `separate backlog` or `same deliverable`. The default
     is `separate backlog`. Use `same deliverable` only when the owner says that ask drives
     the design. Leave a backlog item as a list for later, out of the primary deliverable's
     shape, copy, and placement.
   Do not start `prototype` until those lines are in the reply.
2. Why it is needed. Find the goal it serves and what it costs the user to go without it:
   - CLAUDE.md, opening (roles, goals, non-goals) and "Why it is built this way";
   - `TODO.md`: the open issues agent sessions and the owner logged because they cost them
     time, what is planned, and each deferred item with the trigger that pulls it in;
   - `codemap . notes <regex>` and `codemap . tour feature-<name>` for the area;
   - the `jj log` descriptions of the commits that built the area.
   A request that serves no goal, or runs into a non-goal, goes to the owner before anything
   else. For a feature, write its case in the reply (CLAUDE.md, "Every feature makes its
   case"): the user and the moment, the goal, the cost today shown by a run of the tool in
   step 3, and why nothing codemap has answers it. A case that rests on "it would be nice" or
   "it looks good" is not finished; the owner hears what is missing. When the request is
   help, an out-of-box flow, or onboarding, add:
   - Moment. The cold user, and what they can do once the guide has worked.
   - Walk. Each primary gesture that moment needs, and where the surface shows it.
     Fill this line from the run in step 3.
   Finish the case only when the walk shows every gesture on the surface. The chord that
   opens the palette is one of those gestures. A Help tab of registry rows failed this
   walk by leaving that chord out. The walk stayed failed when a separate audit of making
   tours by hand was folded into that tab and treated as the guide. The gate's result stays
   out of the walk. A panel's existence stays out of it.
3. How it is meant to work. The spec is, in this order: the owner's words, CLAUDE.md, the
   feature's summary in `crates/features/src/lib.rs`, its `features/` tour, the scenarios in
   `crates/codemap/tests/common` that drive it. Then see the current behaviour yourself: run the CLI
   command, or a GUI script with `dump` and `shot` (CLAUDE.md, "Testing the GUI"), and read
   the PNG. A bug is reproduced here, with the exact command or script that shows it; one
   that does not reproduce goes back to the owner with what you ran.
4. Restate again with what steps 2 and 3 taught: the need in one sentence, the behaviour as
   the user will meet it, and each place the first restatement was wrong. Repeat the Goal
   line, the Follow-ups marks, and the Walk line when the request has one. When one of
   them changed, say what the first reading got wrong.
   Take a conflict with CLAUDE.md, or a reported bug that is the spec's own behaviour, to
   the owner before anything is built. Keep a two-way reading of the goal on the Reading
   line from step 1.
5. The need from step 4 has one home: for a feature, its case is the note of its
   `feature-<name>` tour (`add-feature` step 6); for a bug, the fix's commit message.
6. Go on with `prototype` for a new feature, `fix-bug` for a bug, or the skill for the kind
   of change. Carry the Goal, Reading, Follow-ups, and Walk lines into that skill. A design
   that drops them starts again at step 1.
