# Open a tour

Open a tour lets a user pick a mapped tour from the Tours list, read it as a document of steps and notes, and confirm the same tour from the CLI.

## Sub-features

- `open-tour-list` opens a tour by clicking its row in the Tours list.
- `open-tour-step` selects a step in the opened document.
- `open-tour-cli-list` lists tours from the terminal.
- `open-tour-cli-read` prints one tour's note and step code from the terminal.

## How to get to it (user POV)

- Click a tour row in the Tours panel (element id `tours/<n>`).
- Run `codemap <root> tours` to list the map.
- Run `codemap <root> tour <name>` to print one tour.

## Driving it with control-codemap

Preconditions:

- `control-codemap doctor` reports `bin_ok=yes`, `map_ok=yes`, `compositor_owned=yes`.
- The root's map has at least one tour (repo `.codemap` does).
- Artifacts directory: `.cursor/skills/verify-codemap/artifacts/open-tour/`.

- **Settle.** Wait for indexing. Script lines: `idle` then `wait 1`. Backend DUMP later shows `reindexing=false`.
- **Open first tour.** Click the first tour row. Script: `click-id tours/0` then `wait 3` then `dump`. Stderr contains `DUMP tab=Tour` and `DUMP tab=… tour=Some(`.
- **Select a step.** Click a step header. Script: `click-id step/0` then `wait 2` then `dump`. Stderr `DUMP tab=` shows `step=Some(`.
- **Screenshot.** Capture the document. Script: `shot <artifacts>/open-tour/document.png`. PNG shows the Tours list and Tour document.
- **CLI list.** List tours. Run `control-codemap cli -- tours`. Exit code `0`; stdout lists tour names with kinds.
- **CLI read.** Print one named tour from the list output (for example `anchor` or `cli`). Run `control-codemap cli -- tour <name>`. Exit code `0`; stdout includes the tour note and at least one step with a file:line range.
- **Proof.** Save CLI transcripts to `artifacts/open-tour/tours.txt` and `artifacts/open-tour/tour.txt`, keep the DUMP excerpt in `artifacts/open-tour/dump.txt`, and keep `document.png`. `meta.txt` records entry points `tours/0` and `cli tour`.

## Gotchas

- Tour row indices are the filtered visible list, not stable tour ids. Prefer confirming the opened tour via DUMP/`tour` name rather than assuming index `0` is a particular name.
- `idle` before the first click; a cold cache can leave the Tours list empty for a few seconds.
- Do not `click-id save` unless you intend to persist GUI edits; read-only open does not need it.
- CLI `tour` needs the exact tour name as stored in the map (`tours` output).
