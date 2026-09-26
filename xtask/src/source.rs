use std::fs;
use std::iter;

use ignore::WalkBuilder;
use tree_sitter::{Node, Parser, Tree};

use crate::text::{LineNumber, Message, RepoPath, Root};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zone {
    Strict,
    Test,
}

impl Zone {
    pub(crate) fn of(path: &RepoPath) -> Self {
        if path.starts_with("tests/") || path.contains("/tests/") || path.ends_with("/tests.rs") {
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
            match node.kind() {
                "attribute_item" => out.push(self.of(node)),
                "line_comment" | "block_comment" => {}
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
    named_children(node)
        .iter()
        .any(|child| child.kind() == "visibility_modifier" && text.of(*child) == "pub")
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
        .filter(|item| matches!(item.kind(), "mod_item" | "function_item"))
        .any(|item| {
            text.attributes(item)
                .iter()
                .any(|attribute| attribute.contains("cfg(test)") || *attribute == "#[test]")
        })
}

pub(crate) fn workspace_files(root: &Root) -> Vec<RepoPath> {
    let mut out: Vec<RepoPath> = WalkBuilder::new(root.path())
        .hidden(false)
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some(".git" | ".jj" | "target" | ".codemap-cache")
            )
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
        .filter(|path| path.ends_with(".rs"))
        .filter(|path| {
            ["crates/", "xtask/"]
                .iter()
                .any(|prefix| path.starts_with(prefix))
        })
        .collect()
}
