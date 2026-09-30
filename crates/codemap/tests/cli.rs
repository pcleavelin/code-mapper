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

use common::Outcome;

macro_rules! tests {
    ($kind:ident: $($s:ident),* $(,)?) => {$(
        #[test]
        fn $s() {
            let name = concat!(stringify!($kind), "-", stringify!($s));
            common::ran(name, common::cli::$s(&common::bin(), name).outcome());
        }
    )*};
}

cli_scenarios!(tests);
