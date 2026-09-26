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
