# Browse files

Browse files lets a user open directories and files from the Files tab to land in a listing or code view.

## Sub-features

- `browse-files-tab` switches to the Files tab.
- `browse-files-dir` opens a directory row (`dir/<n>`).
- `browse-files-file` opens a file row (`file/<n>`).

## How to get to it (user POV)

- Click the Files tab.
- Click a directory or file row in the Files list.
- Or open a file from its row in the Symbols tab (`symfile/…`).

## Driving it with control-codemap

Preconditions:

- Compositor owned; root has source files (the repo does).
- Artifacts: `.cursor/skills/verify-codemap/artifacts/browse-files/`.

- **Open Files.** Script: `idle` / `click-id tab@Files` / `wait 2` / `dump`. Panels DUMP marks Files.
- **Enter a directory.** Script: `click-id dir/0` / `wait 2` / `dump` / `shot <artifacts>/browse-files/dir.png`. DUMP/status or listing reflects the directory.
- **Open a file.** Script: `click-id file/0` / `wait 3` / `dump` / `shot <artifacts>/browse-files/file.png`. A code/listing view shows file content; DUMP `file=Some(` when focus tracks a file.
- **Proof.** Keep DUMP + PNGs; `meta.txt` notes `tab@Files`, `dir/0`, `file/0`.

## Gotchas

- Directory and file indices depend on sort order under the root; assert via DUMP/screenshot content, not a hard-coded path name at index 0.
- After entering a directory, wait before clicking `file/0` — the list rebuilds one frame later.
- Symbols-tab `symfile` entry is a second path; do not skip it if claiming full coverage of How to get to it.
