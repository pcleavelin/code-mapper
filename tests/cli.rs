//! The CLI, the agent's interface, pinned command by command against golden transcripts.
//! `CODEMAP_BLESS=1 cargo test --test cli` rewrites them.

mod common;

macro_rules! tests {
    ($kind:ident: $($s:ident),* $(,)?) => {$(
        #[test]
        fn $s() {
            let name = concat!(stringify!($kind), "-", stringify!($s));
            common::golden(name, common::cli::$s(&common::bin(), name));
        }
    )*};
}

cli_scenarios!(tests);
