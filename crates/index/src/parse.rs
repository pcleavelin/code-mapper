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
    const ALL: [Self; 7] = [
        Self::Rust,
        Self::Odin,
        Self::Clang,
        Self::Python,
        Self::Javascript,
        Self::Typescript,
        Self::Tsx,
    ];

    fn suffixes(self) -> &'static [Word] {
        match self {
            Self::Rust => &[Word("rs")],
            Self::Odin => &[Word("odin")],
            Self::Clang => &[Word("c"), Word("h")],
            Self::Python => &[Word("py")],
            Self::Javascript => &[Word("js"), Word("mjs"), Word("cjs")],
            Self::Typescript => &[Word("ts")],
            Self::Tsx => &[Word("tsx")],
        }
    }

    fn of(path: &RelativePath) -> Option<Self> {
        let extension = Path::new(path.as_str()).extension()?.to_str()?;
        Self::ALL.into_iter().find(|grammar| {
            grammar
                .suffixes()
                .iter()
                .any(|suffix| suffix.as_str() == extension)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Word(&'static str);

impl Word {
    const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Capture {
    Keyword,
    Include,
    Repeat,
    Conditional,
    StorageClass,
    Storage,
    Exception,
    String,
    Character,
    Escape,
    Comment,
    Function,
    Method,
    Constructor,
    Macro,
    Type,
    Namespace,
    Module,
    Number,
    Constant,
    Boolean,
    Float,
    Property,
    Field,
    Attribute,
    Label,
    Tag,
    BuiltinVariable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Modifier {
    Builtin,
}

impl Modifier {
    fn word(self) -> Word {
        Word(match self {
            Self::Builtin => "builtin",
        })
    }
}

impl Capture {
    const ALL: [Self; 28] = [
        Self::Keyword,
        Self::Include,
        Self::Repeat,
        Self::Conditional,
        Self::StorageClass,
        Self::Storage,
        Self::Exception,
        Self::String,
        Self::Character,
        Self::Escape,
        Self::Comment,
        Self::Function,
        Self::Method,
        Self::Constructor,
        Self::Macro,
        Self::Type,
        Self::Namespace,
        Self::Module,
        Self::Number,
        Self::Constant,
        Self::Boolean,
        Self::Float,
        Self::Property,
        Self::Field,
        Self::Attribute,
        Self::Label,
        Self::Tag,
        Self::BuiltinVariable,
    ];

    fn word(self) -> Word {
        Word(match self {
            Self::Keyword => "keyword",
            Self::Include => "include",
            Self::Repeat => "repeat",
            Self::Conditional => "conditional",
            Self::StorageClass => "storageclass",
            Self::Storage => "storage",
            Self::Exception => "exception",
            Self::String => "string",
            Self::Character => "character",
            Self::Escape => "escape",
            Self::Comment => "comment",
            Self::Function => "function",
            Self::Method => "method",
            Self::Constructor => "constructor",
            Self::Macro => "macro",
            Self::Type => "type",
            Self::Namespace => "namespace",
            Self::Module => "module",
            Self::Number => "number",
            Self::Constant => "constant",
            Self::Boolean => "boolean",
            Self::Float => "float",
            Self::Property => "property",
            Self::Field => "field",
            Self::Attribute => "attribute",
            Self::Label => "label",
            Self::Tag => "tag",
            Self::BuiltinVariable => "variable",
        })
    }

    fn class(self) -> HighlightClass {
        match self {
            Self::Keyword
            | Self::Include
            | Self::Repeat
            | Self::Conditional
            | Self::StorageClass
            | Self::Storage
            | Self::Exception => HighlightClass::Keyword,
            Self::String | Self::Character | Self::Escape => HighlightClass::String,
            Self::Comment => HighlightClass::Comment,
            Self::Function | Self::Method | Self::Constructor | Self::Macro => {
                HighlightClass::Function
            }
            Self::Type | Self::Namespace | Self::Module => HighlightClass::Type,
            Self::Number | Self::Constant | Self::Boolean | Self::Float | Self::BuiltinVariable => {
                HighlightClass::Constant
            }
            Self::Property | Self::Field | Self::Attribute | Self::Label | Self::Tag => {
                HighlightClass::Property
            }
        }
    }

    fn named(text: CaptureName<'_>) -> Option<Self> {
        let prefix = text.0.split('.').next().unwrap_or_default();
        if prefix == Self::BuiltinVariable.word().as_str() {
            return text
                .0
                .contains(Modifier::Builtin.word().as_str())
                .then_some(Self::BuiltinVariable);
        }
        Self::ALL
            .into_iter()
            .filter(|capture| *capture != Self::BuiltinVariable)
            .find(|capture| capture.word().as_str() == prefix)
    }
}

#[derive(Clone, Copy)]
struct CaptureName<'query>(&'query str);

impl CaptureName<'_> {
    fn class(self) -> HighlightClass {
        Capture::named(self).map_or(HighlightClass::Plain, Capture::class)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Split {
    Path,
    Alias,
    PythonImport,
}

impl Split {
    fn word(self) -> Word {
        Word(match self {
            Self::Path => "::",
            Self::Alias => " as ",
            Self::PythonImport => " import ",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypePrefix {
    Mut,
}

impl TypePrefix {
    fn word(self) -> Word {
        Word(match self {
            Self::Mut => "mut ",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScriptSuffix {
    Javascript,
    Typescript,
}

impl ScriptSuffix {
    const ALL: [Self; 2] = [Self::Javascript, Self::Typescript];

    fn word(self) -> Word {
        Word(match self {
            Self::Javascript => ".js",
            Self::Typescript => ".ts",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImportKeyword {
    Import,
    From,
    As,
    Type,
    Default,
}

impl ImportKeyword {
    const ALL: [Self; 5] = [
        Self::Import,
        Self::From,
        Self::As,
        Self::Type,
        Self::Default,
    ];

    fn word(self) -> Word {
        Word(match self {
            Self::Import => "import",
            Self::From => "from",
            Self::As => "as",
            Self::Type => "type",
            Self::Default => "default",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImportPrefix {
    Public,
    RustImport,
    From,
    Import,
}

impl ImportPrefix {
    const ALL: [Self; 4] = [Self::Public, Self::RustImport, Self::From, Self::Import];

    fn word(self) -> Word {
        Word(match self {
            Self::Public => "pub ",
            Self::RustImport => "use ",
            Self::From => "from ",
            Self::Import => "import ",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ImportToken {
    All,
}

impl ImportToken {
    fn word(self) -> Word {
        Word(match self {
            Self::All => "*",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelfWord {
    Lower,
    Type,
    Instance,
    Parent,
}

impl SelfWord {
    const ALL: [Self; 4] = [Self::Lower, Self::Type, Self::Instance, Self::Parent];

    fn word(self) -> Word {
        Word(match self {
            Self::Lower => "self",
            Self::Type => "Self",
            Self::Instance => "this",
            Self::Parent => "super",
        })
    }
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
            .trim_start_matches(TypePrefix::Mut.word().as_str())
            .trim();
        text.rsplit(Split::Path.word().as_str())
            .next()
            .unwrap_or(text)
            .trim()
            .to_owned()
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
            let module = ScriptSuffix::ALL
                .into_iter()
                .fold(
                    quoted.rsplit(['/', ':', '\\']).next().unwrap_or(&quoted),
                    |name, suffix| name.trim_end_matches(suffix.word().as_str()),
                )
                .to_owned();
            let head = text.get(..quote).unwrap_or_default();
            let mut named = false;
            for token in head
                .split(|character: char| !(character.is_alphanumeric() || character == '_'))
                .filter(|token| !token.is_empty())
            {
                if ImportKeyword::ALL
                    .into_iter()
                    .any(|keyword| keyword.word().as_str() == token)
                {
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
        let body = ImportPrefix::ALL.into_iter().fold(text, |body, prefix| {
            body.trim_start_matches(prefix.word().as_str())
        });
        let (path_part, items) = match body.split_once(Split::PythonImport.word().as_str()) {
            Some((path_part, items)) => (path_part.trim(), items.trim()),
            None => match body.find('{') {
                Some(brace) => (
                    body.get(..brace)
                        .unwrap_or_default()
                        .trim()
                        .trim_end_matches(Split::Path.word().as_str()),
                    body.get(brace + 1..)
                        .unwrap_or_default()
                        .trim_end_matches('}'),
                ),
                None => body
                    .rsplit_once(Split::Path.word().as_str())
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
            if item.is_empty() || item == ImportToken::All.word().as_str() {
                continue;
            }
            let (name, alias) = match item.split_once(Split::Alias.word().as_str()) {
                Some((name, alias)) => (name.trim(), Some(alias.trim())),
                None => (item, None),
            };
            let name = name
                .rsplit(Split::Path.word().as_str())
                .next()
                .unwrap_or(name)
                .trim();
            if name.is_empty() || name == SelfWord::Lower.word().as_str() {
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
        if SelfWord::ALL
            .into_iter()
            .any(|word| word.word().as_str() == self.0)
        {
            return Qualifier::SelfReference;
        }
        Qualifier::Named(Scope::new(&TypeText(self.0).bare()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NodeWord {
    Import,
    Comment,
    Package,
    Attribute,
    Impl,
    Module,
    Spec,
    Class,
    Call,
    Member,
    Select,
    Dotted,
}

impl NodeWord {
    const CONTAINER: [Self; 4] = [Self::Impl, Self::Module, Self::Spec, Self::Class];

    fn word(self) -> Word {
        Word(match self {
            Self::Import => "import",
            Self::Comment => "comment",
            Self::Package => "package",
            Self::Attribute => "attribute",
            Self::Impl => "impl",
            Self::Module => "mod",
            Self::Spec => "trait",
            Self::Class => "class",
            Self::Call => "call",
            Self::Member => "member",
            Self::Select => "selector",
            Self::Dotted => "scoped",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NodeKindWord {
    Import,
    Module,
    Impl,
}

impl NodeKindWord {
    fn word(self) -> Word {
        Word(match self {
            Self::Import => "use_declaration",
            Self::Module => "mod_item",
            Self::Impl => "impl_item",
        })
    }
}

#[derive(Clone, Copy)]
struct NodeKind<'tree>(&'tree str);

impl NodeKind<'_> {
    fn imports(self) -> bool {
        self.mentions(NodeWord::Import) || self.named(NodeKindWord::Import)
    }

    fn skipped(self) -> bool {
        self.mentions(NodeWord::Comment)
            || self.mentions(NodeWord::Package)
            || self.mentions(NodeWord::Attribute)
    }

    fn container(self) -> bool {
        NodeWord::CONTAINER
            .into_iter()
            .any(|word| self.mentions(word))
    }

    fn named(self, kind: NodeKindWord) -> bool {
        self.0 == kind.word().as_str()
    }

    fn mentions(self, word: NodeWord) -> bool {
        self.0.contains(word.word().as_str())
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
            if kind.named(NodeKindWord::Module) && node.child_by_field_name("body").is_none() {
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
                None if kind.named(NodeKindWord::Impl) => self.impl_name(node),
                None => {
                    let guessed = first
                        .split('{')
                        .next()
                        .unwrap_or(first)
                        .split(Split::Path.word().as_str())
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
                    .or_else(|| {
                        kind.mentions(NodeWord::Class)
                            .then(|| TypeName::new(name.as_str()))
                    })
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
            if NodeKind(child.kind()).mentions(NodeWord::Call) {
                let callee = child
                    .child_by_field_name("function")
                    .or_else(|| child.named_child(0));
                if let Some(text) = callee.and_then(|callee| self.source.of(callee))
                    && let Some(mut call) = CalleeText(text).call()
                {
                    if call.qualifier == Qualifier::Plain {
                        let parent = NodeKind(node.kind());
                        if [NodeWord::Member, NodeWord::Select, NodeWord::Dotted]
                            .into_iter()
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
