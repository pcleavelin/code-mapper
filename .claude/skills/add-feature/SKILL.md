---
name: add-feature
description: Add anything a user can do with codemap - a CLI command, a GUI button, key binding, gesture or panel action. Use whenever a change gives the user a new function or a new way to trigger one.
---

# Add a user-facing function

Every function the user has is one `Feature` in `crates/features`. The CLI and the GUI build
their commands, buttons and key bindings from it, so a trigger that is not in the registry
cannot be written.

1. `crates/features/src/lib.rs`: add the variant to `Feature` and to `Feature::ALL`, and its
   arm in `spec()`: the name (kebab-case, also the CLI command name), a one-line summary
   (the CLI help line or the button's tooltip), the surface, and every trigger (command,
   key chord, click on a named element, a gesture). `cargo test -p features` checks names and
   triggers are unique.
2. The handler: one function that performs it.
   - A command: follow `add-cli-command`.
   - A GUI function: follow `add-gui-element`.
   - Anything that changes the map is a method on `domain::Map` first (`add-domain-type` if
     it needs a new concept).
3. Tests: a CLI golden line for every outcome (success and each error), or a GUI scenario
   step that triggers it (`add-test-scenario`).
4. The map: a `flow` path named `feature-<name>` in group `features/cli` or `features/gui`,
   whose root step is the handler and whose steps follow what it calls
   (`codemap . promote <handler> 2 feature-<name>`, then `path-group`, trim, annotate).
5. `finish`.
