# TODO

## CLI

1. `tour` and `tours` print different sibling orders. The parent's own signature line
   matches its namesake, a shadowing local wins over the function, steps on an `impl` block
   sink to the bottom. Skip the declaration line; make `tour-swap` pin an order that beats
   the name sort, or refuse and say why.
2. Calls through a trait bound resolve to the trait declaration, so an impl step is "not
   called by" its parent and `roots` lists every trait impl method. Treat "parent calls
   trait method T, child implements T" as a call.
3. `callers`, `callees`, `tree` concatenate every match of an ambiguous name with no
   warning; `help` never shows the `file:sym` form.
4. `step-note`, `tour-note`, `tour-swap`, `tour-rm` print nothing; `note-edit` echoes the
   whole note and a failed one should show the longest matching prefix. Add `--append` and
   notes from stdin or `@file`.
5. Multi-symbol ranges pin as absolute lines and go stale on the first edit above them.
   Anchor to the first symbol, or accept `symA..symB`.
6. `under` takes only an index; accept `file:sym` or `file:line`.
7. `search` has no file scope.
8. Absolute anchors render with an empty symbol slot in `tours`.
9. Negative claims and counts in tour notes are unverifiable from `tour` output; show what
   falls between the steps.
10. `repin` can follow a common name (`fmt`) into the wrong file once the step's symbol is
    gone. Use a stricter bar for a cross-file move.
11. `tour-move` does not bounds-check `under` (`-7` is saved as a root); give it
    `tour-add`'s check and message.
12. `tour` prints line 1 of the file as the body of a symbol-gone step; print nothing.

## GUI

1. No copy out of the Source, Tour or Graph code; ctrl+c/x/v work in text fields only.
2. The graph cannot fit a very tall tree: the node font floor stops the zoom first. Needs
   an overview mode without text.
3. Bodyless `mod foo;` declarations differ between backends: tree-sitter skips them,
   rust-analyzer reports them.
4. Hover requests have no cancellation; one for a word the pointer has left is still
   answered first.
5. `index::build` runs on the main thread before the window opens; move it to a
   background thread with progress.
6. The file browser should sort by type: directories first, then files grouped by
   extension.
7. The Source tab's horizontal scrollbar shows only while a line wider than the view is
   on screen.
8. Moving the cursor in a text field shifts the text around.

## Features

1. Comments for an agent to review and address: the owner leaves a comment on code or a
   step, the next agent session answers it in the map and clears it.

## Deferred

Each waits for its trigger.

- A daemon holding the server sessions for the CLI: cold CLI runs on changed files are the
  bottleneck of an agent session.
- Hand-written resolver for a language: a target repo uses it and has no server.
- Links in the graph (a linked step reveals the linked tour's tree): reading a journey
  across processes in the document is not enough.
- "Intentionally unmapped" marker on symbols: `uncovered` is mostly things already decided
  not to matter.
- Step-level review state: a long tour gets one re-pinned step and rereading it all is a
  cost.
- Kind-specific rendering: a list of 50 mixed-kind tours is unreadable.
- Documentation panel for the focused symbol: doc comments first, then external docs.
- Detachable windows: one window's panel tree is not enough for a real session.
- Multi-threaded search: a search takes more than 100 ms.
- Watch for new / deleted files: restarting for new files annoys.
- Undo: a mis-click deletes something that took effort to build.
- MCP server: the CLI is stable and the shell round trip is the bottleneck.
- More languages: a target repo needs one.
