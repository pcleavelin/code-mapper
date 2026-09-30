# Open issues

Found by agent mapping sessions and the owner's use of the GUI. Fixed items are removed,
not ticked. The CLI list is ordered by what cost the agents the most.

## CLI

1. **`path` and `paths` print different sibling orders.** The document sorts callees by
   first mention in the parent's text; the parent's own signature line matches its
   namesake, so the repository call renders above the fetchers that run first (four
   flows in memejoin-rs). A shadowing local wins over the function of the same name.
   Steps anchored to an `impl` block never match and sink below everything. `path-swap`
   only reorders unmatched siblings, so in the case it is wanted for it silently does
   nothing. Skip the parent's declaration line when scanning; make `path-swap` pin an
   explicit order that beats the name sort, or refuse and say why.
2. **Calls through a trait bound resolve to the trait declaration.** Anchoring a flow
   step to the impl a reader wants is always "not called by" its parent, and `roots`
   lists every trait impl method as an entry point. Treat "parent calls trait method T,
   child implements T" as a call. The "does not call" note also compares the enclosing
   symbol, not the anchored lines.
3. **Ambiguous names.** `callers`, `callees`, `tree` concatenate every match with no
   warning (`callers get_guild`: six symbols' results in one block); only `promote`
   errors with "ambiguous". `help` never shows the `file:sym` form. The agent gave up on
   names and anchored all 212 steps by `file start end`.
4. **Silent mutations.** `step-note`, `path-note`, `path-swap`, `path-rm` print nothing;
   `note-edit` echoes the whole note; a failed `note-edit` restates the needle (one
   apostrophe off in a 120-character needle) instead of the longest matching prefix.
   Wanted: `--append` for notes; notes from stdin or `@file` so a backtick in prose
   cannot lose a batch of ten through the shell.
5. **Multi-symbol ranges pin as absolute lines** and go stale on the first edit above
   them (about 40 of 212 steps in memejoin-rs: files of tiny `From`/`Display` impls).
   Wanted: anchor to the first symbol with a span, or `symA..symB`.
6. **`under` wants an index that batching cannot predict.** Accept a `file:sym` or
   `file:line` target.
7. **`grep` has no file scope.**
8. **Absolute anchors render with an empty symbol slot** in `paths`.
9. **"Intentionally unmapped" marker** (Deferred, below): a third of
    memejoin-rs is a dead previous version; `coverage` cannot distinguish explained from
    acknowledged-dead.
10. **Negative claims and counts are unverifiable from `path` output** ("the only
    handler that names X", "twenty-six methods"); the reviewer found every one it
    checked wrong. Per-path "what falls between the steps" would help.
11. **`repin` can follow a common name into the wrong file.** Once a step's file, or its
    symbol within the file, is gone, every same-named symbol in the repo is a candidate,
    and only "half the lines survive" guards it. A short `fmt` step whose `impl Display`
    was deleted can land on another type's identical `fmt`. The new file is printed, so
    the reread catches it; a stricter bar for a cross-file move (all lines kept, or the
    candidate absent from the parent revision) would stop it.
12. **`path-move` does not bounds-check `under`**, unlike `path-add`. `-7` is saved as
    `-1` (a root) and a number past the step count fails with "no such parent step".
    Give it `path-add`'s check and message.
13. **`path` prints line 1 of the file as the body of a symbol-gone step** (`STALE
    src/main.rs (symbol gone) log_line` followed by `1 mod shapes;`). Print nothing, the
    way the GUI document does.

## GUI

Left open after the UX review of 2026-09-21 and the fixes that followed it.

1. No clipboard: ctrl and alt drop text input, nothing pastes into a field or copies out
   of a code view.
2. The 0.3 zoom floor is not reachable: `graph_px` floors the node font at 6 px, so on a
   14 px UI the real floor is about 0.43 and `fit` cannot show a very tall tree
   (`legacy-v1`, 5400 px); it lands at the floor, top-left aligned. Readable text and a
   whole-tree overview conflict; an overview mode without text is the way out.
3. Bodyless `mod foo;` declarations differ between backends: tree-sitter skips them so
   `.map()` does not link to `mod map;`, rust-analyzer reports them; on a tree-sitter
   index `module-tree` has two "symbol gone" steps.
4. Hover requests have no cancellation: a request in flight for a word the pointer has
   left is still answered before the one it rests on; one in flight at a time bounds it.
5. With several language servers missing, the status line says whichever failure arrived
   last: the server threads fail at once and are drained in HashMap order. Name every
   missing server in one line, in a fixed order.

6. `index::build` reads the whole tree into memory on the main thread before the window
   opens. Move it to a background thread with progress when startup on a big repo annoys.

## Tests

1. The GUI scripts settle with `idle` and a save before their first dump only to hide GUI
   item 5; drop the save once the status line is deterministic.
2. GUI scenarios run one at a time in a real window; on Linux a hidden or unfocused window can
   stop getting frames from the compositor and a script stalls. A private headless compositor
   (weston --backend=headless) with software Vulkan runs them reliably, at about a minute per
   scenario.

## Planned

**Prompts in the code.** The owner leaves a marker comment in a source file where a question
or a request for the map belongs (`// codemap: why does this retry three times?`). `prompts`
lists every marker with file:line and text; the agent contract tells a session to answer
each one in the map (a note, a step, a path) and remove the marker in the same commit; the
GUI shows the open markers in the listing and counts them in the status bar. Done when: the
owner writes a question in a comment, the next agent session's map answers it, and the
marker is gone from the diff.

## Deferred

Each item waits for the trigger beside it. Work on one before its trigger needs the reason
written here first.

| Item | Trigger |
|---|---|
| A daemon holding the server sessions for the CLI | cold CLI runs on changed files are the bottleneck of an agent session |
| Hand-written resolver for a language | a target repo uses it and has no server |
| Links in the graph: a linked step expands the linked path's tree from its node | reading a journey across processes in the document is not enough |
| "Intentionally unmapped" marker on symbols | `uncovered` is mostly things already decided not to matter |
| Step-level review state | a long path gets one re-pinned step and rereading it all is a cost |
| Kind-specific rendering | a list of 50 mixed-kind paths is unreadable |
| Documentation panel for the focused symbol | per language: doc comments first, then external docs |
| Detachable windows | one window's panel tree is not enough for a real session |
| Multi-threaded grep | a search takes more than 100 ms |
| Watch for new / deleted files | restarting for new files annoys |
| Undo | a mis-click deletes something that took effort to build |
| MCP server | the CLI is stable and the shell round trip is the bottleneck |
| More languages | a target repo needs one |
