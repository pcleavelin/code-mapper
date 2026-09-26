//! The GUI, pinned by scripted scenarios: the selection, tooltip, peek, status and graph nodes
//! after each step, against golden files. Opens real windows, one at a time.
//! `CODEMAP_BLESS=1 cargo test --test gui` rewrites the goldens.

mod common;

use std::sync::Mutex;

static ONE_WINDOW: Mutex<()> = Mutex::new(());

macro_rules! tests {
    ($kind:ident: $($s:ident),* $(,)?) => {$(
        #[test]
        fn $s() {
            let _one = ONE_WINDOW.lock().unwrap_or_else(|e| e.into_inner());
            let name = concat!(stringify!($kind), "-", stringify!($s));
            let played = common::gui::play(&common::bin(), name, &common::gui::$s());
            common::golden(name, played.map(|(err, after)| common::gui_state(&err) + &after));
        }
    )*};
}

gui_scenarios!(tests);
