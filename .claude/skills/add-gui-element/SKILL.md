---
name: add-gui-element
description: Add or change a view, panel, button, row, field or gesture in the codemap GUI (crates/gui). Use after the feature exists in the registry when the element triggers a user function.
---

# Add a GUI element

1. The feature, if the element does something: `add-feature` step 3, with its triggers
   naming the element (`Trigger::Click(Element::new("<id name>"))`, a `Chord`, a `Gesture`).
2. `crates/gui/src/ids.rs`: the element's id constant, named as scripts will address it
   (`name`, `name/<n>` for rows, `name@<key>`).
3. `crates/gui/src/theme.rs`: any new colour, size, padding or gap as a named constant.
4. `crates/gui/src/widgets.rs`: build it from an existing helper; if none fits, add the helper
   there. A button that runs a feature takes the `Feature`.
5. What it does: an `Action` variant and its arm in `apply`; selection changes go through
   `nav.rs`; background work through `runtime.rs`; key chords through `keys.rs`.
6. What a script sees: extend the component's `Dump` so its state shows in `dump`.
7. A scenario step in `tests/common/gui.rs` that uses it (`add-test-scenario`), aimed by id.
8. Look at it: `CODEMAP_SHOT=<file.png> target/release/codemap .` or a script with
   `shot`, and read the PNG. Never claim a visual result without the screenshot.
9. `finish`.
