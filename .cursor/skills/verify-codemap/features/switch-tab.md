# Switch tab

Switch tab lets a user show a different view in a panel by clicking its tab, without changing which tour or file is selected.

## Sub-features

- `switch-tab-tour` shows the Tour document tab.
- `switch-tab-graph` shows the Graph tab.
- `switch-tab-symbols` shows the Symbols tab.
- `switch-tab-files` shows the Files tab.

## How to get to it (user POV)

- Click a tab in a panel header: Tour, Graph, Symbols, Files, Diff, Source, Search, References, or Console.

## Driving it with control-codemap

Preconditions:

- Compositor owned; map present; a tour is open (`click-id tours/0` then `wait 3` first).
- Artifacts: `.cursor/skills/verify-codemap/artifacts/switch-tab/`.

- **Graph tab.** Script: `click-id tab@Graph` then `wait 2` then `dump` then `shot <artifacts>/switch-tab/graph.png`. DUMP shows Graph selected in `DUMP panels` (Graph marked with `*`).
- **Symbols tab.** Script: `click-id tab@Symbols` then `wait 2` then `dump` then `shot <artifacts>/switch-tab/symbols.png`. Panels DUMP marks Symbols with `*`.
- **Files tab.** Script: `click-id tab@Files` then `wait 2` then `dump` then `shot <artifacts>/switch-tab/files.png`. Panels DUMP marks Files with `*`.
- **Back to Tour.** Script: `click-id tab@Tour` then `wait 2` then `dump`. Panels DUMP marks Tour with `*`.
- **Proof.** Keep the four DUMP excerpts and PNGs; `meta.txt` lists tab ids used.

## Gotchas

- Tab ids are `tab@` plus the view name with that exact casing (`tab@Graph`, not `tab@graph`; `tab@Tour`, not `tab@Path`).
- Opening Graph before any tour is open may show an empty canvas; open a tour first.
- `DUMP panels` is the reliable proof of which tab is active; the PNG alone can be ambiguous.
