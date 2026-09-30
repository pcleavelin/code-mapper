use domain::{Author, TourKind};
use strum::VariantArray;

use crate::error::{Fault, FieldKey, FieldValue};

pub(crate) const VERSION: &str = "codemap 9";

const VERSION_KEY: &str = "codemap";

const VERSION_VALUE: &str = "9";

pub(crate) const MAP_EXTENSION: &str = "cmap";

pub(crate) const MAP_DIRECTORY: &str = ".codemap";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyName(&'static str);

impl KeyName {
    pub(crate) const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
enum ConflictMark {
    Begin,
    Middle,
    End,
    Diff,
    Added,
}

impl ConflictMark {
    const fn name(self) -> KeyName {
        KeyName(match self {
            Self::Begin => "<<<<<<<",
            Self::Middle => "=======",
            Self::End => ">>>>>>>",
            Self::Diff => "%%%%%%%",
            Self::Added => "+++++++",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineKey {
    Step,
}

impl LineKey {
    const fn name(self) -> KeyName {
        KeyName(match self {
            Self::Step => "step",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, VariantArray)]
pub(crate) enum TourKey {
    Tour,
    Kind,
    Author,
    Group,
    Note,
}

impl TourKey {
    const fn name(self) -> KeyName {
        KeyName(match self {
            Self::Tour => "tour",
            Self::Kind => "kind",
            Self::Author => "author",
            Self::Group => "group",
            Self::Note => "note",
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
pub(crate) enum StepKey {
    Parent,
    Order,
    Author,
    File,
    Symbol,
    Lines,
    Hash,
    Link,
    Note,
}

impl StepKey {
    pub(crate) const fn name(self) -> KeyName {
        KeyName(match self {
            Self::Parent => "parent",
            Self::Order => "order",
            Self::Author => "author",
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::Lines => "lines",
            Self::Hash => "hash",
            Self::Link => "link",
            Self::Note => "note",
        })
    }

    fn named(key: &str) -> Option<Self> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|entry| entry.name().as_str() == key)
    }
}

pub(crate) struct CmapTour {
    pub(crate) name: Option<(String, u32)>,
    pub(crate) kind: TourKind,
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

fn kind_name(kind: TourKind) -> KeyName {
    KeyName(match kind {
        TourKind::Flow => "flow",
        TourKind::Layer => "layer",
        TourKind::Data => "data",
    })
}

fn parse_kind(value: &str) -> Option<TourKind> {
    TourKind::VARIANTS
        .iter()
        .copied()
        .find(|kind| kind_name(*kind).as_str() == value)
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

pub(crate) fn parse(text: &str) -> Result<Vec<CmapTour>, WireFault> {
    let mut tours: Vec<CmapTour> = Vec::new();
    let mut in_step = false;
    for (line, raw_line) in (0u32..).zip(text.lines()) {
        let at = |fault: Fault| WireFault { line, fault };
        if ConflictMark::VARIANTS
            .iter()
            .any(|mark| raw_line.starts_with(mark.name().as_str()))
        {
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
            tours.push(CmapTour {
                name: None,
                kind: TourKind::Flow,
                author: Author::Agent,
                group: String::new(),
                note: String::new(),
                steps: Vec::new(),
            });
            in_step = false;
            continue;
        }
        let Some(tour) = tours.last_mut() else {
            return Err(at(Fault::NoVersion));
        };
        let author = |name: &str| {
            parse_author(name).ok_or_else(|| at(Fault::UnknownAuthor(FieldValue::new(name))))
        };
        if key == LineKey::Step.name().as_str() {
            if tour.steps.iter().any(|step| step.id == value) {
                return Err(at(Fault::SecondStep(FieldValue::new(&value))));
            }
            tour.steps.push(CmapStep {
                id: value,
                line,
                order: None,
                parent: None,
                author: tour.author,
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
            match TourKey::named(key) {
                Some(TourKey::Tour) => tour.name = Some((value, line)),
                Some(TourKey::Kind) => {
                    tour.kind = parse_kind(&value)
                        .ok_or_else(|| at(Fault::UnknownKind(FieldValue::new(&value))))?;
                }
                Some(TourKey::Author) => tour.author = author(&value)?,
                Some(TourKey::Group) => tour.group = value,
                Some(TourKey::Note) => tour.note = value,
                None => return Err(at(Fault::UnknownTourField(FieldKey::new(key)))),
            }
            continue;
        }
        let Some(step) = tour.steps.last_mut() else {
            return Err(at(Fault::UnknownTourField(FieldKey::new(key))));
        };
        match StepKey::named(key) {
            Some(StepKey::Parent) => step.parent = Some((value, line)),
            Some(StepKey::Order) => {
                step.order = Some(parse_order(&value).ok_or_else(|| at(Fault::Order))?);
            }
            Some(StepKey::Author) => step.author = author(&value)?,
            Some(StepKey::File) => step.file = Some(value),
            Some(StepKey::Symbol) => step.symbol = value,
            Some(StepKey::Lines) => {
                step.lines = Some(parse_lines(&value).ok_or_else(|| at(Fault::Lines))?);
            }
            Some(StepKey::Hash) => {
                step.hash = Some(parse_hash(&value).ok_or_else(|| at(Fault::Hash))?);
            }
            Some(StepKey::Link) => step.link = Some((value, line)),
            Some(StepKey::Note) => step.note = value,
            None => return Err(at(Fault::UnknownStepField(FieldKey::new(key)))),
        }
    }
    Ok(tours)
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

pub(crate) fn render(tour: &CmapTour) -> String {
    let mut out = CmapText {
        text: String::new(),
    };
    out.field(VERSION_KEY, VERSION_VALUE);
    let name = tour.name.as_ref().map_or("", |name| name.0.as_str());
    out.field(TourKey::Tour.name().as_str(), name);
    out.field(TourKey::Kind.name().as_str(), kind_name(tour.kind).as_str());
    out.field(
        TourKey::Author.name().as_str(),
        author_name(tour.author).as_str(),
    );
    out.optional(TourKey::Group.name().as_str(), &tour.group);
    out.optional(TourKey::Note.name().as_str(), &tour.note);
    let mut by_id: Vec<&CmapStep> = tour.steps.iter().collect();
    by_id.sort_by(|one, other| one.id.cmp(&other.id));
    for step in by_id {
        out.text.push('\n');
        out.field(LineKey::Step.name().as_str(), &step.id);
        out.field(
            StepKey::Order.name().as_str(),
            &step.order.unwrap_or_default().to_string(),
        );
        if let Some((parent, _)) = &step.parent {
            out.field(StepKey::Parent.name().as_str(), parent);
        }
        out.field(
            StepKey::Author.name().as_str(),
            author_name(step.author).as_str(),
        );
        out.field(
            StepKey::File.name().as_str(),
            step.file.as_deref().unwrap_or_default(),
        );
        out.optional(StepKey::Symbol.name().as_str(), &step.symbol);
        let (start, end) = step.lines.unwrap_or_default();
        out.field(StepKey::Lines.name().as_str(), &format!("{start} {end}"));
        out.field(
            StepKey::Hash.name().as_str(),
            &format!("{:016x}", step.hash.unwrap_or_default()),
        );
        if let Some((link, _)) = &step.link {
            out.optional(StepKey::Link.name().as_str(), link);
        }
        out.optional(StepKey::Note.name().as_str(), &step.note);
    }
    out.text
}
