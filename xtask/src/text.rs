use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RepoPath(String);

impl RepoPath {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.replace('\\', "/").trim_start_matches("./").to_owned())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn starts_with(&self, prefix: &str) -> bool {
        self.0.starts_with(prefix)
    }

    pub(crate) fn ends_with(&self, suffix: &str) -> bool {
        self.0.ends_with(suffix)
    }

    pub(crate) fn contains(&self, part: &str) -> bool {
        self.0.contains(part)
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Literal(&'static str);

impl Literal {
    pub(crate) const fn new(text: &'static str) -> Self {
        Self(text)
    }

    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }

    pub(crate) fn found_in(self, text: &str) -> bool {
        text.contains(self.0)
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Content(String);

impl Content {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LineNumber(usize);

impl LineNumber {
    pub(crate) const fn from_row(row: usize) -> Self {
        Self(row + 1)
    }
}

impl fmt::Display for LineNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) struct Message(String);

impl Message {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    pub(crate) fn push_line(&mut self, line: &str) {
        self.0.push_str(line);
        self.0.push('\n');
    }

    pub(crate) fn head(&self, lines: usize) -> Self {
        let kept: Vec<&str> = self.0.lines().take(lines).collect();
        let dropped = self.0.lines().count().saturating_sub(lines);
        let text = kept.join("\n");
        if dropped > 0 {
            Self(format!("{text}\n... {dropped} more lines"))
        } else {
            Self(text)
        }
    }
}

impl From<String> for Message {
    fn from(text: String) -> Self {
        Self(text)
    }
}

impl fmt::Display for Message {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RuleCode(&'static str);

impl RuleCode {
    pub(crate) const fn new(code: &'static str) -> Self {
        Self(code)
    }
}

impl fmt::Display for RuleCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TypeName(String);

impl TypeName {
    pub(crate) fn new(text: &str) -> Self {
        let bare = text.split('<').next().unwrap_or(text);
        Self(bare.rsplit("::").next().unwrap_or(bare).trim().to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Word(String);

impl Word {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.to_lowercase())
    }

    pub(crate) fn split(identifier: &str) -> Vec<Self> {
        let mut words = Vec::new();
        for part in identifier.split('_').filter(|part| !part.is_empty()) {
            let mut current = String::new();
            let mut previous_lower = false;
            for character in part.chars() {
                if character.is_uppercase() && previous_lower && !current.is_empty() {
                    words.push(Self::new(&current));
                    current.clear();
                }
                previous_lower = character.is_lowercase() || character.is_ascii_digit();
                current.push(character);
            }
            if !current.is_empty() {
                words.push(Self::new(&current));
            }
        }
        words
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn forms(&self) -> Vec<Self> {
        let mut forms = vec![self.clone()];
        if let Some(stem) = self.0.strip_suffix("ies") {
            forms.push(Self(format!("{stem}y")));
        }
        if let Some(stem) = self.0.strip_suffix("es") {
            forms.push(Self(stem.to_owned()));
        }
        if let Some(stem) = self.0.strip_suffix('s') {
            forms.push(Self(stem.to_owned()));
        }
        forms
    }
}

impl fmt::Display for Word {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LintName(String);

impl LintName {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.split_whitespace().collect())
    }

    pub(crate) fn list(arguments: &str) -> Vec<Self> {
        let before_reason = arguments.split("reason").next().unwrap_or_default();
        before_reason
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(Self::new)
            .collect()
    }
}

impl fmt::Display for LintName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CrateName(String);

impl CrateName {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.trim().to_owned())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CrateName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Argument(String);

impl Argument {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub(crate) fn list(words: &[&str]) -> Vec<Self> {
        words.iter().map(|word| Self::new(*word)).collect()
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Program(&'static str);

impl Program {
    pub(crate) const CARGO: Self = Self("cargo");
    pub(crate) const RUSTFMT: Self = Self("rustfmt");
    pub(crate) const GIT: Self = Self("git");
    pub(crate) const JJ: Self = Self("jj");

    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for Program {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Root(PathBuf);

impl Root {
    pub(crate) fn of_workspace() -> Self {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        Self(manifest.parent().unwrap_or(manifest).to_path_buf())
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn join(&self, path: &RepoPath) -> PathBuf {
        self.0.join(path.as_str())
    }

    pub(crate) fn relative(&self, path: &Path) -> Option<RepoPath> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.0.join(path)
        };
        absolute
            .strip_prefix(&self.0)
            .ok()
            .and_then(Path::to_str)
            .map(RepoPath::new)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) struct Count(usize);

impl Count {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn new(value: usize) -> Self {
        Self(value)
    }

    pub(crate) const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for Count {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stamp(u64);

impl Stamp {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        u64::from_str_radix(text.trim(), 16).ok().map(Self)
    }
}

impl fmt::Display for Stamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

#[derive(Default, Debug)]
pub(crate) struct Hasher(u64);

impl Hasher {
    pub(crate) const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    pub(crate) const fn finish(&self) -> Stamp {
        Stamp::new(self.0)
    }
}
