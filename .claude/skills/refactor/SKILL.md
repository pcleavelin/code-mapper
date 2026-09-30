---
name: refactor
description: Change code without changing behaviour - move, rename, restructure, retype. Proves parity against the build before the change, transcripts, GUI dumps and screenshots byte for byte.
---

# Refactor with proof

1. Start from a committed parent: the parent revision (`@-` in jj, `HEAD` in git) is the
   behaviour the refactor must keep.
2. Make the change.
3. `cargo xtask parity`. It builds the parent revision (cached under `target/parity`), plays
   every scenario on both builds, and must list none. `cargo xtask parity <name>` narrows it
   to one scenario while working.
4. A listed scenario is a behaviour change: read its `old.txt`/`new.txt`, then undo the part
   that caused it, or, if the change is wanted, it is not a refactor: split it into its own
   commit.
5. `finish`.
