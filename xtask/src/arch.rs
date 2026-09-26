use crate::lint::Rule;
use crate::source::{SourceFile, Zone};
use crate::text::{CrateName, LintName, Literal, RepoPath};

#[derive(Clone, Copy, Debug)]
struct Allowance {
    path: Literal,
    lint: Literal,
}

#[derive(Clone, Copy, Debug)]
struct Dependency {
    package: Literal,
    on: Literal,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Guard {
    File(Literal),
    Tree(Literal),
}

const EXPECTS: [Allowance; 4] = [
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
];

const DEPENDENCIES: [Dependency; 19] = [
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("winit"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("wgpu"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("fontdue"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("clap"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("ignore"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("regex"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("serde_json"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("png"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("tree-sitter"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("tree-sitter-rust"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("tree-sitter-odin"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("tree-sitter-c"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("tree-sitter-python"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("tree-sitter-javascript"),
    },
    Dependency {
        package: Literal::new("codemap"),
        on: Literal::new("tree-sitter-typescript"),
    },
    Dependency {
        package: Literal::new("xtask"),
        on: Literal::new("ignore"),
    },
    Dependency {
        package: Literal::new("xtask"),
        on: Literal::new("serde_json"),
    },
    Dependency {
        package: Literal::new("xtask"),
        on: Literal::new("tree-sitter"),
    },
    Dependency {
        package: Literal::new("xtask"),
        on: Literal::new("tree-sitter-rust"),
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

const LEGACY_RULES: [Rule; 1] = [Rule::Suppression];

const TEST_RULES: [Rule; 2] = [Rule::Suppression, Rule::TestRegistry];

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
    DEPENDENCIES
        .iter()
        .any(|entry| package.as_str() == entry.package.as_str() && on.as_str() == entry.on.as_str())
}

pub(crate) fn guarded(path: &RepoPath) -> bool {
    RULEBOOK.iter().any(|guard| match guard {
        Guard::File(file) => path.as_str() == file.as_str(),
        Guard::Tree(prefix) => path.starts_with(prefix.as_str()),
    })
}
