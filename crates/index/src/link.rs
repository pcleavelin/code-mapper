use std::collections::BTreeMap;

use domain::{
    Call, Edge, FileId, Index, Qualifier, RelativePath, SourceFile, Symbol, SymbolId, SymbolName,
    TypeName,
};

pub fn link(index: &mut Index) {
    let edges = edges(index);
    index.connect(&edges);
}

fn edges(index: &Index) -> Vec<Edge> {
    let mut named: BTreeMap<SymbolName, Vec<SymbolId>> = BTreeMap::new();
    for id in index.symbol_ids() {
        if let Some(symbol) = index.symbol(id) {
            named.entry(symbol.name().clone()).or_default().push(id);
        }
    }
    let files: BTreeMap<&RelativePath, FileId> = index
        .file_entries()
        .map(|entry| (entry.item.path(), entry.id))
        .collect();
    let resolver = Resolver {
        index,
        named: &named,
    };
    let mut edges = Vec::new();
    for from in index.symbol_ids() {
        let Some(symbol) = index.symbol(from) else {
            continue;
        };
        for call in symbol.calls() {
            if let Some(to) = resolver.resolve(from, call) {
                edges.push(Edge { from, to });
            }
        }
        for target in symbol.targets() {
            let Some(file) = files.get(&target.file) else {
                continue;
            };
            if let Some(to) = index.symbol_at(*file, target.line) {
                edges.push(Edge { from, to });
            }
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SuffixName(&'static str);

impl SuffixName {
    const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Suffix {
    Source,
    Header,
}

impl Suffix {
    const ALL: [Self; 2] = [Self::Source, Self::Header];

    fn name(self) -> SuffixName {
        SuffixName(match self {
            Self::Source => ".c",
            Self::Header => ".h",
        })
    }

    fn matches(path: &RelativePath) -> bool {
        Self::ALL
            .into_iter()
            .any(|suffix| path.ends_with(suffix.name().as_str()))
    }
}

#[derive(Clone, Copy)]
struct Word<'text>(&'text str);

impl Word<'_> {
    fn owns(self, symbol: &Symbol) -> bool {
        symbol.owner().map(TypeName::as_str) == Some(self.0)
    }

    fn names_module_of(self, path: &RelativePath) -> bool {
        path.stem() == self.0 || self.names_directory_of(path)
    }

    fn names_directory_of(self, path: &RelativePath) -> bool {
        path.directory().unwrap_or_default() == self.0
    }

    fn starts_lowercase(self) -> bool {
        self.0
            .chars()
            .next()
            .is_some_and(|character| character.is_lowercase() || character == '_')
    }

    fn directory(path: &RelativePath) -> Word<'_> {
        Word(path.directory().unwrap_or_default())
    }
}

struct Resolver<'index> {
    index: &'index Index,
    named: &'index BTreeMap<SymbolName, Vec<SymbolId>>,
}

impl Resolver<'_> {
    fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.index.symbol(id)
    }

    fn file(&self, id: SymbolId) -> Option<&SourceFile> {
        self.index.file(id.file())
    }

    fn owned_by(&self, id: SymbolId, owner: Word<'_>) -> bool {
        self.symbol(id).is_some_and(|symbol| owner.owns(symbol))
    }

    fn in_module(&self, id: SymbolId, module: Word<'_>) -> bool {
        self.file(id)
            .is_some_and(|file| module.names_module_of(file.path()))
    }

    fn member(&self, id: SymbolId) -> bool {
        self.symbol(id)
            .is_some_and(|symbol| symbol.owner().is_some())
    }

    fn free(&self, id: SymbolId) -> bool {
        self.symbol(id)
            .is_some_and(|symbol| symbol.owner().is_none())
    }

    fn resolve(&self, from: SymbolId, call: &Call) -> Option<SymbolId> {
        let candidates: Vec<SymbolId> = self
            .named
            .get(&call.name)?
            .iter()
            .copied()
            .filter(|id| {
                self.symbol(*id).is_some_and(|symbol| {
                    !["struct", "enum", "union"]
                        .iter()
                        .any(|kind| symbol.kind().contains(kind))
                })
            })
            .collect();
        let file = self.file(from)?;
        let owner = self.symbol(from)?.owner().map(|owner| Word(owner.as_str()));
        let pick = |chosen: &mut dyn Iterator<Item = &SymbolId>| -> Option<SymbolId> {
            let chosen: Vec<SymbolId> = chosen.copied().collect();
            chosen
                .iter()
                .find(|id| id.file() == from.file())
                .or(chosen.first())
                .copied()
        };
        let same_file = |id: &&SymbolId| id.file() == from.file();
        match &call.qualifier {
            Qualifier::SelfReference => owner
                .and_then(|owner| {
                    pick(&mut candidates.iter().filter(|id| self.owned_by(**id, owner)))
                })
                .or_else(|| pick(&mut candidates.iter().filter(same_file)))
                .or_else(|| pick(&mut candidates.iter())),
            Qualifier::Named(scope) => {
                let qualifier = Word(scope.as_str());
                pick(
                    &mut candidates
                        .iter()
                        .filter(|id| self.owned_by(**id, qualifier)),
                )
                .or_else(|| {
                    pick(
                        &mut candidates
                            .iter()
                            .filter(|id| self.in_module(**id, qualifier)),
                    )
                })
                .or_else(|| {
                    if let Some(module) = file.imports().get(scope.as_str()) {
                        return pick(
                            &mut candidates
                                .iter()
                                .filter(|id| self.in_module(**id, Word(module))),
                        );
                    }
                    if !qualifier.starts_lowercase() {
                        return None;
                    }
                    owner
                        .and_then(|owner| {
                            pick(&mut candidates.iter().filter(|id| self.owned_by(**id, owner)))
                        })
                        .or_else(|| pick(&mut candidates.iter().filter(|id| self.member(**id))))
                })
            }
            Qualifier::Plain => pick(
                &mut candidates
                    .iter()
                    .filter(same_file)
                    .filter(|id| self.free(**id)),
            )
            .or_else(|| {
                file.imports().get(call.name.as_str()).and_then(|module| {
                    pick(
                        &mut candidates
                            .iter()
                            .filter(|id| self.in_module(**id, Word(module))),
                    )
                })
            })
            .or_else(|| {
                let directory = Word::directory(file.path());
                pick(
                    &mut candidates.iter().filter(|id| self.free(**id)).filter(|id| {
                        self.file(**id)
                            .is_some_and(|other| directory.names_directory_of(other.path()))
                    }),
                )
            })
            .or_else(|| {
                let clang = Suffix::matches(file.path());
                if clang {
                    pick(&mut candidates.iter().filter(|id| self.free(**id)))
                        .or_else(|| pick(&mut candidates.iter()))
                } else {
                    None
                }
            }),
        }
    }
}
