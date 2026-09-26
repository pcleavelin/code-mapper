const MAGIC: &[u8; 4] = b"CMCH";
const VERSION: u32 = 1;
const PLAIN_TAG: u8 = 0;
const SELF_TAG: u8 = 1;
const NAMED_TAG: u8 = 2;
pub(crate) const SERVER_BACKEND: u8 = 1;
pub(crate) const TREE_SITTER_BACKEND: u8 = 0;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CacheFile {
    pub(crate) files: Vec<CachedFile>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CachedFile {
    pub(crate) path: String,
    pub(crate) hash: u64,
    pub(crate) backend: u8,
    pub(crate) symbols: Vec<CachedSymbol>,
    pub(crate) imports: Vec<(String, String)>,
    pub(crate) highlights: Vec<Vec<CachedHighlight>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CachedSymbol {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) depth: u8,
    pub(crate) owner: String,
    pub(crate) calls: Vec<CachedCall>,
    pub(crate) targets: Vec<CachedLocation>,
    pub(crate) references: Vec<CachedLocation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CachedCall {
    pub(crate) name: String,
    pub(crate) qualifier: CachedQualifier,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CachedQualifier {
    Plain,
    SelfReference,
    Named(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CachedLocation {
    pub(crate) file: String,
    pub(crate) line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CachedHighlight {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) class: u8,
}

pub(crate) struct Reader<'bytes> {
    bytes: &'bytes [u8],
    at: usize,
}

impl<'bytes> Reader<'bytes> {
    pub(crate) fn new(bytes: &'bytes [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, count: usize) -> Option<&'bytes [u8]> {
        let taken = self.bytes.get(self.at..self.at.checked_add(count)?)?;
        self.at += count;
        Some(taken)
    }

    fn byte(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    fn number(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn wide(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn text(&mut self) -> Option<String> {
        let count = usize::try_from(self.number()?).ok()?;
        Some(String::from_utf8_lossy(self.take(count)?).into_owned())
    }

    fn list<Item>(&mut self, mut item: impl FnMut(&mut Self) -> Option<Item>) -> Option<Vec<Item>> {
        (0..self.number()?).map(|_| item(self)).collect()
    }

    pub(crate) fn cache(&mut self) -> Option<CacheFile> {
        if self.take(MAGIC.len())? != MAGIC || self.number()? != VERSION {
            return None;
        }
        Some(CacheFile {
            files: self.list(Self::file)?,
        })
    }

    fn file(&mut self) -> Option<CachedFile> {
        Some(CachedFile {
            path: self.text()?,
            hash: self.wide()?,
            backend: self.byte()?,
            symbols: self.list(Self::symbol)?,
            imports: self.list(Self::import)?,
            highlights: self.list(|reader| reader.list(Self::highlight))?,
        })
    }

    fn symbol(&mut self) -> Option<CachedSymbol> {
        Some(CachedSymbol {
            name: self.text()?,
            kind: self.text()?,
            start: self.number()?,
            end: self.number()?,
            depth: self.byte()?,
            owner: self.text()?,
            calls: self.list(Self::call)?,
            targets: self.list(Self::location)?,
            references: self.list(Self::location)?,
        })
    }

    fn call(&mut self) -> Option<CachedCall> {
        let name = self.text()?;
        let qualifier = match self.byte()? {
            PLAIN_TAG => CachedQualifier::Plain,
            SELF_TAG => CachedQualifier::SelfReference,
            _ => CachedQualifier::Named(self.text()?),
        };
        Some(CachedCall { name, qualifier })
    }

    fn location(&mut self) -> Option<CachedLocation> {
        Some(CachedLocation {
            file: self.text()?,
            line: self.number()?,
        })
    }

    fn import(&mut self) -> Option<(String, String)> {
        Some((self.text()?, self.text()?))
    }

    fn highlight(&mut self) -> Option<CachedHighlight> {
        Some(CachedHighlight {
            start: self.number()?,
            end: self.number()?,
            class: self.byte()?,
        })
    }
}

#[derive(Default)]
pub(crate) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(crate) fn bytes(self) -> Vec<u8> {
        self.bytes
    }

    fn take(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    fn byte(&mut self, byte: u8) {
        self.bytes.push(byte);
    }

    fn number(&mut self, number: u32) {
        self.take(&number.to_le_bytes());
    }

    fn wide(&mut self, wide: u64) {
        self.take(&wide.to_le_bytes());
    }

    fn text(&mut self, text: &str) {
        self.number(u32::try_from(text.len()).unwrap_or_default());
        self.take(text.as_bytes());
    }

    fn list<Item>(&mut self, items: &[Item], mut item: impl FnMut(&mut Self, &Item)) {
        self.number(u32::try_from(items.len()).unwrap_or_default());
        for each in items {
            item(self, each);
        }
    }

    pub(crate) fn cache(&mut self, cache: &CacheFile) {
        self.take(MAGIC);
        self.number(VERSION);
        self.list(&cache.files, Self::file);
    }

    fn file(&mut self, file: &CachedFile) {
        self.text(&file.path);
        self.wide(file.hash);
        self.byte(file.backend);
        self.list(&file.symbols, Self::symbol);
        self.list(&file.imports, Self::import);
        self.list(&file.highlights, |writer, line| {
            writer.list(line, Self::highlight);
        });
    }

    fn symbol(&mut self, symbol: &CachedSymbol) {
        self.text(&symbol.name);
        self.text(&symbol.kind);
        self.number(symbol.start);
        self.number(symbol.end);
        self.byte(symbol.depth);
        self.text(&symbol.owner);
        self.list(&symbol.calls, Self::call);
        self.list(&symbol.targets, Self::location);
        self.list(&symbol.references, Self::location);
    }

    fn call(&mut self, call: &CachedCall) {
        self.text(&call.name);
        match &call.qualifier {
            CachedQualifier::Plain => self.byte(PLAIN_TAG),
            CachedQualifier::SelfReference => self.byte(SELF_TAG),
            CachedQualifier::Named(scope) => {
                self.byte(NAMED_TAG);
                self.text(scope);
            }
        }
    }

    fn location(&mut self, location: &CachedLocation) {
        self.text(&location.file);
        self.number(location.line);
    }

    fn import(&mut self, import: &(String, String)) {
        self.text(&import.0);
        self.text(&import.1);
    }

    fn highlight(&mut self, highlight: &CachedHighlight) {
        self.number(highlight.start);
        self.number(highlight.end);
        self.byte(highlight.class);
    }
}
