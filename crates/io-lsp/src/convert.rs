use std::path::{Path, PathBuf};

use domain::{Line, Location, RelativePath, SymbolName};

use crate::answer::{Character, Definition, Outline, OutlineKind, Position, RangeEnd};
use crate::wire::{WireDefinition, WireLocation, WireName, WireSymbol};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UriPrefix {
    File,
}

impl UriPrefix {
    fn name(self) -> WireName {
        WireName::new(match self {
            Self::File => "file://",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Uri(String);

impl Uri {
    pub(crate) fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub(crate) fn of(path: &Path) -> Self {
        let text = path.to_string_lossy().replace('\\', "/");
        let mut uri = String::from("file:///");
        for byte in text.trim_start_matches('/').bytes() {
            match byte {
                b'A'..=b'Z'
                | b'a'..=b'z'
                | b'0'..=b'9'
                | b'-'
                | b'.'
                | b'_'
                | b'~'
                | b'/'
                | b':' => uri.push(char::from(byte)),
                _ => {
                    uri.push('%');
                    for nibble in [byte >> 4, byte & 0x0f] {
                        uri.extend(
                            char::from_digit(u32::from(nibble), 16)
                                .map(|digit| digit.to_ascii_uppercase()),
                        );
                    }
                }
            }
        }
        Self(uri)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    fn decoded(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut at = 0;
        while let Some(byte) = bytes.get(at).copied() {
            if byte == b'%'
                && at + 2 < bytes.len()
                && let Some(value) = text
                    .get(at + 1..at + 3)
                    .and_then(|digits| u8::from_str_radix(digits, 16).ok())
            {
                out.push(value);
                at += 3;
                continue;
            }
            out.push(byte);
            at += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    pub(crate) fn path(&self) -> Option<PathBuf> {
        let decoded = Self::decoded(self.0.strip_prefix(UriPrefix::File.name().as_str())?);
        Some(PathBuf::from(if cfg!(windows) {
            decoded.trim_start_matches('/').to_owned()
        } else {
            decoded
        }))
    }
}

pub(crate) fn relative(path: &Path, root: &Path) -> Option<RelativePath> {
    let full = path.to_string_lossy().replace('\\', "/");
    let whole = root.to_string_lossy().replace('\\', "/");
    let base = whole.trim_end_matches('/');
    let (compared, prefix) = if cfg!(windows) {
        (full.to_ascii_lowercase(), base.to_ascii_lowercase())
    } else {
        (full.clone(), base.to_owned())
    };
    let tail = compared.strip_prefix(&prefix)?.strip_prefix('/')?;
    full.get(full.len() - tail.len()..).map(RelativePath::new)
}

pub(crate) fn outline(symbol: WireSymbol) -> Outline {
    let line = |value: u64| Line::new(u32::try_from(value).unwrap_or_default());
    let character = |value: u64| Character::new(u32::try_from(value).unwrap_or_default());
    Outline {
        name: SymbolName::new(&symbol.name),
        kind: OutlineKind::new(symbol.kind),
        selection: Position {
            line: line(symbol.selection_line),
            character: character(symbol.selection_character),
        },
        end: line(symbol.end_line),
        range_end: if symbol.end_at_line_start {
            RangeEnd::LineStart
        } else {
            RangeEnd::Inside
        },
        children: symbol.children.into_iter().map(outline).collect(),
    }
}

pub(crate) fn locations(found: Vec<WireLocation>, root: &Path) -> Vec<Location> {
    let mut out: Vec<Location> = found
        .into_iter()
        .filter_map(|entry| {
            Some(Location {
                file: relative(&Uri::new(&entry.uri).path()?, root)?,
                line: Line::new(u32::try_from(entry.line).ok()?),
            })
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

pub(crate) fn definition(found: &WireDefinition, root: &Path) -> Option<Definition> {
    let path = Uri::new(&found.uri).path()?;
    Some(Definition {
        file: relative(&path, root),
        path,
        line: Line::new(u32::try_from(found.line).ok()?),
        character: Character::new(u32::try_from(found.character).ok()?),
    })
}
