---
name: fix-bug
description: Fix behaviour that differs from what the owner, CLAUDE.md, or a feature's summary says it should be. Use for every bug report, including the items in TODO.md.
---

# Fix a bug

1. `understand`: the restatement, the spec the behaviour breaks, and the command or script
   that reproduces it.
2. Pin the failure before touching the code: a unit test in the crate at fault that fails
   on the current code, or, for behaviour only a scenario reaches, a scenario step
   (`add-test-scenario`) that shows it, whose corrected output `cargo xtask parity` lists
   after the fix.
3. Find the cause, not the symptom: `codemap . path` for the feature's flow, then `callers`
   and `callees` from where the output goes wrong. Fix it there. A fix that also changes
   what another feature does is a behaviour change the owner hears about first.
4. If a lint, a type or a skill step should have kept this bug from being written, run
   `tripped`.
5. If the bug is an item in `TODO.md`, remove the item.
6. `finish`. The commit message states what was wrong, what the spec says, and the cause.
