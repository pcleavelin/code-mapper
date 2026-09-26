use crate::lint::Rule;
use crate::source::{SourceFile, Zone};
use crate::text::{CrateName, LintName, Literal, RepoPath};

#[derive(Clone, Copy, Debug)]
struct Allowance {
    path: Literal,
    lint: Literal,
}

#[derive(Clone, Copy, Debug)]
struct Allowed {
    package: Literal,
    dependencies: &'static [Literal],
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Guard {
    File(Literal),
    Tree(Literal),
}

const EXPECTS: [Allowance; 9] = [
    Allowance {
        path: Literal::new("tests/common/mod.rs"),
        lint: Literal::new("dead_code"),
    },
    Allowance {
        path: Literal::new("xtask/src/files.rs"),
        lint: Literal::new("clippy::disallowed_methods"),
    },
    Allowance {
        path: Literal::new("xtask/src/process.rs"),
        lint: Literal::new("clippy::disallowed_methods"),
    },
    Allowance {
        path: Literal::new("src/gfx.rs"),
        lint: Literal::new("unsafe_code"),
    },
    Allowance {
        path: Literal::new("crates/io-process/src/lib.rs"),
        lint: Literal::new("clippy::disallowed_methods"),
    },
    Allowance {
        path: Literal::new("crates/io-store/src/lib.rs"),
        lint: Literal::new("clippy::disallowed_methods"),
    },
    Allowance {
        path: Literal::new("crates/io-lsp/src/session.rs"),
        lint: Literal::new("clippy::disallowed_methods"),
    },
    Allowance {
        path: Literal::new("crates/gui/src/runtime.rs"),
        lint: Literal::new("clippy::disallowed_methods"),
    },
    Allowance {
        path: Literal::new("crates/platform/src/gpu.rs"),
        lint: Literal::new("unsafe_code"),
    },
];

const DEPENDENCIES: [Allowed; 17] = [
    Allowed {
        package: Literal::new("codemap"),
        dependencies: &[
            Literal::new("winit"),
            Literal::new("wgpu"),
            Literal::new("fontdue"),
            Literal::new("clap"),
            Literal::new("ignore"),
            Literal::new("regex"),
            Literal::new("serde_json"),
            Literal::new("png"),
            Literal::new("tree-sitter"),
            Literal::new("tree-sitter-rust"),
            Literal::new("tree-sitter-odin"),
            Literal::new("tree-sitter-c"),
            Literal::new("tree-sitter-python"),
            Literal::new("tree-sitter-javascript"),
            Literal::new("tree-sitter-typescript"),
        ],
    },
    Allowed {
        package: Literal::new("xtask"),
        dependencies: &[
            Literal::new("ignore"),
            Literal::new("serde_json"),
            Literal::new("tree-sitter"),
            Literal::new("tree-sitter-rust"),
        ],
    },
    Allowed {
        package: Literal::new("domain"),
        dependencies: &[],
    },
    Allowed {
        package: Literal::new("io-process"),
        dependencies: &[],
    },
    Allowed {
        package: Literal::new("io-store"),
        dependencies: &[],
    },
    Allowed {
        package: Literal::new("io-source"),
        dependencies: &[Literal::new("domain"), Literal::new("ignore")],
    },
    Allowed {
        package: Literal::new("io-map"),
        dependencies: &[Literal::new("domain"), Literal::new("io-store")],
    },
    Allowed {
        package: Literal::new("io-cache"),
        dependencies: &[Literal::new("domain"), Literal::new("io-store")],
    },
    Allowed {
        package: Literal::new("io-vcs"),
        dependencies: &[Literal::new("domain"), Literal::new("io-process")],
    },
    Allowed {
        package: Literal::new("io-lsp"),
        dependencies: &[
            Literal::new("domain"),
            Literal::new("io-process"),
            Literal::new("serde_json"),
        ],
    },
    Allowed {
        package: Literal::new("index"),
        dependencies: &[
            Literal::new("domain"),
            Literal::new("io-source"),
            Literal::new("io-cache"),
            Literal::new("io-lsp"),
            Literal::new("tree-sitter"),
            Literal::new("tree-sitter-rust"),
            Literal::new("tree-sitter-odin"),
            Literal::new("tree-sitter-c"),
            Literal::new("tree-sitter-python"),
            Literal::new("tree-sitter-javascript"),
            Literal::new("tree-sitter-typescript"),
        ],
    },
    Allowed {
        package: Literal::new("features"),
        dependencies: &[],
    },
    Allowed {
        package: Literal::new("cli"),
        dependencies: &[
            Literal::new("domain"),
            Literal::new("index"),
            Literal::new("io-map"),
            Literal::new("io-vcs"),
            Literal::new("features"),
            Literal::new("clap"),
            Literal::new("regex"),
        ],
    },
    Allowed {
        package: Literal::new("ui"),
        dependencies: &[],
    },
    Allowed {
        package: Literal::new("platform"),
        dependencies: &[
            Literal::new("ui"),
            Literal::new("io-store"),
            Literal::new("winit"),
            Literal::new("wgpu"),
            Literal::new("fontdue"),
            Literal::new("png"),
        ],
    },
    Allowed {
        package: Literal::new("gui"),
        dependencies: &[
            Literal::new("domain"),
            Literal::new("index"),
            Literal::new("io-map"),
            Literal::new("io-vcs"),
            Literal::new("io-lsp"),
            Literal::new("cli"),
            Literal::new("features"),
            Literal::new("ui"),
            Literal::new("platform"),
            Literal::new("regex"),
        ],
    },
    Allowed {
        package: Literal::new("codemap"),
        dependencies: &[
            Literal::new("domain"),
            Literal::new("index"),
            Literal::new("io-map"),
            Literal::new("cli"),
            Literal::new("gui"),
        ],
    },
];

pub(crate) const RULEBOOK: [Guard; 10] = [
    Guard::File(Literal::new("conventions.md")),
    Guard::Tree(Literal::new("xtask/")),
    Guard::File(Literal::new("clippy.toml")),
    Guard::File(Literal::new("rustfmt.toml")),
    Guard::File(Literal::new(".gitattributes")),
    Guard::File(Literal::new(".cargo/config.toml")),
    Guard::File(Literal::new(".claude/settings.json")),
    Guard::Tree(Literal::new(".claude/skills/")),
    Guard::Tree(Literal::new("api/io-")),
    Guard::File(Literal::new("CLAUDE.md")),
];

const LEGACY_RULES: [Rule; 2] = [Rule::Suppression, Rule::Comment];

const TEST_RULES: [Rule; 3] = [Rule::Suppression, Rule::TestRegistry, Rule::Comment];

const SCENARIO_FILES: [Literal; 2] = [Literal::new("tests/cli.rs"), Literal::new("tests/gui.rs")];

pub(crate) fn applies(rule: Rule, file: &SourceFile) -> bool {
    match file.zone {
        Zone::Strict => rule != Rule::TestRegistry,
        Zone::Legacy => LEGACY_RULES.contains(&rule),
        Zone::Test => TEST_RULES.contains(&rule),
    }
}

pub(crate) fn scenario_file(path: &RepoPath) -> bool {
    SCENARIO_FILES
        .iter()
        .any(|file| path.as_str() == file.as_str())
}

pub(crate) fn expected(path: &RepoPath, lint: &LintName) -> bool {
    EXPECTS.iter().any(|entry| {
        path.as_str() == entry.path.as_str() && *lint == LintName::new(entry.lint.as_str())
    })
}

pub(crate) fn allowed_dependency(package: &CrateName, on: &CrateName) -> bool {
    DEPENDENCIES.iter().any(|entry| {
        package.as_str() == entry.package.as_str()
            && entry
                .dependencies
                .iter()
                .any(|dependency| on.as_str() == dependency.as_str())
    })
}

pub(crate) fn guarded(path: &RepoPath) -> bool {
    RULEBOOK.iter().any(|guard| match guard {
        Guard::File(file) => path.as_str() == file.as_str(),
        Guard::Tree(prefix) => path.starts_with(prefix.as_str()),
    })
}
