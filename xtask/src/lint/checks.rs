use tree_sitter::Node;

use crate::arch;
use crate::lint::{Finding, Rule, Workspace};
use crate::source::{
    NodeKind, SourceFile, SourceText, TestAttribute, ancestors, descendants, in_test_code, line_of,
    named_children, public,
};
use crate::text::{CrateName, LintName, Literal, Message, TypeName, Word};
use crate::vocabulary::Verdict;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operator {
    Equal,
    Unequal,
}

impl Operator {
    const fn name(self) -> Literal {
        match self {
            Self::Equal => Literal::new("=="),
            Self::Unequal => Literal::new("!="),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TypeWord {
    Boolean,
    String,
}

impl TypeWord {
    const fn name(self) -> Literal {
        match self {
            Self::Boolean => Literal::new("bool"),
            Self::String => Literal::new("String"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sentinel {
    EmptyString,
    NegativeOne,
}

impl Sentinel {
    const fn name(self) -> Literal {
        match self {
            Self::EmptyString => Literal::new("\"\""),
            Self::NegativeOne => Literal::new("-1"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MacroName {
    Matches,
}

impl MacroName {
    const fn name(self) -> Literal {
        match self {
            Self::Matches => Literal::new("matches"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Punctuation {
    Comma,
}

impl Punctuation {
    const fn name(self) -> Literal {
        match self {
            Self::Comma => Literal::new(","),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SuppressionAttribute {
    Allow,
    Expect,
    Reason,
}

impl SuppressionAttribute {
    const fn name(self) -> Literal {
        match self {
            Self::Allow => Literal::new("allow"),
            Self::Expect => Literal::new("expect"),
            Self::Reason => Literal::new("reason"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RepoArea {
    WireFile,
    WireDirectory,
    CratesPrefix,
    DomainCrate,
    GuiCrateSource,
}

impl RepoArea {
    const fn name(self) -> Literal {
        match self {
            Self::WireFile => Literal::new("/wire.rs"),
            Self::WireDirectory => Literal::new("/wire/"),
            Self::CratesPrefix => Literal::new("crates/"),
            Self::DomainCrate => Literal::new("crates/domain/"),
            Self::GuiCrateSource => Literal::new("crates/gui/src/"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PatternMethod {
    StartsWith,
    EndsWith,
    StripPrefix,
    StripSuffix,
    Contains,
    Find,
    ReverseFind,
    Split,
    ReverseSplit,
    SplitCount,
    ReverseSplitCount,
    SplitOnce,
    ReverseSplitOnce,
    SplitTerminator,
    Matches,
    MatchIndices,
    TrimMatches,
    TrimStartMatches,
    TrimEndMatches,
    Replace,
    ReplaceCount,
    EqualIgnoreAsciiCase,
}

impl PatternMethod {
    const ALL: [Self; 22] = [
        Self::StartsWith,
        Self::EndsWith,
        Self::StripPrefix,
        Self::StripSuffix,
        Self::Contains,
        Self::Find,
        Self::ReverseFind,
        Self::Split,
        Self::ReverseSplit,
        Self::SplitCount,
        Self::ReverseSplitCount,
        Self::SplitOnce,
        Self::ReverseSplitOnce,
        Self::SplitTerminator,
        Self::Matches,
        Self::MatchIndices,
        Self::TrimMatches,
        Self::TrimStartMatches,
        Self::TrimEndMatches,
        Self::Replace,
        Self::ReplaceCount,
        Self::EqualIgnoreAsciiCase,
    ];

    const fn name(self) -> Literal {
        match self {
            Self::StartsWith => Literal::new("starts_with"),
            Self::EndsWith => Literal::new("ends_with"),
            Self::StripPrefix => Literal::new("strip_prefix"),
            Self::StripSuffix => Literal::new("strip_suffix"),
            Self::Contains => Literal::new("contains"),
            Self::Find => Literal::new("find"),
            Self::ReverseFind => Literal::new("rfind"),
            Self::Split => Literal::new("split"),
            Self::ReverseSplit => Literal::new("rsplit"),
            Self::SplitCount => Literal::new("splitn"),
            Self::ReverseSplitCount => Literal::new("rsplitn"),
            Self::SplitOnce => Literal::new("split_once"),
            Self::ReverseSplitOnce => Literal::new("rsplit_once"),
            Self::SplitTerminator => Literal::new("split_terminator"),
            Self::Matches => Literal::new("matches"),
            Self::MatchIndices => Literal::new("match_indices"),
            Self::TrimMatches => Literal::new("trim_matches"),
            Self::TrimStartMatches => Literal::new("trim_start_matches"),
            Self::TrimEndMatches => Literal::new("trim_end_matches"),
            Self::Replace => Literal::new("replace"),
            Self::ReplaceCount => Literal::new("replacen"),
            Self::EqualIgnoreAsciiCase => Literal::new("eq_ignore_ascii_case"),
        }
    }
}

const INTEGER_TYPES: [Literal; 12] = [
    Literal::new("u8"),
    Literal::new("u16"),
    Literal::new("u32"),
    Literal::new("u64"),
    Literal::new("u128"),
    Literal::new("usize"),
    Literal::new("i8"),
    Literal::new("i16"),
    Literal::new("i32"),
    Literal::new("i64"),
    Literal::new("i128"),
    Literal::new("isize"),
];

const DOMAIN_IO: [Literal; 5] = [
    Literal::new("std::fs"),
    Literal::new("std::process"),
    Literal::new("std::thread"),
    Literal::new("std::net"),
    Literal::new("std::io"),
];

const WIDGET_CALLS: [Literal; 2] = [Literal::new("open"), Literal::new("leaf")];

const THEME_CALLS: [Literal; 3] = [
    Literal::new("Px::new"),
    Literal::new("Color::rgba"),
    Literal::new("FontSize::new"),
];

const ID_CALLS: [Literal; 1] = [Literal::new("Id::new")];

const KEY_PATHS: [Literal; 2] = [Literal::new("Key::"), Literal::new("Mods::")];

const PRINTING: [Literal; 5] = [
    Literal::new("print"),
    Literal::new("println"),
    Literal::new("eprint"),
    Literal::new("eprintln"),
    Literal::new("dbg"),
];

struct Report<'report> {
    file: &'report SourceFile,
    findings: &'report mut Vec<Finding>,
    rule: Rule,
}

impl Report<'_> {
    fn add(&mut self, node: Node<'_>, detail: impl Into<Message>) {
        self.findings.push(Finding {
            path: self.file.path.clone(),
            line: line_of(node),
            rule: self.rule,
            detail: detail.into(),
        });
    }
}

pub(super) fn check(
    rule: Rule,
    workspace: &Workspace,
    file: &SourceFile,
    findings: &mut Vec<Finding>,
) {
    let mut report = Report {
        file,
        findings,
        rule,
    };
    match rule {
        Rule::Comment => comments(file, &mut report),
        Rule::Primitive => primitives(workspace, file, &mut report),
        Rule::NewtypeField => newtype_fields(file, &mut report),
        Rule::Indexing => indexing(file, &mut report),
        Rule::Absence => absence(file, &mut report),
        Rule::Vocabulary => vocabulary(workspace, file, &mut report),
        Rule::WireLeak => wire_leaks(workspace, file, &mut report),
        Rule::DomainIo => domain_io(file, &mut report),
        Rule::Suppression => suppressions(file, &mut report),
        Rule::TestRegistry => test_registry(file, &mut report),
        Rule::Alias => aliases(file, &mut report),
        Rule::Widget => gui_calls(file, &mut report, Literal::new("widgets"), &WIDGET_CALLS),
        Rule::Theme => gui_literals(file, &mut report, Literal::new("theme"), &THEME_CALLS),
        Rule::ElementId => gui_literals(file, &mut report, Literal::new("ids"), &ID_CALLS),
        Rule::KeyBinding => gui_paths(file, &mut report, Literal::new("keys"), &KEY_PATHS),
        Rule::Compared => compared(file, &mut report),
    }
}

pub(super) fn collect_types(file: &SourceFile, workspace: &mut Workspace) {
    for node in file.nodes() {
        if !NodeKind::Struct.is(node) {
            continue;
        }
        let Some(name) = node.child_by_field_name("name") else {
            continue;
        };
        let name = TypeName::new(file.text.of(name));
        if is_wire(file) {
            workspace
                .wire_types
                .entry(package_of(file))
                .or_default()
                .insert(name.clone());
        }
        if newtype_field(node).is_some() {
            workspace.newtypes.insert(name);
        }
    }
    for node in file.nodes() {
        if is_wire(file)
            && NodeKind::Enum.is(node)
            && let Some(name) = node.child_by_field_name("name")
        {
            workspace
                .wire_types
                .entry(package_of(file))
                .or_default()
                .insert(TypeName::new(file.text.of(name)));
        }
    }
}

fn package_of(file: &SourceFile) -> CrateName {
    let inside = file
        .path
        .as_str()
        .strip_prefix(RepoArea::CratesPrefix.name().as_str())
        .and_then(|rest| rest.split('/').next());
    CrateName::new(inside.unwrap_or_default())
}

fn is_wire(file: &SourceFile) -> bool {
    file.path.ends_with(RepoArea::WireFile.name().as_str())
        || file.path.contains(RepoArea::WireDirectory.name().as_str())
}

fn newtype_field(item: Node<'_>) -> Option<Node<'_>> {
    let body = item.child_by_field_name("body")?;
    if !NodeKind::OrderedFieldDeclarationList.is(body) {
        return None;
    }
    let mut cursor = body.walk();
    let types: Vec<Node<'_>> = body.children_by_field_name("type", &mut cursor).collect();
    match types.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

fn comments(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        if matches!(
            NodeKind::of(node),
            Some(NodeKind::LineComment | NodeKind::BlockComment)
        ) {
            let text = file.text.of(node).lines().next().unwrap_or_default();
            report.add(node, format!("`{}`", text.trim()));
        }
    }
}

fn offenders<'tree>(annotation: Node<'tree>, text: &'tree SourceText) -> Vec<Node<'tree>> {
    descendants(annotation)
        .into_iter()
        .filter(|node| match NodeKind::of(*node) {
            Some(NodeKind::PrimitiveType) => !returned_by_closure_bound(*node, text),
            Some(NodeKind::TupleType) => true,
            Some(NodeKind::TypeIdentifier) => text.of(*node) == TypeWord::String.name().as_str(),
            _ => false,
        })
        .collect()
}

fn returned_by_closure_bound(node: Node<'_>, text: &SourceText) -> bool {
    text.of(node) == TypeWord::Boolean.name().as_str()
        && node.parent().is_some_and(|parent| {
            NodeKind::FunctionType.is(parent)
                && parent
                    .child_by_field_name("return_type")
                    .is_some_and(|returned| returned.id() == node.id())
        })
}

fn exempt_function(workspace: &Workspace, file: &SourceFile, function: Node<'_>) -> bool {
    ancestors(function)
        .into_iter()
        .filter(|node| NodeKind::Impl.is(*node))
        .any(|block| {
            block.child_by_field_name("trait").is_some()
                || block.child_by_field_name("type").is_some_and(|name| {
                    workspace
                        .newtypes
                        .contains(&TypeName::new(file.text.of(name)))
                })
        })
}

fn primitives(workspace: &Workspace, file: &SourceFile, report: &mut Report<'_>) {
    if is_wire(file) {
        return;
    }
    for node in file.nodes() {
        if in_test_code(node, &file.text) {
            continue;
        }
        let mut types: Vec<Node<'_>> = Vec::new();
        match NodeKind::of(node) {
            Some(NodeKind::Function | NodeKind::FunctionSignature) => {
                if exempt_function(workspace, file, node) {
                    continue;
                }
                if let Some(parameters) = node.child_by_field_name("parameters") {
                    types.extend(
                        named_children(parameters)
                            .into_iter()
                            .filter(|parameter| NodeKind::Parameter.is(*parameter))
                            .filter_map(|parameter| parameter.child_by_field_name("type")),
                    );
                }
                if let Some(returned) = node.child_by_field_name("return_type")
                    && file.text.of(returned) != TypeWord::Boolean.name().as_str()
                {
                    types.push(returned);
                }
            }
            Some(NodeKind::FieldDeclaration) => {
                types.extend(node.child_by_field_name("type"));
            }
            Some(NodeKind::Constant | NodeKind::Static) => {
                if !exempt_function(workspace, file, node) {
                    types.extend(node.child_by_field_name("type"));
                }
            }
            Some(NodeKind::OrderedFieldDeclarationList) => {
                let newtype = node
                    .parent()
                    .is_some_and(|item| NodeKind::Struct.is(item) && newtype_field(item).is_some());
                if !newtype {
                    let mut cursor = node.walk();
                    types.extend(node.children_by_field_name("type", &mut cursor));
                }
            }
            _ => {}
        }
        for annotation in types {
            for offender in offenders(annotation, &file.text) {
                report.add(
                    offender,
                    format!(
                        "`{}` in `{}`",
                        file.text.of(offender),
                        file.text.of(annotation)
                    ),
                );
            }
        }
    }
}

fn newtype_fields(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        if !NodeKind::Struct.is(node) || newtype_field(node).is_none() {
            continue;
        }
        let Some(body) = node.child_by_field_name("body") else {
            continue;
        };
        if named_children(body)
            .iter()
            .any(|child| NodeKind::VisibilityModifier.is(*child))
        {
            report.add(body, format!("`{}`", file.text.of(body)));
        }
    }
}

fn indexing(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        if NodeKind::IndexExpression.is(node) && !in_test_code(node, &file.text) {
            report.add(node, format!("`{}`", file.text.of(node)));
        }
    }
}

fn absence(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        if in_test_code(node, &file.text) {
            continue;
        }
        let text = file.text.of(node);
        let sentinel = match NodeKind::of(node) {
            Some(NodeKind::BinaryExpression) => {
                node.child_by_field_name("operator")
                    .is_some_and(|operator| {
                        (file.text.of(operator) == Operator::Equal.name().as_str()
                            || file.text.of(operator) == Operator::Unequal.name().as_str())
                            && ["left", "right"].iter().any(|side| {
                                node.child_by_field_name(side).is_some_and(|operand| {
                                    file.text.of(operand) == Sentinel::EmptyString.name().as_str()
                                        || file.text.of(operand)
                                            == Sentinel::NegativeOne.name().as_str()
                                })
                            })
                    })
            }
            Some(NodeKind::ScopedIdentifier) => INTEGER_TYPES.iter().any(|integer| {
                text == format!("{integer}::MAX") || text == format!("{integer}::MIN")
            }),
            _ => false,
        };
        if sentinel {
            report.add(node, format!("`{text}`"));
        }
    }
}

pub(crate) fn named_by_us(file: &SourceFile) -> Vec<Node<'_>> {
    file.nodes()
        .into_iter()
        .filter(|node| !in_test_code(*node, &file.text))
        .flat_map(chosen_names)
        .collect()
}

fn chosen_names(node: Node<'_>) -> Vec<Node<'_>> {
    let dictated = matches!(
        NodeKind::of(node),
        Some(NodeKind::Parameter | NodeKind::Function | NodeKind::TypeItem | NodeKind::Constant)
    ) && ancestors(node)
        .iter()
        .any(|item| NodeKind::Impl.is(*item) && item.child_by_field_name("trait").is_some());
    if dictated {
        Vec::new()
    } else {
        defined_names(node)
    }
}

fn defined_names(node: Node<'_>) -> Vec<Node<'_>> {
    let field = match NodeKind::of(node) {
        Some(
            NodeKind::Function
            | NodeKind::FunctionSignature
            | NodeKind::Struct
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::TypeItem
            | NodeKind::Union
            | NodeKind::EnumVariant
            | NodeKind::Constant
            | NodeKind::Static
            | NodeKind::Module
            | NodeKind::FieldDeclaration
            | NodeKind::MacroDefinition,
        ) => "name",
        Some(NodeKind::Parameter | NodeKind::LetDeclaration) => "pattern",
        _ => return Vec::new(),
    };
    node.child_by_field_name(field)
        .map(|name| {
            descendants(name)
                .into_iter()
                .filter(|part| {
                    matches!(
                        NodeKind::of(*part),
                        Some(
                            NodeKind::Identifier
                                | NodeKind::TypeIdentifier
                                | NodeKind::FieldIdentifier
                        )
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn vocabulary(workspace: &Workspace, file: &SourceFile, report: &mut Report<'_>) {
    for name in named_by_us(file) {
        {
            let identifier = file.text.of(name);
            for word in Word::split(identifier) {
                match workspace.vocabulary.verdict(&word) {
                    Verdict::Known => {}
                    Verdict::Unknown => report.add(
                        name,
                        format!("`{word}` in `{identifier}` is not in vocabulary.txt"),
                    ),
                    Verdict::Synonym(canonical) => report.add(
                        name,
                        format!("`{word}` in `{identifier}` is written `{canonical}`"),
                    ),
                }
            }
        }
    }
}

fn wire_leaks(workspace: &Workspace, file: &SourceFile, report: &mut Report<'_>) {
    if is_wire(file) {
        return;
    }
    for node in file.nodes() {
        if !public(node, &file.text)
            || !matches!(
                NodeKind::of(node),
                Some(
                    NodeKind::Function
                        | NodeKind::Struct
                        | NodeKind::Enum
                        | NodeKind::FieldDeclaration
                )
            )
        {
            continue;
        }
        let signature = node.child_by_field_name("body").map_or(node, |_| node);
        for part in descendants(signature) {
            if NodeKind::TypeIdentifier.is(part)
                && workspace
                    .wire_types
                    .get(&package_of(file))
                    .is_some_and(|names| names.contains(&TypeName::new(file.text.of(part))))
            {
                report.add(part, format!("`{}` is a wire type", file.text.of(part)));
            }
        }
    }
}

fn domain_io(file: &SourceFile, report: &mut Report<'_>) {
    if !file.path.starts_with(RepoArea::DomainCrate.name().as_str()) {
        return;
    }
    for node in file.nodes() {
        let text = file.text.of(node);
        let io = match NodeKind::of(node) {
            Some(NodeKind::UseDeclaration | NodeKind::ScopedIdentifier) => {
                DOMAIN_IO.iter().any(|prefix| {
                    text.starts_with(prefix.as_str()) || text.starts_with(&format!("use {prefix}"))
                })
            }
            Some(NodeKind::MacroInvocation) => {
                node.child_by_field_name("macro").is_some_and(|name| {
                    PRINTING
                        .iter()
                        .any(|macro_name| macro_name.as_str() == file.text.of(name))
                })
            }
            _ => false,
        };
        if io {
            report.add(
                node,
                format!("`{}`", text.lines().next().unwrap_or_default()),
            );
        }
    }
}

fn suppressions(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        if !matches!(
            NodeKind::of(node),
            Some(NodeKind::AttributeItem | NodeKind::InnerAttributeItem)
        ) {
            continue;
        }
        let Some(attribute) = node.named_child(0) else {
            continue;
        };
        let name = attribute
            .named_child(0)
            .map(|path| file.text.of(path))
            .unwrap_or_default();
        let arguments = attribute
            .child_by_field_name("arguments")
            .map(|arguments| file.text.of(arguments))
            .unwrap_or_default();
        if name == SuppressionAttribute::Allow.name().as_str() {
            report.add(node, format!("`{}`", file.text.of(node)));
        } else if name == SuppressionAttribute::Expect.name().as_str() {
            if !arguments.contains(SuppressionAttribute::Reason.name().as_str()) {
                report.add(node, "no reason given".to_owned());
            }
            for lint in LintName::list(arguments) {
                if !arch::expected(&file.path, &lint) {
                    report.add(node, format!("`{lint}` is not listed for {}", file.path));
                }
            }
        }
    }
}

fn test_registry(file: &SourceFile, report: &mut Report<'_>) {
    if !arch::scenario_file(&file.path) {
        return;
    }
    for node in file.nodes() {
        if NodeKind::AttributeItem.is(node)
            && file.text.of(node) == TestAttribute::Test.name().as_str()
        {
            report.add(node, "a hand-registered #[test]".to_owned());
        }
    }
}

fn aliases(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        let associated = ancestors(node)
            .iter()
            .any(|item| matches!(NodeKind::of(*item), Some(NodeKind::Impl | NodeKind::Trait)));
        if NodeKind::TypeItem.is(node) && !associated && !in_test_code(node, &file.text) {
            report.add(node, format!("`{}`", file.text.of(node)));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Home,
    Elsewhere,
    Outside,
}

fn gui_module(file: &SourceFile, home: Literal) -> Place {
    if !file
        .path
        .starts_with(RepoArea::GuiCrateSource.name().as_str())
    {
        Place::Outside
    } else if file.path.ends_with(&format!("/{home}.rs"))
        || file.path.contains(&format!("/{home}/"))
    {
        Place::Home
    } else {
        Place::Elsewhere
    }
}

fn gui_calls(file: &SourceFile, report: &mut Report<'_>, home: Literal, methods: &[Literal]) {
    if gui_module(file, home) != Place::Elsewhere {
        return;
    }
    for node in file.nodes() {
        if !NodeKind::CallExpression.is(node) || in_test_code(node, &file.text) {
            continue;
        }
        let called = node
            .child_by_field_name("function")
            .filter(|function| NodeKind::FieldExpression.is(*function))
            .and_then(|function| function.child_by_field_name("field"))
            .map(|field| file.text.of(field))
            .unwrap_or_default();
        if methods.iter().any(|method| method.as_str() == called) {
            report.add(node, format!("`.{called}(`"));
        }
    }
}

fn gui_literals(file: &SourceFile, report: &mut Report<'_>, home: Literal, calls: &[Literal]) {
    if gui_module(file, home) != Place::Elsewhere {
        return;
    }
    for node in file.nodes() {
        if !NodeKind::CallExpression.is(node) || in_test_code(node, &file.text) {
            continue;
        }
        let called = node
            .child_by_field_name("function")
            .map(|function| file.text.of(function))
            .unwrap_or_default();
        let literal = node
            .child_by_field_name("arguments")
            .is_some_and(|arguments| {
                named_children(arguments).iter().any(|argument| {
                    matches!(
                        NodeKind::of(*argument),
                        Some(
                            NodeKind::IntegerLiteral
                                | NodeKind::FloatLiteral
                                | NodeKind::StringLiteral
                                | NodeKind::UnaryExpression
                        )
                    )
                })
            });
        if literal && calls.iter().any(|call| called.ends_with(call.as_str())) {
            report.add(node, format!("`{}`", file.text.of(node)));
        }
    }
}

fn gui_paths(file: &SourceFile, report: &mut Report<'_>, home: Literal, paths: &[Literal]) {
    if gui_module(file, home) != Place::Elsewhere {
        return;
    }
    for node in file.nodes() {
        if !NodeKind::ScopedIdentifier.is(node) || in_test_code(node, &file.text) {
            continue;
        }
        let text = file.text.of(node);
        if paths.iter().any(|path| text.starts_with(path.as_str())) {
            report.add(node, format!("`{text}`"));
        }
    }
}

fn compared(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        if !matches!(
            NodeKind::of(node),
            Some(NodeKind::StringLiteral | NodeKind::RawStringLiteral)
        ) || in_test_code(node, &file.text)
        {
            continue;
        }
        if let Some(place) = compared_place(file, node) {
            report.add(node, format!("`{}` {place}", file.text.of(node)));
        }
    }
}

fn compared_place(file: &SourceFile, literal: Node<'_>) -> Option<Message> {
    let parent = literal.parent()?;
    let pattern = matches!(
        NodeKind::of(parent),
        Some(
            NodeKind::MatchPattern
                | NodeKind::OrPattern
                | NodeKind::TuplePattern
                | NodeKind::SlicePattern
                | NodeKind::TupleStructPattern
                | NodeKind::FieldPattern
                | NodeKind::ReferencePattern
                | NodeKind::CapturedPattern
        )
    ) || parent
        .child_by_field_name("pattern")
        .is_some_and(|pattern| pattern.id() == literal.id());
    if pattern {
        return Some(Message::new("as a pattern"));
    }
    if NodeKind::BinaryExpression.is(parent) {
        let operator = parent.child_by_field_name("operator")?;
        let equality = file.text.of(operator) == Operator::Equal.name().as_str()
            || file.text.of(operator) == Operator::Unequal.name().as_str();
        return (equality && file.text.of(literal) != Sentinel::EmptyString.name().as_str())
            .then(|| Message::new(format!("compared with {}", file.text.of(operator))));
    }
    if NodeKind::Arguments.is(parent) {
        let first = named_children(parent).first().copied()?;
        let function = parent.parent()?.child_by_field_name("function")?;
        let method = function.child_by_field_name("field")?;
        let pattern_method = PatternMethod::ALL
            .iter()
            .any(|name| file.text.of(method) == name.name().as_str());
        return (first.id() == literal.id()
            && NodeKind::FieldExpression.is(function)
            && pattern_method)
            .then(|| Message::new(format!("as the pattern of {}", file.text.of(method))));
    }
    in_matches_pattern(file, literal).then(|| Message::new("as a matches! pattern"))
}

fn in_matches_pattern(file: &SourceFile, literal: Node<'_>) -> bool {
    let Some(tree) = ancestors(literal).into_iter().find(|node| {
        node.parent()
            .is_some_and(|parent| NodeKind::MacroInvocation.is(parent))
    }) else {
        return false;
    };
    let named_matches = tree
        .parent()
        .and_then(|invocation| invocation.child_by_field_name("macro"))
        .is_some_and(|name| file.text.of(name) == MacroName::Matches.name().as_str());
    let mut cursor = tree.walk();
    let comma = tree
        .children(&mut cursor)
        .find(|child| file.text.of(*child) == Punctuation::Comma.name().as_str());
    named_matches && comma.is_some_and(|comma| literal.start_byte() > comma.start_byte())
}
