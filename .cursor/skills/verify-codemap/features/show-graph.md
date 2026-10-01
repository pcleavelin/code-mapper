# Show graph

Show graph lets a user draw the path being read as a call graph from the Path document toolbar.

## Sub-features

- `show-graph-button` opens the graph from the document `doc-graph` control.
- `show-graph-tab` reaches the same Graph view via `tab@Graph`.

## How to get to it (user POV)

- With a path open in the Path view, click the graph control on the document toolbar (`doc-graph`).
- Or click the Graph tab (`tab@Graph`).

## Driving it with control-codemap

Preconditions:

- Compositor owned; map present.
- Artifacts: `.cursor/skills/verify-codemap/artifacts/show-graph/`.

- **Open a path.** Script: `idle` / `click-id paths/0` / `wait 3`.
- **Toolbar entry.** Script: `click-id doc-graph` then `wait 3` then `dump` then `shot <artifacts>/show-graph/from-toolbar.png`. Panels DUMP marks Graph; `DUMP graph` appears with zoom/pan fields.
- **Tab entry (separate run or after returning to Path).** Script: `click-id tab@Path` / `wait 2` / `click-id tab@Graph` / `wait 3` / `dump` / `shot <artifacts>/show-graph/from-tab.png`. Same Graph-active proof.
- **Proof.** PNGs show the graph canvas; DUMP includes `DUMP graph` and Graph marked active.

## Gotchas

- `doc-graph` is only meaningful with a path open in the document panel.
- Graph layout may still be settling; wait at least 3 frames after opening before `shot`.
- Do not treat an empty graph on a path with no call edges as failure — assert the Graph view is active, then whether nodes appear for that path.
