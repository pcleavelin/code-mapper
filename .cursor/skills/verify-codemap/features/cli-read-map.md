# CLI read map

CLI read map lets an agent list tours, print one tour as a document, and search notes without opening the GUI.

## Sub-features

- `cli-tours` lists every tour (or one) as a tree of steps.
- `cli-tour` prints a tour's note and step code.
- `cli-tour-inline` prints linked tours inline with `--inline`.
- `cli-notes` searches tour and step notes by regex.

## How to get to it (user POV)

- Run `codemap <root> tours`.
- Run `codemap <root> tour <name>`.
- Run `codemap <root> tour <name> --inline`.
- Run `codemap <root> notes <regex>`.

## Driving it with control-codemap

Preconditions:

- `control-codemap doctor` reports `bin_ok=yes` and `map_ok=yes`. Compositor not required.
- Artifacts: `.cursor/skills/verify-codemap/artifacts/cli-read-map/`.

- **List.** Run `control-codemap cli -- tours`. Exit `0`; stdout contains multiple tour names and kinds such as `[flow]` / `[layer]` / `[data]`.
- **Read.** Pick a name from that list. Run `control-codemap cli -- tour <name>`. Exit `0`; stdout includes the tour note and numbered steps with `file:line` ranges.
- **Inline.** Run `control-codemap cli -- tour <name> --inline` for a tour that shows `→` links in `tours`. Exit `0`; stdout includes linked tour content inline.
- **Notes search.** Run `control-codemap cli -- notes 'the'`. Exit `0`; stdout lists matching tour/step note hits (or is empty only if the map truly has no match — try a token from a known tour note).
- **Proof.** Save full transcripts under `artifacts/cli-read-map/` (`tours.txt`, `tour.txt`, `tour-inline.txt`, `notes.txt`) each starting with a line `# exit=<code>`.

## Gotchas

- No compositor: do not call `launch` for this feature alone.
- `tour` without a name fails (exit `2`); always pass the name.
- GUI Console runs the same commands; proving Console is a separate entry point (feature RunCommand) and is not covered by this file's CLI-only drive.
