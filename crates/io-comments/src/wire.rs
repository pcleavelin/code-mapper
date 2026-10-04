use domain::Author;
use strum::VariantArray;

use crate::error::{CommentFault, FieldKey, FieldValue};

pub(crate) const VERSION: &str = "codemap-comment 1";

const VERSION_KEY: &str = "codemap-comment";

const VERSION_VALUE: &str = "1";

pub(crate) const COMMENT_EXTENSION: &str = "comment";

pub(crate) const SHARED_DIRECTORY: &str = "codemap-comments";

pub(crate) const LOCAL_DIRECTORY: &str = ".codemap-comments";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyName(&'static str);

impl KeyName {
    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
pub(crate) enum CommentKey {
    Target,
    Tour,
    Step,
    File,
    Symbol,
    Lines,
    Hash,
    Author,
    Text,
    Reply,
    ReplyAuthor,
}

impl CommentKey {
    pub(crate) const fn name(self) -> KeyName {
        KeyName(match self {
            Self::Target => "target",
            Self::Tour => "tour",
            Self::Step => "step",
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::Lines => "lines",
            Self::Hash => "hash",
            Self::Author => "author",
            Self::Text => "text",
            Self::Reply => "reply",
            Self::ReplyAuthor => "reply-author",
        })
    }

    fn named(key: &str) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|entry| entry.name().as_str() == key)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
pub(crate) enum TargetKind {
    Step,
    Tour,
    Code,
}

impl TargetKind {
    const fn name(self) -> KeyName {
        KeyName(match self {
            Self::Step => "step",
            Self::Tour => "tour",
            Self::Code => "code",
        })
    }

    fn named(value: &str) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|entry| entry.name().as_str() == value)
    }
}

pub(crate) struct WireComment {
    pub(crate) target: Option<TargetKind>,
    pub(crate) tour: Option<String>,
    pub(crate) step: Option<String>,
    pub(crate) file: Option<String>,
    pub(crate) symbol: String,
    pub(crate) lines: Option<(i32, i32)>,
    pub(crate) hash: Option<u64>,
    pub(crate) author: Option<Author>,
    pub(crate) text: String,
    pub(crate) reply: Option<String>,
    pub(crate) reply_author: Option<Author>,
}

pub(crate) struct WireFault {
    pub(crate) line: Option<u32>,
    pub(crate) fault: CommentFault,
}

const AUTHORS: [Author; 2] = [Author::Human, Author::Agent];

fn author_name(author: Author) -> KeyName {
    KeyName(match author {
        Author::Human => "human",
        Author::Agent => "ai",
    })
}

fn parse_author(value: &str) -> Option<Author> {
    AUTHORS
        .into_iter()
        .find(|author| author_name(*author).as_str() == value)
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            match characters.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some(escaped) => out.push(escaped),
                None => out.push('\\'),
            }
        } else {
            out.push(character);
        }
    }
    out
}

fn parse_lines(value: &str) -> Option<(i32, i32)> {
    let mut parts = value.split(' ');
    let start = parts.next()?.parse().ok()?;
    let end = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((start, end))
}

fn parse_hash(value: &str) -> Option<u64> {
    let digits =
        value.chars().count() == 16 && value.chars().all(|digit| digit.is_ascii_hexdigit());
    if digits {
        u64::from_str_radix(value, 16).ok()
    } else {
        None
    }
}

pub(crate) fn parse(text: &str) -> Result<WireComment, WireFault> {
    let mut comment: Option<WireComment> = None;
    for (line, raw_line) in (0u32..).zip(text.lines()) {
        let at = |fault: CommentFault| WireFault {
            line: Some(line),
            fault,
        };
        if raw_line.is_empty() {
            continue;
        }
        let (key, raw) = raw_line.split_once(' ').unwrap_or((raw_line, ""));
        let value = unescape(raw);
        if key == VERSION_KEY {
            if raw_line != VERSION || comment.is_some() {
                return Err(at(CommentFault::Version(FieldValue::new(raw_line))));
            }
            comment = Some(WireComment {
                target: None,
                tour: None,
                step: None,
                file: None,
                symbol: String::new(),
                lines: None,
                hash: None,
                author: None,
                text: String::new(),
                reply: None,
                reply_author: None,
            });
            continue;
        }
        let Some(comment) = comment.as_mut() else {
            return Err(at(CommentFault::NoVersion));
        };
        let author = |name: &str| {
            parse_author(name).ok_or_else(|| at(CommentFault::UnknownAuthor(FieldValue::new(name))))
        };
        match CommentKey::named(key) {
            Some(CommentKey::Target) => {
                comment.target = Some(
                    TargetKind::named(&value)
                        .ok_or_else(|| at(CommentFault::UnknownTarget(FieldValue::new(&value))))?,
                );
            }
            Some(CommentKey::Tour) => comment.tour = Some(value),
            Some(CommentKey::Step) => comment.step = Some(value),
            Some(CommentKey::File) => comment.file = Some(value),
            Some(CommentKey::Symbol) => comment.symbol = value,
            Some(CommentKey::Lines) => {
                comment.lines = Some(parse_lines(&value).ok_or_else(|| at(CommentFault::Lines))?);
            }
            Some(CommentKey::Hash) => {
                comment.hash = Some(parse_hash(&value).ok_or_else(|| at(CommentFault::Hash))?);
            }
            Some(CommentKey::Author) => comment.author = Some(author(&value)?),
            Some(CommentKey::Text) => comment.text = value,
            Some(CommentKey::Reply) => comment.reply = Some(value),
            Some(CommentKey::ReplyAuthor) => comment.reply_author = Some(author(&value)?),
            None => return Err(at(CommentFault::UnknownField(FieldKey::new(key)))),
        }
    }
    comment.ok_or(WireFault {
        line: None,
        fault: CommentFault::NoVersion,
    })
}

struct CommentFile {
    text: String,
}

impl CommentFile {
    fn field(&mut self, key: CommentKey, value: &str) {
        self.text.push_str(key.name().as_str());
        self.text.push(' ');
        self.text.push_str(&escape(value));
        self.text.push('\n');
    }

    fn optional(&mut self, key: CommentKey, value: Option<&str>) {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            self.field(key, value);
        }
    }
}

pub(crate) fn render(comment: &WireComment) -> String {
    let mut out = CommentFile {
        text: format!("{VERSION_KEY} {VERSION_VALUE}\n"),
    };
    let target = comment.target.unwrap_or(TargetKind::Tour);
    out.field(CommentKey::Target, target.name().as_str());
    out.optional(CommentKey::Tour, comment.tour.as_deref());
    out.optional(CommentKey::Step, comment.step.as_deref());
    out.optional(CommentKey::File, comment.file.as_deref());
    out.optional(CommentKey::Symbol, Some(comment.symbol.as_str()));
    if let Some((start, end)) = comment.lines {
        out.field(CommentKey::Lines, &format!("{start} {end}"));
    }
    if let Some(hash) = comment.hash {
        out.field(CommentKey::Hash, &format!("{hash:016x}"));
    }
    out.field(
        CommentKey::Author,
        author_name(comment.author.unwrap_or(Author::Human)).as_str(),
    );
    out.field(CommentKey::Text, &comment.text);
    out.optional(CommentKey::Reply, comment.reply.as_deref());
    if let Some(author) = comment.reply_author {
        out.field(CommentKey::ReplyAuthor, author_name(author).as_str());
    }
    out.text
}
