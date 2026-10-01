# CLI read map

CLI read map lets an agent list paths, print one path as a document, and search notes without opening the GUI.

## Sub-features

- `cli-paths` lists every path (or one) as a tree of steps.
- `cli-path` prints a path's note and step code.
- `cli-path-expand` prints linked paths inline with `--expand`.
- `cli-notes` searches path and step notes by regex.

## How to get to it (user POV)

- Run `codemap <root> paths`.
- Run `codemap <root> path <name>`.
- Run `codemap <root> path <name> --expand`.
- Run `codemap <root> notes <regex>`.

## Driving it with control-codemap

Preconditions:

- `control-codemap doctor` reports `bin_ok=yes` and `map_ok=yes`. Compositor not required.
- Artifacts: `.cursor/skills/verify-codemap/artifacts/cli-read-map/`.

- **List.** Run `control-codemap cli -- paths`. Exit `0`; stdout contains multiple path names and kinds such as `[flow]` / `[layer]` / `[type]`.
- **Read.** Pick a name from that list. Run `control-codemap cli -- path <name>`. Exit `0`; stdout includes the path note and numbered steps with `file:line` ranges.
- **Expand.** Run `control-codemap cli -- path <name> --expand` for a path that shows `→` links in `paths`. Exit `0`; stdout includes linked path content inline.
- **Notes search.** Run `control-codemap cli -- notes 'the'`. Exit `0`; stdout lists matching path/step note hits (or is empty only if the map truly has no match — try a token from a known path note).
- **Proof.** Save full transcripts under `artifacts/cli-read-map/` (`paths.txt`, `path.txt`, `path-expand.txt`, `notes.txt`) each starting with a line `# exit=<code>`.

## Gotchas

- No compositor: do not call `launch` for this feature alone.
- `path` without a name fails (exit `2`); always pass the name.
- GUI Output panel runs the same commands; proving Output is a separate entry point (`feature` RunCommand) and is not covered by this file's CLI-only drive.
