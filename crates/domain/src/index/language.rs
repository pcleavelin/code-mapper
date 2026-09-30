use std::fmt;

use strum::VariantArray;

use crate::text::RelativePath;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Extension(&'static str);

impl Extension {
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Program(&'static str);

impl Program {
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Program {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Argument(&'static str);

impl Argument {
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, VariantArray)]
pub enum Language {
    Rust,
    Odin,
    Clang,
    Python,
    Javascript,
}

const STANDARD_INPUT: [Argument; 1] = [Argument("--stdio")];

impl Language {
    pub const fn extensions(self) -> &'static [Extension] {
        match self {
            Self::Rust => &[Extension("rs")],
            Self::Odin => &[Extension("odin")],
            Self::Clang => &[Extension("c"), Extension("h")],
            Self::Python => &[Extension("py")],
            Self::Javascript => &[
                Extension("js"),
                Extension("mjs"),
                Extension("cjs"),
                Extension("ts"),
                Extension("tsx"),
            ],
        }
    }

    pub const fn program(self) -> Program {
        match self {
            Self::Rust => Program("rust-analyzer"),
            Self::Odin => Program("ols"),
            Self::Clang => Program("clangd"),
            Self::Python => Program("pyright-langserver"),
            Self::Javascript => Program("typescript-language-server"),
        }
    }

    pub const fn arguments(self) -> &'static [Argument] {
        match self {
            Self::Rust | Self::Odin | Self::Clang => &[],
            Self::Python | Self::Javascript => &STANDARD_INPUT,
        }
    }

    pub fn of(path: &RelativePath) -> Option<Self> {
        let last = path.last_dotted();
        Self::VARIANTS.iter().copied().find(|language| {
            language
                .extensions()
                .iter()
                .any(|extension| extension.as_str() == last)
        })
    }
}
