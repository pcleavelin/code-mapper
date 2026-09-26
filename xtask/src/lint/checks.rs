use tree_sitter::Node;

use crate::arch;
use crate::lint::{Finding, Rule, Workspace};
use crate::source::{
    SourceFile, SourceText, ancestors, descendants, in_test_code, line_of, named_children, public,
};
use crate::text::{LintName, Literal, Message, TypeName, Word};
use crate::vocabulary::Verdict;

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
    }
}

pub(super) fn collect_types(file: &SourceFile, workspace: &mut Workspace) {
    for node in file.nodes() {
        if node.kind() != "struct_item" {
            continue;
        }
        let Some(name) = node.child_by_field_name("name") else {
            continue;
        };
        let name = TypeName::new(file.text.of(name));
        if is_wire(file) {
            workspace.wire_types.insert(name.clone());
        }
        if newtype_field(node).is_some() {
            workspace.newtypes.insert(name);
        }
    }
    for node in file.nodes() {
        if is_wire(file)
            && node.kind() == "enum_item"
            && let Some(name) = node.child_by_field_name("name")
        {
            workspace
                .wire_types
                .insert(TypeName::new(file.text.of(name)));
        }
    }
}

fn is_wire(file: &SourceFile) -> bool {
    file.path.ends_with("/wire.rs") || file.path.contains("/wire/")
}

fn newtype_field(item: Node<'_>) -> Option<Node<'_>> {
    let body = item.child_by_field_name("body")?;
    if body.kind() != "ordered_field_declaration_list" {
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
        if matches!(node.kind(), "line_comment" | "block_comment") {
            let text = file.text.of(node).lines().next().unwrap_or_default();
            report.add(node, format!("`{}`", text.trim()));
        }
    }
}

fn offenders<'tree>(annotation: Node<'tree>, text: &'tree SourceText) -> Vec<Node<'tree>> {
    descendants(annotation)
        .into_iter()
        .filter(|node| match node.kind() {
            "primitive_type" | "tuple_type" => true,
            "type_identifier" => text.of(*node) == "String",
            _ => false,
        })
        .collect()
}

fn exempt_function(workspace: &Workspace, file: &SourceFile, function: Node<'_>) -> bool {
    ancestors(function)
        .into_iter()
        .filter(|node| node.kind() == "impl_item")
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
        match node.kind() {
            "function_item" | "function_signature_item" => {
                if exempt_function(workspace, file, node) {
                    continue;
                }
                if let Some(parameters) = node.child_by_field_name("parameters") {
                    types.extend(
                        named_children(parameters)
                            .into_iter()
                            .filter(|parameter| parameter.kind() == "parameter")
                            .filter_map(|parameter| parameter.child_by_field_name("type")),
                    );
                }
                if let Some(returned) = node.child_by_field_name("return_type")
                    && file.text.of(returned) != "bool"
                {
                    types.push(returned);
                }
            }
            "field_declaration" | "const_item" | "static_item" => {
                types.extend(node.child_by_field_name("type"));
            }
            "ordered_field_declaration_list" => {
                let newtype = node.parent().is_some_and(|item| {
                    item.kind() == "struct_item" && newtype_field(item).is_some()
                });
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
        if node.kind() != "struct_item" || newtype_field(node).is_none() {
            continue;
        }
        let Some(body) = node.child_by_field_name("body") else {
            continue;
        };
        if named_children(body)
            .iter()
            .any(|child| child.kind() == "visibility_modifier")
        {
            report.add(body, format!("`{}`", file.text.of(body)));
        }
    }
}

fn indexing(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        if node.kind() == "index_expression" && !in_test_code(node, &file.text) {
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
        let sentinel = match node.kind() {
            "binary_expression" => node
                .child_by_field_name("operator")
                .is_some_and(|operator| {
                    matches!(file.text.of(operator), "==" | "!=")
                        && ["left", "right"].iter().any(|side| {
                            node.child_by_field_name(side).is_some_and(|operand| {
                                matches!(file.text.of(operand), "\"\"" | "-1")
                            })
                        })
                }),
            "scoped_identifier" => INTEGER_TYPES.iter().any(|integer| {
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
    let dictated = matches!(node.kind(), "parameter" | "function_item")
        && ancestors(node)
            .iter()
            .any(|item| item.kind() == "impl_item" && item.child_by_field_name("trait").is_some());
    if dictated {
        Vec::new()
    } else {
        defined_names(node)
    }
}

fn defined_names(node: Node<'_>) -> Vec<Node<'_>> {
    let field = match node.kind() {
        "function_item"
        | "function_signature_item"
        | "struct_item"
        | "enum_item"
        | "trait_item"
        | "type_item"
        | "union_item"
        | "enum_variant"
        | "const_item"
        | "static_item"
        | "mod_item"
        | "field_declaration"
        | "macro_definition" => "name",
        "parameter" | "let_declaration" => "pattern",
        _ => return Vec::new(),
    };
    node.child_by_field_name(field)
        .map(|name| {
            descendants(name)
                .into_iter()
                .filter(|part| {
                    matches!(
                        part.kind(),
                        "identifier" | "type_identifier" | "field_identifier"
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
                node.kind(),
                "function_item" | "struct_item" | "enum_item" | "field_declaration"
            )
        {
            continue;
        }
        let signature = node.child_by_field_name("body").map_or(node, |_| node);
        for part in descendants(signature) {
            if part.kind() == "type_identifier"
                && workspace
                    .wire_types
                    .contains(&TypeName::new(file.text.of(part)))
            {
                report.add(part, format!("`{}` is a wire type", file.text.of(part)));
            }
        }
    }
}

fn domain_io(file: &SourceFile, report: &mut Report<'_>) {
    if !file.path.starts_with("crates/domain/") {
        return;
    }
    for node in file.nodes() {
        let text = file.text.of(node);
        let io = match node.kind() {
            "use_declaration" | "scoped_identifier" => DOMAIN_IO.iter().any(|prefix| {
                text.starts_with(prefix.as_str()) || text.starts_with(&format!("use {prefix}"))
            }),
            "macro_invocation" => node.child_by_field_name("macro").is_some_and(|name| {
                PRINTING
                    .iter()
                    .any(|macro_name| macro_name.as_str() == file.text.of(name))
            }),
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
        if !matches!(node.kind(), "attribute_item" | "inner_attribute_item") {
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
        match name {
            "allow" => report.add(node, format!("`{}`", file.text.of(node))),
            "expect" => {
                if !arguments.contains("reason") {
                    report.add(node, "no reason given".to_owned());
                }
                for lint in LintName::list(arguments) {
                    if !arch::expected(&file.path, &lint) {
                        report.add(node, format!("`{lint}` is not listed for {}", file.path));
                    }
                }
            }
            _ => {}
        }
    }
}

fn test_registry(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        if node.kind() == "attribute_item" && file.text.of(node) == "#[test]" {
            report.add(node, "a hand-registered #[test]".to_owned());
        }
    }
}

fn aliases(file: &SourceFile, report: &mut Report<'_>) {
    for node in file.nodes() {
        if node.kind() == "type_item" && !in_test_code(node, &file.text) {
            report.add(node, format!("`{}`", file.text.of(node)));
        }
    }
}
