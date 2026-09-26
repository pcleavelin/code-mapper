//! The GUI, pinned by scripted scenarios: the selection, tooltip, peek, status and graph nodes
//! after each step, against golden files. Opens real windows, one at a time.
//! `CODEMAP_BLESS=1 cargo test --test gui` rewrites the goldens.

mod common;

use std::sync::Mutex;

static ONE_WINDOW: Mutex<()> = Mutex::new(());

fn scenario(name: &str) {
    let _one = ONE_WINDOW.lock().unwrap_or_else(|e| e.into_inner());
    let s = common::gui::SCENARIOS
        .iter()
        .find(|s| s.name == name)
        .unwrap();
    let (err, after) = common::gui::play(&common::bin(), s);
    common::golden(name, &(common::gui_state(&err) + &after));
}

#[test]
fn document() {
    scenario("gui-document");
}

#[test]
fn peek() {
    scenario("gui-peek");
}

#[test]
fn listing() {
    scenario("gui-listing");
}

#[test]
fn graph() {
    scenario("gui-graph");
}

#[test]
fn panels() {
    scenario("gui-panels");
}

#[test]
fn delete() {
    scenario("gui-delete");
}

#[test]
fn diff() {
    scenario("gui-diff");
}

#[test]
fn reload() {
    scenario("gui-reload");
}

#[test]
fn dock() {
    scenario("gui-dock");
}
