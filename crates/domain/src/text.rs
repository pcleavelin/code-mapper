use std::fmt;
use std::iter;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Line(u32);

impl Line {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) fn at(position: usize) -> Option<Self> {
        u32::try_from(position).ok().map(Self)
    }

    pub const fn value(self) -> u32 {
        self.0
    }

    pub const fn number(self) -> u32 {
        self.0.saturating_add(1)
    }

    pub(crate) fn position(self) -> usize {
        usize::try_from(self.0).unwrap_or_default()
    }

    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }

    pub fn previous(self) -> Option<Self> {
        self.0.checked_sub(1).map(Self)
    }

    pub fn shifted(self, offset: LineOffset) -> Option<Self> {
        let moved = i64::from(self.0) + i64::from(offset.value());
        u32::try_from(moved).ok().map(Self)
    }

    pub fn offset_from(self, base: Self) -> Option<LineOffset> {
        let offset = i64::from(self.0) - i64::from(base.0);
        i32::try_from(offset).ok().map(LineOffset::new)
    }

    pub fn distance(self, other: Self) -> LineCount {
        LineCount::new(self.0.abs_diff(other.0))
    }
}

impl fmt::Display for Line {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineCount(u32);

impl LineCount {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) fn of(count: usize) -> Self {
        Self(u32::try_from(count).unwrap_or_default())
    }

    pub const fn value(self) -> u32 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn last(self) -> Option<Line> {
        self.0.checked_sub(1).map(Line::new)
    }
}

impl fmt::Display for LineCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineOffset(i32);

impl LineOffset {
    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> i32 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for LineOffset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    start: Line,
    end: Line,
}

impl Span {
    pub fn new(start: Line, end: Line) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    pub const fn line(line: Line) -> Self {
        Self {
            start: line,
            end: line,
        }
    }

    pub const fn start(self) -> Line {
        self.start
    }

    pub const fn end(self) -> Line {
        self.end
    }

    pub fn contains(self, line: Line) -> bool {
        self.start <= line && line <= self.end
    }

    pub fn encloses(self, inner: Self) -> bool {
        self.start <= inner.start && inner.end <= self.end
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }

    pub fn lines(self) -> impl Iterator<Item = Line> {
        (self.start.value()..=self.end.value()).map(Line::new)
    }

    pub fn count(self) -> LineCount {
        LineCount::new(self.end.value() - self.start.value() + 1)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Column(u32);

impl Column {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteOffset(u32);

impl ByteOffset {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u32 {
        self.0
    }

    pub(crate) fn position(self) -> usize {
        usize::try_from(self.0).unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextHash(u64);

impl TextHash {
    const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }

    pub fn of(bytes: &[u8]) -> Self {
        Self::of_bytes(bytes.iter().copied())
    }

    pub fn of_bytes(bytes: impl IntoIterator<Item = u8>) -> Self {
        let mut hash = Self::BASIS;
        for byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(Self::PRIME);
        }
        Self(hash)
    }

    pub fn of_lines(lines: &[SourceLine]) -> Self {
        Self::of_bytes(
            lines
                .iter()
                .flat_map(|line| line.as_str().bytes().chain(iter::once(b'\n'))),
        )
    }
}

impl fmt::Display for TextHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceLine(String);

impl SourceLine {
    pub fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn trimmed(&self) -> &str {
        self.0.trim()
    }
}

impl fmt::Display for SourceLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileText {
    lines: Vec<SourceLine>,
    whole: TextHash,
}

impl Default for FileText {
    fn default() -> Self {
        Self::from("")
    }
}

impl From<&str> for FileText {
    fn from(raw: &str) -> Self {
        let expanded = raw.replace('\t', "    ");
        Self {
            lines: expanded.lines().map(SourceLine::new).collect(),
            whole: TextHash::of(expanded.as_bytes()),
        }
    }
}

impl FileText {
    pub fn line(&self, line: Line) -> Option<&SourceLine> {
        self.lines.get(line.position())
    }

    pub fn lines(&self, span: Span) -> Option<&[SourceLine]> {
        self.lines
            .get(span.start().position()..=span.end().position())
    }

    pub fn all(&self) -> &[SourceLine] {
        &self.lines
    }

    pub fn hash(&self, span: Span) -> Option<TextHash> {
        self.lines(span).map(TextHash::of_lines)
    }

    pub fn count(&self) -> LineCount {
        LineCount::of(self.lines.len())
    }

    pub fn span(&self) -> Option<Span> {
        self.count()
            .last()
            .map(|last| Span::new(Line::new(0), last))?
    }

    pub fn whole_hash(&self) -> TextHash {
        self.whole
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Revision(String);

impl Revision {
    pub fn new(revision: &str) -> Self {
        Self(revision.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Folder(&'static str);

const PACKAGE_FOLDERS: [Folder; 4] = [
    Folder("crates"),
    Folder("packages"),
    Folder("libs"),
    Folder("apps"),
];

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativePath(String);

impl RelativePath {
    pub fn new(path: &str) -> Self {
        Self(path.replace('\\', "/"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn stem(&self) -> &str {
        let base = self.0.rsplit('/').next().unwrap_or(&self.0);
        base.split('.').next().unwrap_or(base)
    }

    pub fn package(&self) -> &str {
        let mut segments = self.0.match_indices('/').map(|(at, _)| at);
        let first = segments.next().unwrap_or(self.0.len());
        let head = self.0.get(..first).unwrap_or_default();
        let end = if PACKAGE_FOLDERS.iter().any(|folder| folder.0 == head) {
            segments.next().unwrap_or(self.0.len())
        } else {
            first
        };
        self.0.get(..end).unwrap_or_default()
    }

    pub fn directory(&self) -> Option<&str> {
        let parent = self.0.rsplit_once('/')?.0;
        parent.rsplit('/').next().filter(|name| !name.is_empty())
    }

    pub fn extension(&self) -> Option<&str> {
        self.0.rsplit_once('.').map(|parts| parts.1)
    }

    pub(crate) fn last_dotted(&self) -> &str {
        self.0.rsplit('.').next().unwrap_or_default()
    }

    pub fn ends_with(&self, suffix: &str) -> bool {
        self.0.ends_with(suffix)
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests;
