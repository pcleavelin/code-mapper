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
7. **`promote` depth 2 yields 42 steps**, 30 of them getters and builder combinators.
   A "skip symbols under N lines / with more than K callers" filter.
8. **`grep` has no file scope.**
9. **Absolute anchors render with an empty symbol slot** in `paths`.
10. **"Intentionally unmapped" marker** (design.md deferred list): a third of
    memejoin-rs is a dead previous version; `coverage` cannot distinguish explained from
    acknowledged-dead.
11. **Negative claims and counts are unverifiable from `path` output** ("the only
    handler that names X", "twenty-six methods"); the reviewer found every one it
    checked wrong. Per-path "what falls between the steps" would help.

## GUI

From the UX review of 2026-09-21 (four reviewers driving the GUI by script). Fixed items
are removed.

### Document and left panel

1. A header button whose label shrinks ("whole symbol" to "slice", "hide code" to
   "code", "no context" appearing) slides `delete` under the pointer; the next click on
   the same spot deletes the step with no confirmation, no status line, no undo. The
   likely source of "steps and nodes vanish randomly".
2. The "new path" field does nothing: the field clears its text on enter before
   `create_path` reads it, and nothing pushes `Action::NewPath`.
3. Back and forward never return to the Listing, Results or Diff tab: `select_step`
   forces the tab back to Path after `go` restored it, and the forward stack records the
   rewritten place.
4. Deleting a path or a step shifts indices under the history, the fold, collapse and
   context sets; back after `delete path` opens the wrong path; `remove_anchor`
   re-parents the deleted step's children instead of removing them; the per-path state of
   every other path is cleared too.
5. Text typed into a top-bar field overflows its box and paints over the buttons next
   to it; the caret goes off screen.
6. "expand all" only un-hides code; neither button touches folds. Labels lie.
7. The outline never scrolls to follow the document; the top-step highlight and the
   selected-step highlight are the same colour and merge with the selected path row.
8. Selecting a step the document cannot show (a folded ancestor, a lines-only step)
   leaves the three views disagreeing with no message; a lines-only step does not move
   the right panel.
9. Symbols tab rows are cut at the panel edge: file and line are unreadable, no
   ellipsis, the scrollbar thumb covers the last column. Long path names likewise.
10. Delete step and delete path give no feedback at all.
11. Results tab is an empty void with no search; a bad regex wipes the previous results.
12. A multi-line status message renders newlines as tofu.
13. "whole symbol" while the code is hidden flips its label and shows nothing.
14. Clicking the already-selected path row jumps back to its first step, losing the
    reading position.
15. Clicking a Symbols row while the Path tab is up changes only the right panel; reads as
    a dead click.
16. No keyboard in the reading views; the command line has focus from startup so stray
    keystrokes land there.
17. Back and forward never show whether there is anywhere to go; smallest targets in the
    bar.


### Listing, peek, output, diff, results

1. The peek panel's `go` and `x` buttons are laid out past the window edge whenever the
   title and place are long (an untruncated `path:line` in a Fit row); an out-of-repo peek
   then cannot be dismissed at all.
2. The output log scrolls to the bottom of the previous output, never the newest: the
   scroll is clamped against last frame's content, so a mistyped command shows no error.
3. A bad regex wipes the previous search results and the tab label goes to 0.
4. The tooltip is pushed ~200 px away from the pointer near the bottom or right edge: the
   clamp reserves 90x30 cells instead of the measured size; text lines are cut at 110
   columns, wider than the clamp assumes.
5. Fields keep their text and caret after enter, so the next entry is appended
   (`200099999`); no select-all, no paste, ctrl+u undocumented.
6. Multi-line status text renders newlines as tofu (also in the document list).
7. Long lines are cut with no horizontal scroll and no marker; the 320 px peek panel
   shows about 30 columns of a signature.
8. Ctrl-click in the document always leaves for the Listing, even when the target is a
    step of the path being read.
9. Hover has no debounce: every word the pointer crosses is a server request, answered
    in order; the hover cache is never evicted.
10. Bad go-to-line input is silently ignored; out of range clamps silently.
11. The output panel is a fixed sixth of the window, not resizable, no `clear`, one
    element per log line every frame.
12. Diff rows for removed paths are inert.
13. The References count can exceed the rows shown (refs in unindexed files are skipped).
14. The stale/step anchor bar is 2 px wide at the left edge: invisible in practice.
15. No clipboard: ctrl and alt drop text input, nothing pastes or copies.
16. Back/forward are alt+arrows (the design says ctrl); a focused field also moves its
    caret on alt+arrows.
17. Minor: empty Results shows nothing; the peek reserves a third of the height for a
    12-line body; `x` is 12x20 px; the map path in the status bar mixes separators; the
    last file line is clipped by a few pixels; out-of-repo peek text has no colours.

### Graph and state

1. The 0.3 zoom floor is not reachable: `graph_px` floors the node font at 6 px, so on a
   14 px UI the real floor is about 0.43 and `fit` cannot show a very tall tree
   (`legacy-v1`, 5400 px); it lands at the floor, top-left aligned. Readable text and a
   whole-tree overview conflict; an overview mode without text is the way out.
2. Bodyless `mod foo;` declarations differ between backends: tree-sitter skips them so
   `.map()` does not link to `mod map;`, rust-analyzer reports them; on a tree-sitter
   index `module-tree` has two "symbol gone" steps.
