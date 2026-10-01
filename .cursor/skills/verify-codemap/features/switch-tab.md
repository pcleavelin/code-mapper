# Switch tab

Switch tab lets a user show a different view in a panel by clicking its tab, without changing which path or file is selected.

## Sub-features

- `switch-tab-path` shows the Path document tab.
- `switch-tab-graph` shows the Graph tab.
- `switch-tab-symbols` shows the Symbols tab.
- `switch-tab-files` shows the Files tab.

## How to get to it (user POV)

- Click a tab in a panel header: Path, Graph, Symbols, Files, Diff, Listing, Results, Xrefs, or Output.

## Driving it with control-codemap

Preconditions:

- Compositor owned; map present; a path is open (`click-id paths/0` then `wait 3` first).
- Artifacts: `.cursor/skills/verify-codemap/artifacts/switch-tab/`.

- **Graph tab.** Script: `click-id tab@Graph` then `wait 2` then `dump` then `shot <artifacts>/switch-tab/graph.png`. DUMP shows Graph selected in `DUMP panels` (Graph marked with `*`).
- **Symbols tab.** Script: `click-id tab@Symbols` then `wait 2` then `dump` then `shot <artifacts>/switch-tab/symbols.png`. Panels DUMP marks Symbols with `*`.
- **Files tab.** Script: `click-id tab@Files` then `wait 2` then `dump` then `shot <artifacts>/switch-tab/files.png`. Panels DUMP marks Files with `*`.
- **Back to Path.** Script: `click-id tab@Path` then `wait 2` then `dump`. Panels DUMP marks Path with `*`.
- **Proof.** Keep the four DUMP excerpts and PNGs; `meta.txt` lists tab ids used.

## Gotchas

- Tab ids are `tab@` plus the view name with that exact casing (`tab@Graph`, not `tab@graph`).
- Opening Graph before any path is open may show an empty canvas; open a path first.
- `DUMP panels` is the reliable proof of which tab is active; the PNG alone can be ambiguous.
