---
name: add-feature
description: Add anything a user can do with codemap - a CLI command, a GUI button, key binding, gesture or panel action. Use whenever a change gives the user a new function or a new way to trigger one.
---

# Add a user-facing function

Every function the user has is one `Feature` in `crates/features`. The CLI and the GUI build
their commands, buttons and key bindings from it, so a trigger that is not in the registry
cannot be written.

1. `understand`: the request restated, why it is needed, how it is meant to work.
2. `prototype`: several designs built and run, one kept. The steps below finish the one
   that was kept.
3. `crates/features/src/lib.rs`: add the variant to `Feature` and its
   arm in `spec()`: the name (kebab-case, also the CLI command name), a one-line summary
   (the CLI help line or the button's tooltip), the surface, and every trigger (command,
   key chord, click on a named element, a gesture). `cargo test -p features` checks names and
   triggers are unique.
4. The handler: one function that performs it.
   - A command: follow `add-cli-command`.
   - A GUI function: follow `add-gui-element`.
   - Anything that changes the map is a method on `domain::Map` first (`add-domain-type` if
     it needs a new concept).
5. Tests: the behaviour stated before the code, as a unit test of the handler or domain method;
   then a CLI scenario line for every outcome (success and each error), or a GUI scenario
   step that triggers it and a dump that shows its effect (`add-test-scenario`).
6. The map: a `flow` tour named `feature-<name>` in group `features/cli` or `features/gui`,
   whose note is the case (CLAUDE.md, "Every feature makes its case") as `understand` and
   `prototype` completed it, whose root step is the handler and whose steps
   follow what it calls (`codemap . promote <handler> 2 feature-<name>`, then `tour-group`,
   `tour-note`, trim, annotate). For help, an out-of-box flow, or onboarding, the note
   keeps the Walk line from `understand`, and `finish` rejects the note while the walk
   fails on the surface.
7. `finish`.
