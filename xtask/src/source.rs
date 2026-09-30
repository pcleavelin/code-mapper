use std::fs;
use std::iter;
use std::sync::OnceLock;

use ignore::WalkBuilder;
use strum::VariantArray;
use tree_sitter::{Language, Node, Parser, Tree};

use crate::text::{LineNumber, Literal, Message, RepoPath, Root};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KindName(&'static str);

impl KindName {
    const fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, VariantArray)]
pub(crate) enum NodeKind {
    Struct,
    Enum,
    Function,
    FunctionSignature,
    FunctionType,
    FieldDeclaration,
    Constant,
    Static,
    OrderedFieldDeclarationList,
    LineComment,
    BlockComment,
    PrimitiveType,
    TupleType,
    TypeIdentifier,
    IndexExpression,
    BinaryExpression,
    ScopedIdentifier,
    Parameter,
    LetDeclaration,
    Identifier,
    FieldIdentifier,
    AttributeItem,
    InnerAttributeItem,
    CallExpression,
    FieldExpression,
    IntegerLiteral,
    FloatLiteral,
    StringLiteral,
    UnaryExpression,
    MatchPattern,
    OrPattern,
    TuplePattern,
    SlicePattern,
    TupleStructPattern,
    FieldPattern,
    ReferencePattern,
    ReferenceExpression,
    CapturedPattern,
    Arguments,
    MacroInvocation,
    RawStringLiteral,
    Trait,
    Impl,
    Module,
    MacroDefinition,
    Union,
    EnumVariant,
    UseDeclaration,
    TypeItem,
    VisibilityModifier,
}

impl NodeKind {
    const fn name(self) -> KindName {
        KindName(match self {
            Self::Struct => "struct_item",
            Self::Enum => "enum_item",
            Self::Function => "function_item",
            Self::FunctionSignature => "function_signature_item",
            Self::FunctionType => "function_type",
            Self::FieldDeclaration => "field_declaration",
            Self::Constant => "const_item",
            Self::Static => "static_item",
            Self::OrderedFieldDeclarationList => "ordered_field_declaration_list",
            Self::LineComment => "line_comment",
            Self::BlockComment => "block_comment",
            Self::PrimitiveType => "primitive_type",
            Self::TupleType => "tuple_type",
            Self::TypeIdentifier => "type_identifier",
            Self::IndexExpression => "index_expression",
            Self::BinaryExpression => "binary_expression",
            Self::ScopedIdentifier => "scoped_identifier",
            Self::Parameter => "parameter",
            Self::LetDeclaration => "let_declaration",
            Self::Identifier => "identifier",
            Self::FieldIdentifier => "field_identifier",
            Self::AttributeItem => "attribute_item",
            Self::InnerAttributeItem => "inner_attribute_item",
            Self::CallExpression => "call_expression",
            Self::FieldExpression => "field_expression",
            Self::IntegerLiteral => "integer_literal",
            Self::FloatLiteral => "float_literal",
            Self::StringLiteral => "string_literal",
            Self::UnaryExpression => "unary_expression",
            Self::MatchPattern => "match_pattern",
            Self::OrPattern => "or_pattern",
            Self::TuplePattern => "tuple_pattern",
            Self::SlicePattern => "slice_pattern",
            Self::TupleStructPattern => "tuple_struct_pattern",
            Self::FieldPattern => "field_pattern",
            Self::ReferencePattern => "reference_pattern",
            Self::ReferenceExpression => "reference_expression",
            Self::CapturedPattern => "captured_pattern",
            Self::Arguments => "arguments",
            Self::MacroInvocation => "macro_invocation",
            Self::RawStringLiteral => "raw_string_literal",
            Self::Trait => "trait_item",
            Self::Impl => "impl_item",
            Self::Module => "mod_item",
            Self::MacroDefinition => "macro_definition",
            Self::Union => "union_item",
            Self::EnumVariant => "enum_variant",
            Self::UseDeclaration => "use_declaration",
            Self::TypeItem => "type_item",
            Self::VisibilityModifier => "visibility_modifier",
        })
    }

    pub(crate) fn is(self, node: Node<'_>) -> bool {
        node.kind() == self.name().as_str()
    }

    pub(crate) fn of(node: Node<'_>) -> Option<Self> {
        kind_map()
            .get(usize::from(node.kind_id()))
            .copied()
            .flatten()
    }
}

fn kind_map() -> &'static [Option<NodeKind>] {
    static MAP: OnceLock<Vec<Option<NodeKind>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let language: Language = tree_sitter_rust::LANGUAGE.into();
        let mut map = vec![None; language.node_kind_count()];
        for kind in NodeKind::VARIANTS.iter().copied() {
            let slot = usize::from(language.id_for_node_kind(kind.name().as_str(), true));
            if let Some(entry) = map.get_mut(slot) {
                *entry = Some(kind);
            }
        }
        map
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestPath {
    Prefix,
    Middle,
    Suffix,
}

impl TestPath {
    const fn name(self) -> Literal {
        match self {
            Self::Prefix => Literal::new("tests/"),
            Self::Middle => Literal::new("/tests/"),
            Self::Suffix => Literal::new("/tests.rs"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TestAttribute {
    Test,
    ConfigTest,
}

impl TestAttribute {
    pub(crate) const fn name(self) -> Literal {
        match self {
            Self::Test => Literal::new("#[test]"),
            Self::ConfigTest => Literal::new("cfg(test)"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IgnoredDirectory {
    Git,
    Jj,
    Target,
    CodeMapCache,
}

impl IgnoredDirectory {
    const fn name(self) -> Literal {
        match self {
            Self::Git => Literal::new(".git"),
            Self::Jj => Literal::new(".jj"),
            Self::Target => Literal::new("target"),
            Self::CodeMapCache => Literal::new(".codemap-cache"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Extension {
    Rust,
}

impl Extension {
    pub(crate) const fn name(self) -> Literal {
        match self {
            Self::Rust => Literal::new(".rs"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keyword {
    Pub,
}

impl Keyword {
    const fn name(self) -> Literal {
        match self {
            Self::Pub => Literal::new("pub"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    Strict,
    Test,
}

impl Zone {
    pub(crate) fn of(path: &RepoPath) -> Self {
        if path.starts_with(TestPath::Prefix.name().as_str())
            || path.contains(TestPath::Middle.name().as_str())
            || path.ends_with(TestPath::Suffix.name().as_str())
        {
            Self::Test
        } else {
            Self::Strict
        }
    }
}

#[derive(Debug)]
pub(crate) struct SourceText(String);

impl SourceText {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub(crate) fn of<'tree>(&'tree self, node: Node<'tree>) -> &'tree str {
        node.utf8_text(self.0.as_bytes()).unwrap_or_default()
    }

    pub(crate) fn attributes<'tree>(&'tree self, item: Node<'tree>) -> Vec<&'tree str> {
        let mut out = Vec::new();
        let mut previous = item.prev_named_sibling();
        while let Some(node) = previous {
            match NodeKind::of(node) {
                Some(NodeKind::AttributeItem) => out.push(self.of(node)),
                Some(NodeKind::LineComment | NodeKind::BlockComment) => {}
                _ => break,
            }
            previous = node.prev_named_sibling();
        }
        out
    }
}

#[derive(Debug)]
pub(crate) struct SourceFile {
    pub(crate) path: RepoPath,
    pub(crate) text: SourceText,
    pub(crate) tree: Tree,
    pub(crate) zone: Zone,
}

impl SourceFile {
    pub(crate) fn parse(path: RepoPath, text: SourceText) -> Result<Self, Message> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|error| Message::new(format!("tree-sitter-rust: {error}")))?;
        let tree = parser
            .parse(text.0.as_bytes(), None)
            .ok_or_else(|| Message::new(format!("{path}: does not parse")))?;
        let zone = Zone::of(&path);
        Ok(Self {
            path,
            text,
            tree,
            zone,
        })
    }

    pub(crate) fn load(root: &Root, path: RepoPath) -> Result<Self, Message> {
        let text = fs::read_to_string(root.join(&path))
            .map_err(|error| Message::new(format!("{path}: {error}")))?;
        Self::parse(path, SourceText::new(text))
    }

    pub(crate) fn nodes(&self) -> Vec<Node<'_>> {
        let mut out = Vec::new();
        let mut cursor = self.tree.walk();
        let mut descending = true;
        loop {
            if descending {
                out.push(cursor.node());
                if cursor.goto_first_child() {
                    continue;
                }
            }
            if cursor.goto_next_sibling() {
                descending = true;
                continue;
            }
            if !cursor.goto_parent() {
                break;
            }
            descending = false;
        }
        out
    }
}

pub(crate) fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

pub(crate) fn public(node: Node<'_>, text: &SourceText) -> bool {
    named_children(node).iter().any(|child| {
        NodeKind::VisibilityModifier.is(*child) && text.of(*child) == Keyword::Pub.name().as_str()
    })
}

pub(crate) fn line_of(node: Node<'_>) -> LineNumber {
    LineNumber::from_row(node.start_position().row)
}

pub(crate) fn descendants(node: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(next) = stack.pop() {
        out.push(next);
        let mut cursor = next.walk();
        let children: Vec<Node<'_>> = next.children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    out
}

pub(crate) fn ancestors(node: Node<'_>) -> Vec<Node<'_>> {
    let mut out = Vec::new();
    let mut next = node.parent();
    while let Some(parent) = next {
        out.push(parent);
        next = parent.parent();
    }
    out
}

pub(crate) fn in_test_code(node: Node<'_>, text: &SourceText) -> bool {
    ancestors(node)
        .into_iter()
        .chain(iter::once(node))
        .filter(|item| {
            matches!(
                NodeKind::of(*item),
                Some(NodeKind::Module | NodeKind::Function)
            )
        })
        .any(|item| {
            text.attributes(item).iter().any(|attribute| {
                attribute.contains(TestAttribute::ConfigTest.name().as_str())
                    || *attribute == TestAttribute::Test.name().as_str()
            })
        })
}

pub(crate) fn workspace_files(root: &Root) -> Vec<RepoPath> {
    let mut out: Vec<RepoPath> = WalkBuilder::new(root.path())
        .hidden(false)
        .filter_entry(|entry| {
            !entry.file_name().to_str().is_some_and(|name| {
                name == IgnoredDirectory::Git.name().as_str()
                    || name == IgnoredDirectory::Jj.name().as_str()
                    || name == IgnoredDirectory::Target.name().as_str()
                    || name == IgnoredDirectory::CodeMapCache.name().as_str()
            })
        })
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter_map(|entry| root.relative(entry.path()))
        .collect();
    out.sort();
    out
}

pub(crate) fn rust_sources(root: &Root) -> Vec<RepoPath> {
    workspace_files(root)
        .into_iter()
        .filter(|path| path.ends_with(Extension::Rust.name().as_str()))
        .filter(|path| {
            ["crates/", "xtask/"]
                .iter()
                .any(|prefix| path.starts_with(prefix))
        })
        .collect()
}
