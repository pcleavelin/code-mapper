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

Left open after the UX review of 2026-09-21 and the fixes that followed it.

1. No clipboard: ctrl and alt drop text input, nothing pastes into a field or copies out
   of a code view.
2. Panels are fixed: the output panel is a sixth of the window, the side panels a
   quarter; none can be dragged. Panel resizing is a planned feature.
3. The 0.3 zoom floor is not reachable: `graph_px` floors the node font at 6 px, so on a
   14 px UI the real floor is about 0.43 and `fit` cannot show a very tall tree
   (`legacy-v1`, 5400 px); it lands at the floor, top-left aligned. Readable text and a
   whole-tree overview conflict; an overview mode without text is the way out.
4. Bodyless `mod foo;` declarations differ between backends: tree-sitter skips them so
   `.map()` does not link to `mod map;`, rust-analyzer reports them; on a tree-sitter
   index `module-tree` has two "symbol gone" steps.
5. Hover requests have no cancellation: a request in flight for a word the pointer has
   left is still answered before the one it rests on; one in flight at a time bounds it.
