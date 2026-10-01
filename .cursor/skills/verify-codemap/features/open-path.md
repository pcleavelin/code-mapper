# Open a path

Open a path lets a user pick a mapped path from the Paths list, read it as a document of steps and notes, and confirm the same path from the CLI.

## Sub-features

- `open-path-list` opens a path by clicking its row in the Paths list.
- `open-path-step` selects a step in the opened document.
- `open-path-cli-list` lists paths from the terminal.
- `open-path-cli-read` prints one path's note and step code from the terminal.

## How to get to it (user POV)

- Click a path row in the Paths panel (element id `paths/<n>`).
- Run `codemap <root> paths` to list the map.
- Run `codemap <root> path <name>` to print one path.

## Driving it with control-codemap

Preconditions:

- `control-codemap doctor` reports `bin_ok=yes`, `map_ok=yes`, `compositor_owned=yes`.
- The root's map has at least one path (repo `.codemap` does).
- Artifacts directory: `.cursor/skills/verify-codemap/artifacts/open-path/`.

- **Settle.** Wait for indexing. Script lines: `idle` then `wait 1`. Backend DUMP later shows `reindexing=false`.
- **Open first path.** Click the first path row. Script: `click-id paths/0` then `wait 3` then `dump`. Stderr contains `DUMP tab=Path` and `DUMP tab=… path=Some(`.
- **Select a step.** Click a step header. Script: `click-id step/0` then `wait 2` then `dump`. Stderr `DUMP tab=` shows `step=Some(`.
- **Screenshot.** Capture the document. Script: `shot <artifacts>/open-path/document.png`. PNG shows the Paths list and Path document.
- **CLI list.** List paths. Run `control-codemap cli -- paths`. Exit code `0`; stdout lists path names with kinds.
- **CLI read.** Print one named path from the list output (for example `anchor` or `cli`). Run `control-codemap cli -- path <name>`. Exit code `0`; stdout includes the path note and at least one step with a file:line range.
- **Proof.** Save CLI transcripts to `artifacts/open-path/paths.txt` and `artifacts/open-path/path.txt`, keep the DUMP excerpt in `artifacts/open-path/dump.txt`, and keep `document.png`. `meta.txt` records entry points `paths/0` and `cli path`.

## Gotchas

- Path row indices are the filtered visible list, not stable path ids. Prefer confirming the opened path via DUMP/`path` name rather than assuming index `0` is a particular name.
- `idle` before the first click; a cold cache can leave the Paths list empty for a few seconds.
- Do not `click-id save` unless you intend to persist GUI edits; read-only open does not need it.
- CLI `path` needs the exact path name as stored in the map (`paths` output).
