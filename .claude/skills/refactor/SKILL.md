---
name: refactor
description: Change code without changing behaviour - move, rename, restructure, retype. Proves parity against the build before the change, transcripts, GUI dumps and screenshots byte for byte.
---

# Refactor with proof

1. Before touching code, build the base: `cargo build --release -p codemap` and copy
   `target/release/codemap` somewhere outside the repo (e.g. the system temp directory).
   Note the current revision (`jj log -r @- --no-graph -T commit_id` or `git rev-parse HEAD`).
2. Make the change. Keep goldens untouched: a golden diff means behaviour changed.
3. `cargo build --release -p codemap`, then
   `CODEMAP_BASE_BIN=<base binary> CODEMAP_PARITY_REV=<revision> cargo test --release --test parity -- --ignored`.
   `CODEMAP_PARITY_ONLY=<name>` narrows it to one scenario. Differences are written under the
   scratch directory it prints.
4. A difference is a behaviour change: undo the part that caused it, or, if the change is
   wanted, it is not a refactor: split it into its own commit and rebless there.
5. `finish`.
