#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::disallowed_methods,
    reason = "a test harness fails by panicking, builds fixtures on disk and runs the binary"
)]

mod common;

use std::sync::Mutex;

static ONE_WINDOW: Mutex<()> = Mutex::new(());

macro_rules! tests {
    ($kind:ident: $($s:ident),* $(,)?) => {$(
        #[test]
        fn $s() {
            let _one = ONE_WINDOW.lock().unwrap_or_else(|error| error.into_inner());
            let name = concat!(stringify!($kind), "-", stringify!($s));
            let played = common::gui::play(&common::bin(), name, &common::gui::$s());
            common::golden(name, played.map(|(err, after)| common::gui_state(&err) + &after));
        }
    )*};
}

gui_scenarios!(tests);
