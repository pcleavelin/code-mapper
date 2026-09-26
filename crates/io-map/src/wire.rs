use domain::{Author, PathKind};

use crate::error::{Fault, FieldKey, FieldValue};

pub(crate) const VERSION: &str = "codemap 8";

const VERSION_KEY: &str = "codemap";

const VERSION_VALUE: &str = "8";

const CONFLICT_MARKS: [&str; 5] = ["<<<<<<<", "=======", ">>>>>>>", "%%%%%%%", "+++++++"];

pub(crate) const MAP_EXTENSION: &str = "cmap";

pub(crate) const MAP_DIRECTORY: &str = ".codemap";

pub(crate) const ORDER_KEY: &str = "order";

pub(crate) const FILE_KEY: &str = "file";

pub(crate) const LINES_KEY: &str = "lines";

pub(crate) const HASH_KEY: &str = "hash";

pub(crate) struct CmapPath {
    pub(crate) name: Option<(String, u32)>,
    pub(crate) kind: PathKind,
    pub(crate) author: Author,
    pub(crate) group: String,
    pub(crate) note: String,
    pub(crate) steps: Vec<CmapStep>,
}

pub(crate) struct CmapStep {
    pub(crate) id: String,
    pub(crate) line: u32,
    pub(crate) order: Option<u32>,
    pub(crate) parent: Option<(String, u32)>,
    pub(crate) author: Author,
    pub(crate) file: Option<String>,
    pub(crate) symbol: String,
    pub(crate) lines: Option<(i32, i32)>,
    pub(crate) hash: Option<u64>,
    pub(crate) link: Option<(String, u32)>,
    pub(crate) note: String,
}

pub(crate) struct WireFault {
    pub(crate) line: u32,
    pub(crate) fault: Fault,
}

fn author_name(author: Author) -> &'static str {
    match author {
        Author::Human => "human",
        Author::Agent => "ai",
    }
}

fn parse_author(value: &str) -> Option<Author> {
    match value {
        "human" => Some(Author::Human),
        "ai" => Some(Author::Agent),
        _ => None,
    }
}

fn kind_name(kind: PathKind) -> &'static str {
    match kind {
        PathKind::Flow => "flow",
        PathKind::Layer => "layer",
        PathKind::Type => "type",
    }
}

fn parse_kind(value: &str) -> Option<PathKind> {
    match value {
        "flow" => Some(PathKind::Flow),
        "layer" => Some(PathKind::Layer),
        "type" => Some(PathKind::Type),
        _ => None,
    }
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

fn parse_order(value: &str) -> Option<u32> {
    value
        .chars()
        .all(|digit| digit.is_ascii_digit())
        .then(|| value.parse().ok())
        .flatten()
}

pub(crate) fn parse(text: &str) -> Result<Vec<CmapPath>, WireFault> {
    let mut paths: Vec<CmapPath> = Vec::new();
    let mut in_step = false;
    for (line, raw_line) in (0u32..).zip(text.lines()) {
        let at = |fault: Fault| WireFault { line, fault };
        if CONFLICT_MARKS.iter().any(|mark| raw_line.starts_with(mark)) {
            return Err(at(Fault::Conflict));
        }
        if raw_line.is_empty() {
            continue;
        }
        let (key, raw) = raw_line.split_once(' ').unwrap_or((raw_line, ""));
        let value = unescape(raw);
        if key == VERSION_KEY {
            if raw_line != VERSION {
                return Err(at(Fault::Version(FieldValue::new(raw_line))));
            }
            paths.push(CmapPath {
                name: None,
                kind: PathKind::Flow,
                author: Author::Agent,
                group: String::new(),
                note: String::new(),
                steps: Vec::new(),
            });
            in_step = false;
            continue;
        }
        let Some(path) = paths.last_mut() else {
            return Err(at(Fault::NoVersion));
        };
        let author = |name: &str| {
            parse_author(name).ok_or_else(|| at(Fault::UnknownAuthor(FieldValue::new(name))))
        };
        if key == "step" {
            if path.steps.iter().any(|step| step.id == value) {
                return Err(at(Fault::SecondStep(FieldValue::new(&value))));
            }
            path.steps.push(CmapStep {
                id: value,
                line,
                order: None,
                parent: None,
                author: path.author,
                file: None,
                symbol: String::new(),
                lines: None,
                hash: None,
                link: None,
                note: String::new(),
            });
            in_step = true;
            continue;
        }
        if !in_step {
            match key {
                "path" => path.name = Some((value, line)),
                "kind" => {
                    path.kind = parse_kind(&value)
                        .ok_or_else(|| at(Fault::UnknownKind(FieldValue::new(&value))))?;
                }
                "author" => path.author = author(&value)?,
                "group" => path.group = value,
                "note" => path.note = value,
                _ => return Err(at(Fault::UnknownPathField(FieldKey::new(key)))),
            }
            continue;
        }
        let Some(step) = path.steps.last_mut() else {
            return Err(at(Fault::UnknownPathField(FieldKey::new(key))));
        };
        match key {
            "parent" => step.parent = Some((value, line)),
            ORDER_KEY => step.order = Some(parse_order(&value).ok_or_else(|| at(Fault::Order))?),
            "author" => step.author = author(&value)?,
            FILE_KEY => step.file = Some(value),
            "symbol" => step.symbol = value,
            LINES_KEY => step.lines = Some(parse_lines(&value).ok_or_else(|| at(Fault::Lines))?),
            HASH_KEY => step.hash = Some(parse_hash(&value).ok_or_else(|| at(Fault::Hash))?),
            "link" => step.link = Some((value, line)),
            "note" => step.note = value,
            _ => return Err(at(Fault::UnknownStepField(FieldKey::new(key)))),
        }
    }
    Ok(paths)
}

pub(crate) struct CmapText {
    text: String,
}

impl CmapText {
    fn field(&mut self, key: &str, value: &str) {
        self.text.push_str(key);
        self.text.push(' ');
        self.text.push_str(&escape(value));
        self.text.push('\n');
    }

    fn optional(&mut self, key: &str, value: &str) {
        if !value.is_empty() {
            self.field(key, value);
        }
    }
}

pub(crate) fn render(path: &CmapPath) -> String {
    let mut out = CmapText {
        text: String::new(),
    };
    out.field(VERSION_KEY, VERSION_VALUE);
    let name = path.name.as_ref().map_or("", |name| name.0.as_str());
    out.field("path", name);
    out.field("kind", kind_name(path.kind));
    out.field("author", author_name(path.author));
    out.optional("group", &path.group);
    out.optional("note", &path.note);
    let mut by_id: Vec<&CmapStep> = path.steps.iter().collect();
    by_id.sort_by(|one, other| one.id.cmp(&other.id));
    for step in by_id {
        out.text.push('\n');
        out.field("step", &step.id);
        out.field(ORDER_KEY, &step.order.unwrap_or_default().to_string());
        if let Some((parent, _)) = &step.parent {
            out.field("parent", parent);
        }
        out.field("author", author_name(step.author));
        out.field(FILE_KEY, step.file.as_deref().unwrap_or_default());
        out.optional("symbol", &step.symbol);
        let (start, end) = step.lines.unwrap_or_default();
        out.field(LINES_KEY, &format!("{start} {end}"));
        out.field(HASH_KEY, &format!("{:016x}", step.hash.unwrap_or_default()));
        if let Some((link, _)) = &step.link {
            out.optional("link", link);
        }
        out.optional("note", &step.note);
    }
    out.text
}
