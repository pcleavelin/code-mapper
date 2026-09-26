use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::mem;
use std::time::Duration;

use domain::{
    Backend, Depth, FileId, Index, Language, Location, Program, Readiness, RelativePath, Root,
    SourceLine, Span, Symbol, SymbolId, SymbolKind, SymbolName, TextHash, TypeName,
};
use io_lsp::{Character, DocumentPosition, LspSession, Outline, OutlineKind, RangeEnd, StartError};

use crate::build::save_cache;
use crate::link::link;
use crate::parse::TypeText;

const READY_WAIT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerFile {
    pub path: RelativePath,
    pub hash: TextHash,
    pub symbols: Vec<Symbol>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileVersion {
    pub path: RelativePath,
    pub hash: TextHash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileCount(usize);

impl FileCount {
    pub const fn new(count: usize) -> Self {
        Self(count)
    }

    pub const fn value(self) -> usize {
        self.0
    }
}

impl fmt::Display for FileCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Debug)]
pub enum ServerNotice {
    ToIndex { program: Program, files: FileCount },
    Unavailable(StartError),
}

impl fmt::Display for ServerNotice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ToIndex { program, files } => {
                write!(formatter, "{program}: {files} files to index")
            }
            Self::Unavailable(error) => {
                write!(
                    formatter,
                    "{error}: its files keep the tree-sitter resolver"
                )
            }
        }
    }
}

pub fn start_session(root: &Root, language: Language) -> Result<LspSession, StartError> {
    let mut session = LspSession::start(language, root.as_path())?;
    session.wait_ready(READY_WAIT);
    Ok(session)
}

pub fn versions(index: &Index, files: &[FileId]) -> Vec<FileVersion> {
    files
        .iter()
        .filter_map(|file| index.file(*file))
        .map(|file| FileVersion {
            path: file.path().clone(),
            hash: file.hash(),
        })
        .collect()
}

pub fn apply(index: &mut Index, answer: ServerFile) {
    let Some(file) = index
        .find_file(&answer.path)
        .and_then(|file| index.file_mut(file))
    else {
        return;
    };
    if file.hash() != answer.hash {
        return;
    }
    file.set_symbols(answer.symbols);
    file.set_backend(Backend::Server);
    file.set_readiness(Readiness::Ready);
}

fn kind_name(kind: OutlineKind, name: &SymbolName) -> SymbolKind {
    let names = [
        "symbol",
        "file",
        "module",
        "namespace",
        "package",
        "class",
        "method",
        "property",
        "field",
        "constructor",
        "enum",
        "interface",
        "function",
        "variable",
        "constant",
        "string",
        "number",
        "boolean",
        "array",
        "object",
        "key",
        "null",
        "enum-member",
        "struct",
        "event",
        "operator",
        "type",
    ];
    if kind.value() == 19 && name.as_str().starts_with("impl") {
        return SymbolKind::new("impl");
    }
    let spelled = usize::try_from(kind.value())
        .ok()
        .and_then(|position| names.get(position))
        .copied()
        .unwrap_or("symbol");
    SymbolKind::new(spelled)
}

fn impl_type(name: &SymbolName) -> TypeName {
    let implemented = name
        .as_str()
        .rsplit(" for ")
        .next()
        .unwrap_or(name.as_str())
        .trim_start_matches("impl")
        .trim();
    TypeName::new(&TypeText::new(implemented).bare())
}

struct Outlined {
    symbols: Vec<Symbol>,
    positions: Vec<DocumentPosition>,
}

impl Outlined {
    fn push(
        &mut self,
        file: &RelativePath,
        outline: &Outline,
        depth: Depth,
        owner: Option<&TypeName>,
    ) {
        if outline.name.as_str().is_empty() {
            return;
        }
        let kind = outline.kind.value();
        let start = outline.selection.line;
        let mut end = outline.end.max(start);
        if outline.range_end == RangeEnd::LineStart && end > start {
            end = end.previous().unwrap_or(start);
        }
        let Some(span) = Span::new(start, end) else {
            return;
        };
        let container = depth == Depth::default() && [2, 3, 5, 11, 19].contains(&kind);
        let own = if container {
            Some(if kind == 19 {
                impl_type(&outline.name)
            } else {
                TypeName::new(outline.name.as_str())
            })
        } else {
            owner.cloned()
        };
        self.positions.push(DocumentPosition {
            file: file.clone(),
            line: outline.selection.line,
            character: outline.selection.character,
        });
        self.symbols.push(Symbol::new(
            outline.name.clone(),
            kind_name(outline.kind, &outline.name),
            span,
            depth,
            own.clone(),
            Vec::new(),
        ));
        if container {
            for child in &outline.children {
                self.push(file, child, Depth::default().deeper(), own.as_ref());
            }
        }
    }
}

pub fn index_files(session: &mut LspSession, files: &[FileVersion]) -> Vec<ServerFile> {
    let paths: Vec<RelativePath> = files.iter().map(|file| file.path.clone()).collect();
    let mut outlined: Vec<Outlined> = session
        .outlines(&paths)
        .into_iter()
        .zip(&paths)
        .map(|(outlines, path)| {
            let mut outlined = Outlined {
                symbols: Vec::new(),
                positions: Vec::new(),
            };
            for outline in &outlines {
                outlined.push(path, outline, Depth::default(), None);
            }
            outlined
        })
        .collect();
    let mut position_owners = Vec::new();
    let mut positions = Vec::new();
    for (file, each) in outlined.iter().enumerate() {
        for (symbol, position) in each.positions.iter().enumerate() {
            position_owners.push((file, symbol));
            positions.push(position.clone());
        }
    }
    let items = session.call_items(&positions);
    let mut item_owners = Vec::new();
    let mut asked = Vec::new();
    for (owner, found) in position_owners.into_iter().zip(items) {
        for item in found {
            item_owners.push(owner);
            asked.push(item);
        }
    }
    let mut targets: BTreeMap<(usize, usize), Vec<Location>> = BTreeMap::new();
    for (owner, locations) in item_owners
        .into_iter()
        .zip(session.outgoing_targets(&asked))
    {
        targets.entry(owner).or_default().extend(locations);
    }
    for (file, each) in outlined.iter_mut().enumerate() {
        for (symbol, entry) in each.symbols.iter_mut().enumerate() {
            let mut found = targets.remove(&(file, symbol)).unwrap_or_default();
            found.sort();
            found.dedup();
            entry.set_targets(found);
        }
    }
    files
        .iter()
        .zip(outlined)
        .map(|(file, each)| ServerFile {
            path: file.path.clone(),
            hash: file.hash,
            symbols: each.symbols,
        })
        .collect()
}

pub fn name_position(index: &Index, id: SymbolId) -> Option<DocumentPosition> {
    let file = index.file(id.file())?;
    let symbol = index.symbol(id)?;
    let start = symbol.span().start();
    let line = file
        .text()
        .line(start)
        .map(SourceLine::as_str)
        .unwrap_or_default();
    let character = line
        .find(symbol.name().as_str())
        .and_then(|found| line.get(..found))
        .map_or(0, |head| head.encode_utf16().count());
    Some(DocumentPosition {
        file: file.path().clone(),
        line: start,
        character: Character::new(u32::try_from(character).unwrap_or_default()),
    })
}

pub struct Servers {
    root: Root,
    live: BTreeMap<Language, Option<LspSession>>,
    report: Box<dyn FnMut(&ServerNotice)>,
}

impl Servers {
    pub fn new(root: &Root, report: impl FnMut(&ServerNotice) + 'static) -> Self {
        Self {
            root: root.clone(),
            live: BTreeMap::new(),
            report: Box::new(report),
        }
    }

    fn session(&mut self, language: Language) -> Option<&mut LspSession> {
        if !self.live.contains_key(&language) {
            let started = start_session(&self.root, language);
            let session = match started {
                Ok(session) => Some(session),
                Err(error) => {
                    (self.report)(&ServerNotice::Unavailable(error));
                    None
                }
            };
            self.live.insert(language, session);
        }
        self.live.get_mut(&language).and_then(Option::as_mut)
    }

    pub fn index(&mut self, index: &mut Index, paths: &[RelativePath]) {
        let wanted: BTreeSet<&RelativePath> = paths.iter().collect();
        let mut answered = false;
        for batch in index.pending() {
            let files: Vec<FileVersion> = versions(index, &batch.files)
                .into_iter()
                .filter(|file| wanted.contains(&file.path))
                .collect();
            if files.is_empty() {
                continue;
            }
            (self.report)(&ServerNotice::ToIndex {
                program: batch.language.program(),
                files: FileCount::new(files.len()),
            });
            let Some(session) = self.session(batch.language) else {
                index.give_up(batch.language);
                continue;
            };
            let mut done = Vec::new();
            for chunk in files.chunks(32) {
                done.extend(index_files(session, chunk));
            }
            for answer in done {
                apply(index, answer);
                answered = true;
            }
        }
        if answered {
            link(index);
            drop(save_cache(index));
        }
    }

    pub fn incoming_calls(&mut self, index: &Index, id: SymbolId) -> Option<Vec<Location>> {
        let language = index.file(id.file())?.language()?;
        let position = name_position(index, id)?;
        Some(self.session(language)?.incoming_callers(&position))
    }

    pub fn references(&mut self, index: &Index, id: SymbolId) -> Option<Vec<Location>> {
        let language = index.file(id.file())?.language()?;
        let position = name_position(index, id)?;
        Some(self.session(language)?.references(&position))
    }
}

impl Drop for Servers {
    fn drop(&mut self) {
        for session in mem::take(&mut self.live).into_values().flatten() {
            session.shutdown();
        }
    }
}
