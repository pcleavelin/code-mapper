use domain::{
    Backend, ByteOffset, Call, Depth, FileText, Highlight, HighlightClass, Imports, Index, Line,
    Location, Qualifier, RelativePath, Scope, SourceFile, Span, Symbol, SymbolKind, SymbolName,
    TextHash, TypeName,
};
use strum::VariantArray;

use crate::wire::{
    CacheFile, CachedCall, CachedFile, CachedHighlight, CachedLocation, CachedQualifier,
    CachedSymbol, SERVER_BACKEND, TREE_SITTER_BACKEND,
};

const LARGEST_DEPTH: Depth = Depth::new(255);

pub(crate) fn source_file(cached: &CachedFile) -> Option<SourceFile> {
    let symbols = cached
        .symbols
        .iter()
        .map(symbol)
        .collect::<Option<Vec<Symbol>>>()?;
    let mut imports = Imports::new();
    for import in &cached.imports {
        imports.insert(&import.0, &import.1);
    }
    let highlights = cached
        .highlights
        .iter()
        .map(|line| line.iter().map(highlight).collect())
        .collect();
    let backend = if cached.backend == SERVER_BACKEND {
        Backend::Server
    } else {
        Backend::TreeSitter
    };
    Some(SourceFile::new(
        RelativePath::new(&cached.path),
        FileText::default(),
        highlights,
        symbols,
        imports,
        TextHash::new(cached.hash),
        backend,
    ))
}

fn symbol(cached: &CachedSymbol) -> Option<Symbol> {
    let span = Span::new(Line::new(cached.start), Line::new(cached.end))?;
    let owner = (!cached.owner.is_empty()).then(|| TypeName::new(&cached.owner));
    let calls = cached.calls.iter().map(call).collect();
    let mut symbol = Symbol::new(
        SymbolName::new(&cached.name),
        SymbolKind::new(&cached.kind),
        span,
        Depth::new(u32::from(cached.depth)),
        owner,
        calls,
    );
    symbol.set_targets(cached.targets.iter().map(location).collect());
    symbol.set_references(cached.references.iter().map(location).collect());
    Some(symbol)
}

fn call(cached: &CachedCall) -> Call {
    Call {
        name: SymbolName::new(&cached.name),
        qualifier: match &cached.qualifier {
            CachedQualifier::Plain => Qualifier::Plain,
            CachedQualifier::SelfReference => Qualifier::SelfReference,
            CachedQualifier::Named(scope) => Qualifier::Named(Scope::new(scope)),
        },
    }
}

fn location(cached: &CachedLocation) -> Location {
    Location {
        file: RelativePath::new(&cached.file),
        line: Line::new(cached.line),
    }
}

fn highlight(cached: &CachedHighlight) -> Highlight {
    let class = HighlightClass::VARIANTS
        .iter()
        .copied()
        .zip(0..)
        .find(|pair| pair.1 == cached.class)
        .map_or(HighlightClass::Plain, |pair| pair.0);
    Highlight {
        start: ByteOffset::new(cached.start),
        end: ByteOffset::new(cached.end),
        class,
    }
}

pub(crate) fn cache_file(index: &Index) -> CacheFile {
    CacheFile {
        files: index.files().map(cached_file).collect(),
    }
}

fn cached_file(file: &SourceFile) -> CachedFile {
    CachedFile {
        path: file.path().as_str().to_owned(),
        hash: file.hash().value(),
        backend: match file.backend() {
            Backend::Server => SERVER_BACKEND,
            Backend::TreeSitter => TREE_SITTER_BACKEND,
        },
        symbols: file.symbols().map(cached_symbol).collect(),
        imports: file
            .imports()
            .iter()
            .map(|import| (import.0.to_owned(), import.1.to_owned()))
            .collect(),
        highlights: file
            .highlights()
            .iter()
            .map(|line| line.iter().map(cached_highlight).collect())
            .collect(),
    }
}

fn cached_symbol(symbol: &Symbol) -> CachedSymbol {
    CachedSymbol {
        name: symbol.name().as_str().to_owned(),
        kind: symbol.kind().as_str().to_owned(),
        start: symbol.span().start().value(),
        end: symbol.span().end().value(),
        depth: u8::try_from(symbol.depth().min(LARGEST_DEPTH).value()).unwrap_or_default(),
        owner: symbol
            .owner()
            .map(TypeName::as_str)
            .unwrap_or_default()
            .to_owned(),
        calls: symbol.calls().iter().map(cached_call).collect(),
        targets: symbol.targets().iter().map(cached_location).collect(),
        references: symbol.references().iter().map(cached_location).collect(),
    }
}

fn cached_call(call: &Call) -> CachedCall {
    CachedCall {
        name: call.name.as_str().to_owned(),
        qualifier: match &call.qualifier {
            Qualifier::Plain => CachedQualifier::Plain,
            Qualifier::SelfReference => CachedQualifier::SelfReference,
            Qualifier::Named(scope) => CachedQualifier::Named(scope.as_str().to_owned()),
        },
    }
}

fn cached_location(location: &Location) -> CachedLocation {
    CachedLocation {
        file: location.file.as_str().to_owned(),
        line: location.line.value(),
    }
}

fn cached_highlight(highlight: &Highlight) -> CachedHighlight {
    CachedHighlight {
        start: highlight.start.value(),
        end: highlight.end.value(),
        class: HighlightClass::VARIANTS
            .iter()
            .copied()
            .zip(0..)
            .find(|pair| pair.0 == highlight.class)
            .map_or(0, |pair| pair.1),
    }
}
