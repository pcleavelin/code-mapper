use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::iter;
use std::path::Path;

use domain::{
    Backend, ByteOffset, Call, Depth, FileText, Highlight, HighlightClass, Imports, Line,
    Qualifier, RelativePath, Scope, SourceFile, SourceLine, Span, Symbol, SymbolKind, SymbolName,
    TypeName,
};
use io_source::Contents;
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Grammar {
    Rust,
    Odin,
    Clang,
    Python,
    Javascript,
    Typescript,
    Tsx,
}

impl Grammar {
    fn of(path: &RelativePath) -> Option<Self> {
        let extension = Path::new(path.as_str()).extension()?.to_str()?;
        Some(match extension {
            "rs" => Self::Rust,
            "odin" => Self::Odin,
            "c" | "h" => Self::Clang,
            "py" => Self::Python,
            "js" | "mjs" | "cjs" => Self::Javascript,
            "ts" => Self::Typescript,
            "tsx" => Self::Tsx,
            _ => return None,
        })
    }

    fn compile(self) -> Option<Compiled> {
        let (language, highlights): (tree_sitter::Language, &str) = match self {
            Self::Rust => (
                tree_sitter_rust::LANGUAGE.into(),
                tree_sitter_rust::HIGHLIGHTS_QUERY,
            ),
            Self::Odin => (
                tree_sitter_odin::LANGUAGE.into(),
                tree_sitter_odin::HIGHLIGHTS_QUERY,
            ),
            Self::Clang => (
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::HIGHLIGHT_QUERY,
            ),
            Self::Python => (
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::HIGHLIGHTS_QUERY,
            ),
            Self::Javascript => (
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::HIGHLIGHT_QUERY,
            ),
            Self::Typescript => (
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ),
            Self::Tsx => (
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ),
        };
        let query = Query::new(&language, highlights).ok()?;
        Some(Compiled { language, query })
    }
}

struct Compiled {
    language: tree_sitter::Language,
    query: Query,
}

#[derive(Default)]
pub struct Parsers {
    parser: Parser,
    compiled: BTreeMap<Grammar, Option<Compiled>>,
}

impl Parsers {
    pub fn parse(&mut self, path: RelativePath, contents: &Contents) -> SourceFile {
        let text = contents.text();
        let source = Source(contents.as_str());
        let mut highlights = vec![Vec::new(); text.all().len()];
        let mut collector = Collector {
            source,
            text: &text,
            symbols: Vec::new(),
            imports: Imports::new(),
        };
        let Self { parser, compiled } = self;
        if let Some(grammar) = Grammar::of(&path)
            && let Some(compiled) = compiled
                .entry(grammar)
                .or_insert_with(|| grammar.compile())
                .as_ref()
            && parser.set_language(&compiled.language).is_ok()
            && let Some(tree) = parser.parse(contents.as_str(), None)
        {
            collector.collect(tree.root_node(), Depth::default(), None);
            highlights = highlight(tree.root_node(), &compiled.query, source, &text);
        }
        let Collector {
            symbols, imports, ..
        } = collector;
        SourceFile::new(
            path,
            text,
            highlights,
            symbols,
            imports,
            contents.hash(),
            Backend::TreeSitter,
        )
    }
}

#[derive(Clone, Copy)]
struct Source<'text>(&'text str);

impl<'text> Source<'text> {
    fn of(self, node: Node<'_>) -> Option<&'text str> {
        node.utf8_text(self.0.as_bytes()).ok()
    }

    fn line_starts(self) -> Vec<usize> {
        iter::once(0)
            .chain(self.0.match_indices('\n').map(|found| found.0 + 1))
            .collect()
    }

    fn bytes(self) -> &'text [u8] {
        self.0.as_bytes()
    }
}

#[derive(Clone, Copy)]
struct CaptureName<'query>(&'query str);

impl CaptureName<'_> {
    fn class(self) -> HighlightClass {
        match self.0.split('.').next().unwrap_or_default() {
            "keyword" | "include" | "repeat" | "conditional" | "storageclass" | "storage"
            | "exception" => HighlightClass::Keyword,
            "string" | "character" | "escape" => HighlightClass::String,
            "comment" => HighlightClass::Comment,
            "function" | "method" | "constructor" | "macro" => HighlightClass::Function,
            "type" | "namespace" | "module" => HighlightClass::Type,
            "number" | "constant" | "boolean" | "float" => HighlightClass::Constant,
            "property" | "field" | "attribute" | "label" | "tag" => HighlightClass::Property,
            "variable" if self.0.contains("builtin") => HighlightClass::Constant,
            _ => HighlightClass::Plain,
        }
    }
}

#[derive(Clone, Copy)]
struct Offset(usize);

impl Offset {
    fn byte(self) -> ByteOffset {
        ByteOffset::new(u32::try_from(self.0).unwrap_or_default())
    }
}

fn highlight(
    root: Node<'_>,
    query: &Query,
    source: Source<'_>,
    text: &FileText,
) -> Vec<Vec<Highlight>> {
    let line_starts = source.line_starts();
    let lines = text.all();
    let names = query.capture_names();
    let mut spans: Vec<Vec<Highlight>> = vec![Vec::new(); lines.len()];
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(query, root, source.bytes());
    while let Some((found, position)) = captures.next() {
        let Some(capture) = found.captures.get(*position) else {
            continue;
        };
        let class = usize::try_from(capture.index)
            .ok()
            .and_then(|index| names.get(index))
            .map_or(HighlightClass::Plain, |name| CaptureName(name).class());
        if class == HighlightClass::Plain {
            continue;
        }
        let (start, end) = (capture.node.start_byte(), capture.node.end_byte());
        let first = line_starts
            .partition_point(|line_start| *line_start <= start)
            .saturating_sub(1);
        let rows = line_starts
            .iter()
            .zip(lines)
            .zip(spans.iter_mut())
            .skip(first);
        for ((line_start, line), on_line) in rows {
            if *line_start >= end {
                break;
            }
            let line_end = line_start + line.as_str().len();
            let (from, to) = (start.max(*line_start), end.min(line_end));
            if from < to {
                on_line.push(Highlight {
                    start: Offset(from - line_start).byte(),
                    end: Offset(to - line_start).byte(),
                    class,
                });
            }
        }
    }
    for on_line in &mut spans {
        on_line.sort_by_key(|span| span.start);
        let mut covered = ByteOffset::default();
        on_line.retain(|span| {
            let keep = span.start >= covered;
            if keep {
                covered = span.end;
            }
            keep
        });
    }
    spans
}

#[derive(Clone, Copy)]
pub(crate) struct TypeText<'text>(&'text str);

impl<'text> TypeText<'text> {
    pub(crate) fn new(text: &'text str) -> Self {
        Self(text)
    }

    pub(crate) fn bare(self) -> String {
        let text = self
            .0
            .split('<')
            .next()
            .unwrap_or(self.0)
            .trim_start_matches(['&', '*', ' '])
            .trim_start_matches("mut ")
            .trim();
        text.rsplit("::").next().unwrap_or(text).trim().to_owned()
    }
}

#[derive(Clone, Copy)]
struct ImportText<'text>(&'text str);

impl ImportText<'_> {
    fn record(self, imports: &mut Imports) {
        let text = self.0.trim().trim_end_matches(';');
        if let Some(quote) = text.find(['"', '\'']) {
            let quoted: String = text
                .get(quote + 1..)
                .unwrap_or_default()
                .chars()
                .take_while(|character| *character != '"' && *character != '\'')
                .collect();
            let module = quoted
                .rsplit(['/', ':', '\\'])
                .next()
                .unwrap_or(&quoted)
                .trim_end_matches(".js")
                .trim_end_matches(".ts")
                .to_owned();
            let head = text.get(..quote).unwrap_or_default();
            let mut named = false;
            for token in head
                .split(|character: char| !(character.is_alphanumeric() || character == '_'))
                .filter(|token| !token.is_empty())
            {
                if ["import", "from", "as", "type", "default"].contains(&token) {
                    continue;
                }
                imports.insert(token, &module);
                named = true;
            }
            if !named {
                imports.insert(&module, &module);
            }
            return;
        }
        let body = text
            .trim_start_matches("pub ")
            .trim_start_matches("use ")
            .trim_start_matches("from ")
            .trim_start_matches("import ");
        let (path_part, items) = match body.split_once(" import ") {
            Some((path_part, items)) => (path_part.trim(), items.trim()),
            None => match body.find('{') {
                Some(brace) => (
                    body.get(..brace)
                        .unwrap_or_default()
                        .trim()
                        .trim_end_matches("::"),
                    body.get(brace + 1..)
                        .unwrap_or_default()
                        .trim_end_matches('}'),
                ),
                None => body
                    .rsplit_once("::")
                    .or_else(|| body.rsplit_once('.'))
                    .unwrap_or((body, body)),
            },
        };
        let module = path_part
            .rsplit(['.', ':'])
            .find(|segment| !segment.is_empty())
            .unwrap_or(path_part);
        for item in items.split(',') {
            let item = item.trim();
            if item.is_empty() || item == "*" {
                continue;
            }
            let (name, alias) = match item.split_once(" as ") {
                Some((name, alias)) => (name.trim(), Some(alias.trim())),
                None => (item, None),
            };
            let name = name.rsplit("::").next().unwrap_or(name).trim();
            if name.is_empty() || name == "self" {
                continue;
            }
            imports.insert(alias.unwrap_or(name), module);
        }
    }
}

#[derive(Clone, Copy)]
struct CalleeText<'text>(&'text str);

impl CalleeText<'_> {
    fn call(self) -> Option<Call> {
        let mut clean = String::with_capacity(self.0.len());
        let mut depth = 0_i32;
        for character in self.0.chars() {
            match character {
                '<' | '(' | '[' => depth += 1,
                '>' | ')' | ']' => depth -= 1,
                _ if depth == 0 => clean.push(character),
                _ => {}
            }
        }
        let clean = clean.trim().trim_end_matches('!').trim();
        let segments: Vec<&str> = clean
            .split(['.', ':'])
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .collect();
        let name = *segments.last()?;
        if name.len() > 64
            || !name
                .chars()
                .next()
                .is_some_and(|character| character.is_alphabetic() || character == '_')
        {
            return None;
        }
        let qualifier = segments
            .iter()
            .rev()
            .nth(1)
            .map_or(Qualifier::Plain, |segment| {
                QualifierText(segment).qualifier()
            });
        Some(Call {
            name: SymbolName::new(name),
            qualifier,
        })
    }
}

#[derive(Clone, Copy)]
struct QualifierText<'text>(&'text str);

impl QualifierText<'_> {
    fn qualifier(self) -> Qualifier {
        match self.0 {
            "self" | "Self" | "this" | "super" => Qualifier::SelfReference,
            text => Qualifier::Named(Scope::new(&TypeText(text).bare())),
        }
    }
}

#[derive(Clone, Copy)]
struct NodeKind<'tree>(&'tree str);

impl NodeKind<'_> {
    fn imports(self) -> bool {
        self.0.contains("import") || self.0 == "use_declaration"
    }

    fn skipped(self) -> bool {
        self.0.contains("comment") || self.0.contains("package") || self.0.contains("attribute")
    }

    fn container(self) -> bool {
        ["impl", "mod", "trait", "class"]
            .iter()
            .any(|container| self.0.contains(container))
    }

    fn named(self, kind: &str) -> bool {
        self.0 == kind
    }

    fn mentions(self, word: &str) -> bool {
        self.0.contains(word)
    }

    fn symbol_kind(self) -> SymbolKind {
        SymbolKind::new(self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Row(usize);

impl Row {
    fn line(self) -> Option<Line> {
        u32::try_from(self.0).ok().map(Line::new)
    }
}

pub(crate) fn call_order(one: &Call, other: &Call) -> Ordering {
    let spelled = |qualifier: &Qualifier| match qualifier {
        Qualifier::Plain => "None".to_owned(),
        Qualifier::SelfReference => "SelfRef".to_owned(),
        Qualifier::Named(scope) => format!("Some({:?})", scope.as_str()),
    };
    one.name
        .cmp(&other.name)
        .then_with(|| spelled(&one.qualifier).cmp(&spelled(&other.qualifier)))
}

struct Collector<'text> {
    source: Source<'text>,
    text: &'text FileText,
    symbols: Vec<Symbol>,
    imports: Imports,
}

impl Collector<'_> {
    fn line(&self, row: Row) -> Option<&SourceLine> {
        self.text.line(row.line()?)
    }

    fn impl_name(&self, node: Node<'_>) -> Option<SymbolName> {
        let field = |name: &str| {
            node.child_by_field_name(name)
                .and_then(|child| self.source.of(child))
                .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        };
        match (field("trait"), field("type")) {
            (Some(named), Some(implemented)) => {
                Some(SymbolName::new(&format!("impl {named} for {implemented}")))
            }
            (None, Some(implemented)) => Some(SymbolName::new(&format!("impl {implemented}"))),
            _ => None,
        }
    }

    fn collect(&mut self, parent: Node<'_>, depth: Depth, owner: Option<&TypeName>) {
        let mut cursor = parent.walk();
        for node in parent.named_children(&mut cursor) {
            let kind = NodeKind(node.kind());
            if kind.imports() {
                if let Some(text) = self.source.of(node) {
                    ImportText(text).record(&mut self.imports);
                }
                continue;
            }
            if kind.skipped() {
                continue;
            }
            if kind.named("mod_item") && node.child_by_field_name("body").is_none() {
                continue;
            }
            let start = Row(node.start_position().row);
            let mut end = Row(node.end_position().row);
            if node.end_position().column == 0 && end > start {
                end = Row(end.0 - 1);
            }
            let mut name_row = start;
            while name_row < end
                && self
                    .line(name_row)
                    .is_some_and(|line| line.as_str().trim_start().starts_with('@'))
            {
                name_row = Row(name_row.0 + 1);
            }
            let first = self
                .line(name_row)
                .map(SourceLine::trimmed)
                .unwrap_or_default();
            let named = match node.child_by_field_name("name") {
                Some(name) => Some(SymbolName::new(self.source.of(name).unwrap_or_default())),
                None if kind.named("impl_item") => self.impl_name(node),
                None => {
                    let guessed = first
                        .split('{')
                        .next()
                        .unwrap_or(first)
                        .split("::")
                        .next()
                        .unwrap_or(first)
                        .trim();
                    (guessed.len() <= 80).then(|| SymbolName::new(guessed))
                }
            };
            let Some(name) = named.filter(|name| !name.as_str().is_empty()) else {
                continue;
            };
            let (Some(start_line), Some(end_line)) = (start.line(), end.line()) else {
                continue;
            };
            let Some(span) = Span::new(start_line, end_line) else {
                continue;
            };
            let container = depth == Depth::default() && kind.container();
            let own_type = if container {
                node.child_by_field_name("type")
                    .and_then(|child| self.source.of(child))
                    .map(|text| TypeName::new(&TypeText(text).bare()))
                    .or_else(|| kind.mentions("class").then(|| TypeName::new(name.as_str())))
            } else {
                None
            };
            let mut calls = Vec::new();
            if !container {
                self.find_calls(node, &mut calls);
                calls.sort_by(call_order);
                calls.dedup();
            }
            self.symbols.push(Symbol::new(
                name,
                kind.symbol_kind(),
                span,
                depth,
                if container {
                    own_type.clone()
                } else {
                    owner.cloned()
                },
                calls,
            ));
            if container && let Some(body) = node.child_by_field_name("body") {
                self.collect(body, Depth::default().deeper(), own_type.as_ref());
            }
        }
    }

    fn find_calls(&self, node: Node<'_>, calls: &mut Vec<Call>) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if NodeKind(child.kind()).mentions("call") {
                let callee = child
                    .child_by_field_name("function")
                    .or_else(|| child.named_child(0));
                if let Some(text) = callee.and_then(|callee| self.source.of(callee))
                    && let Some(mut call) = CalleeText(text).call()
                {
                    if call.qualifier == Qualifier::Plain {
                        let parent = NodeKind(node.kind());
                        if ["member", "selector", "scoped"]
                            .iter()
                            .any(|word| parent.mentions(word))
                            && let Some(first) =
                                node.named_child(0).filter(|first| first.id() != child.id())
                            && let Some(qualified) = self.source.of(first)
                            && let Some(segment) = qualified
                                .rsplit(['.', ':'])
                                .next()
                                .filter(|segment| !segment.is_empty())
                        {
                            call.qualifier = QualifierText(segment).qualifier();
                        }
                    }
                    calls.push(call);
                }
            }
            self.find_calls(child, calls);
        }
    }
}

#[cfg(test)]
pub(crate) fn callee(text: &str) -> Option<Call> {
    CalleeText(text).call()
}

#[cfg(test)]
pub(crate) fn record_import(text: &str, imports: &mut Imports) {
    ImportText(text).record(imports);
}
