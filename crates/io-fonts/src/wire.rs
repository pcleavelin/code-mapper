use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tag([u8; 4]);

const COLLECTION: Tag = Tag(*b"ttcf");
const NAMES: Tag = Tag(*b"name");
const POSTSCRIPT: Tag = Tag(*b"post");
const METRICS: Tag = Tag(*b"OS/2");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NameId(u16);

const FAMILY: NameId = NameId(1);
const STYLE: NameId = NameId(2);
const TYPOGRAPHIC_FAMILY: NameId = NameId(16);
const TYPOGRAPHIC_STYLE: NameId = NameId(17);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Platform(u16);

const UNICODE: Platform = Platform(0);
const MACINTOSH: Platform = Platform(1);
const WINDOWS: Platform = Platform(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LanguageId(u16);

const WINDOWS_ENGLISH: LanguageId = LanguageId(0x0409);
const MACINTOSH_ENGLISH: LanguageId = LanguageId(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ByteCount(u32);

const OFFSET_TABLE: ByteCount = ByteCount(12);
const TABLE_RECORD: ByteCount = ByteCount(16);
const NAME_RECORD: ByteCount = ByteCount(12);
const NAME_HEADER: ByteCount = ByteCount(6);
const PANOSE_PROPORTION_AT: ByteCount = ByteCount(35);
const PANOSE_LATIN_TEXT: u8 = 2;
const PANOSE_MONOSPACED: u8 = 9;
const FIXED_PITCH_AT: ByteCount = ByteCount(12);
const LARGEST_NAME_TABLE: ByteCount = ByteCount(1 << 20);
const MOST_FACES: ByteCount = ByteCount(64);
const MOST_TABLES: ByteCount = ByteCount(256);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WireFace {
    pub(crate) family: String,
    pub(crate) regular: bool,
    pub(crate) monospaced: bool,
    pub(crate) index: u32,
}

fn read_at(file: &mut File, offset: u32, length: ByteCount) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(u64::from(offset))).ok()?;
    let mut bytes = vec![0; usize::try_from(length.0).ok()?];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

fn u16_at(bytes: &[u8], at: u32) -> Option<u16> {
    let at = usize::try_from(at).ok()?;
    let pair = bytes.get(at..at + 2)?;
    Some(u16::from_be_bytes(pair.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: u32) -> Option<u32> {
    let at = usize::try_from(at).ok()?;
    let quad = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes(quad.try_into().ok()?))
}

fn tag_at(bytes: &[u8], at: u32) -> Option<Tag> {
    let at = usize::try_from(at).ok()?;
    let quad = bytes.get(at..at + 4)?;
    Some(Tag(quad.try_into().ok()?))
}

pub(crate) fn faces(file: &mut File) -> Vec<WireFace> {
    let Some(head) = read_at(file, 0, OFFSET_TABLE) else {
        return Vec::new();
    };
    if tag_at(&head, 0) != Some(COLLECTION) {
        return face(file, 0, 0).into_iter().collect();
    }
    let count = u32_at(&head, 8).unwrap_or(0).min(MOST_FACES.0);
    let Some(offsets) = read_at(file, OFFSET_TABLE.0, ByteCount(count * 4)) else {
        return Vec::new();
    };
    (0..count)
        .filter_map(|index| {
            let offset = u32_at(&offsets, index * 4)?;
            face(file, offset, index)
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
struct TableRecord {
    offset: u32,
    length: ByteCount,
}

#[derive(Clone, Copy, Debug, Default)]
struct Tables {
    names: Option<TableRecord>,
    post: Option<TableRecord>,
    metrics: Option<TableRecord>,
}

fn tables(file: &mut File, start: u32) -> Option<Tables> {
    let head = read_at(file, start, OFFSET_TABLE)?;
    let count = u32::from(u16_at(&head, 4)?).min(MOST_TABLES.0);
    let records = read_at(
        file,
        start.checked_add(OFFSET_TABLE.0)?,
        ByteCount(count * TABLE_RECORD.0),
    )?;
    let mut tables = Tables::default();
    for slot in 0..count {
        let at = slot * TABLE_RECORD.0;
        let record = Some(TableRecord {
            offset: u32_at(&records, at + 8)?,
            length: ByteCount(u32_at(&records, at + 12)?),
        });
        match tag_at(&records, at)? {
            tag if tag == NAMES => tables.names = record,
            tag if tag == POSTSCRIPT => tables.post = record,
            tag if tag == METRICS => tables.metrics = record,
            _ => {}
        }
    }
    Some(tables)
}

fn byte_in(file: &mut File, table: Option<TableRecord>, at: ByteCount) -> Option<Vec<u8>> {
    table
        .filter(|table| table.length > at)
        .and_then(|table| read_at(file, table.offset, ByteCount(at.0 + 4)))
}

fn face(file: &mut File, start: u32, index: u32) -> Option<WireFace> {
    let tables = tables(file, start)?;
    let names = tables
        .names
        .filter(|names| names.length <= LARGEST_NAME_TABLE)?;
    let names = read_at(file, names.offset, names.length)?;
    let fixed_pitch = byte_in(file, tables.post, FIXED_PITCH_AT)
        .and_then(|header| u32_at(&header, FIXED_PITCH_AT.0))
        .is_some_and(|fixed| fixed != 0);
    let panose_monospaced = byte_in(file, tables.metrics, PANOSE_PROPORTION_AT)
        .and_then(|header| {
            let at = usize::try_from(PANOSE_PROPORTION_AT.0).ok()?;
            Some((header.get(at - 3).copied()?, header.get(at).copied()?))
        })
        .is_some_and(|(kind, proportion)| {
            kind == PANOSE_LATIN_TEXT && proportion == PANOSE_MONOSPACED
        });
    let family = name(&names, TYPOGRAPHIC_FAMILY).or_else(|| name(&names, FAMILY))?;
    let regular = name(&names, TYPOGRAPHIC_STYLE)
        .or_else(|| name(&names, STYLE))
        .is_some_and(|style| regular(&style));
    Some(WireFace {
        family,
        regular,
        monospaced: fixed_pitch || panose_monospaced,
        index,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Preference {
    English,
    Other,
}

fn name(table: &[u8], wanted: NameId) -> Option<String> {
    let count = u32::from(u16_at(table, 2)?);
    let storage = u32::from(u16_at(table, 4)?);
    (0..count)
        .filter_map(|slot| {
            let at = NAME_HEADER.0 + slot * NAME_RECORD.0;
            let platform = Platform(u16_at(table, at)?);
            let language = LanguageId(u16_at(table, at + 4)?);
            if NameId(u16_at(table, at + 6)?) != wanted {
                return None;
            }
            let length = usize::from(u16_at(table, at + 8)?);
            let start = usize::try_from(storage + u32::from(u16_at(table, at + 10)?)).ok()?;
            let raw = table.get(start..start + length)?;
            let (text, preference) = match platform {
                UNICODE => (utf16(raw)?, Preference::Other),
                WINDOWS => (
                    utf16(raw)?,
                    if language == WINDOWS_ENGLISH {
                        Preference::English
                    } else {
                        Preference::Other
                    },
                ),
                MACINTOSH if language == MACINTOSH_ENGLISH => (ascii(raw)?, Preference::Other),
                _ => return None,
            };
            (!text.trim().is_empty()).then_some((preference, text))
        })
        .min_by_key(|(preference, _)| *preference)
        .map(|(_, text)| text)
}

fn utf16(raw: &[u8]) -> Option<String> {
    let units = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect::<Vec<u16>>();
    String::from_utf16(&units).ok()
}

fn ascii(raw: &[u8]) -> Option<String> {
    raw.is_ascii()
        .then(|| raw.iter().copied().map(char::from).collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RegularName {
    Regular,
    Book,
    Roman,
    Normal,
}

const REGULAR_NAMES: [RegularName; 4] = [
    RegularName::Regular,
    RegularName::Book,
    RegularName::Roman,
    RegularName::Normal,
];

impl RegularName {
    const fn name(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Book => "book",
            Self::Roman => "roman",
            Self::Normal => "normal",
        }
    }
}

fn regular(style: &str) -> bool {
    let style = style.trim().to_lowercase();
    REGULAR_NAMES.iter().any(|name| name.name() == style)
}
