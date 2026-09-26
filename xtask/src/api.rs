use std::fs;

use tree_sitter::Node;

use crate::files::write;
use crate::manifest::Manifest;
use crate::source::{SourceFile, public, rust_sources};
use crate::text::{CrateName, Literal, Message, RepoPath, Root};

const ITEMS: [Literal; 8] = [
    Literal::new("function_item"),
    Literal::new("struct_item"),
    Literal::new("enum_item"),
    Literal::new("trait_item"),
    Literal::new("const_item"),
    Literal::new("static_item"),
    Literal::new("mod_item"),
    Literal::new("macro_definition"),
];

#[derive(Debug)]
pub(crate) struct Surface {
    pub(crate) package: CrateName,
    pub(crate) file: RepoPath,
    pub(crate) listing: Message,
}

pub(crate) fn surfaces(root: &Root) -> Result<Vec<Surface>, Message> {
    let workspace = Manifest::load(root, &RepoPath::new("Cargo.toml"))?;
    let mut out = Vec::new();
    for member in workspace.members(root) {
        let library = RepoPath::new(&format!("{member}/src/lib.rs"));
        if !root.join(&library).is_file() {
            continue;
        }
        let manifest = Manifest::load(root, &RepoPath::new(&format!("{member}/Cargo.toml")))?;
        let Some(package) = manifest.package() else {
            continue;
        };
        let source_prefix = RepoPath::new(&format!("{member}/src/"));
        let mut listing = Message::default();
        for path in rust_sources(root)
            .into_iter()
            .filter(|path| path.starts_with(source_prefix.as_str()))
        {
            let module = module_of(&path, &source_prefix);
            let file = SourceFile::load(root, path)?;
            for node in file.nodes() {
                if ITEMS.iter().any(|item| item.as_str() == node.kind()) && public(node, &file.text)
                {
                    listing.push_line(&format!("{module}{}", signature(node, &file)));
                }
            }
        }
        out.push(Surface {
            file: RepoPath::new(&format!("api/{package}.api")),
            package,
            listing,
        });
    }
    Ok(out)
}

fn module_of(path: &RepoPath, source_prefix: &RepoPath) -> Message {
    let inner = path
        .as_str()
        .trim_start_matches(source_prefix.as_str())
        .trim_end_matches(".rs")
        .trim_end_matches("lib")
        .trim_end_matches('/')
        .replace('/', "::");
    if inner.is_empty() {
        Message::new("")
    } else {
        Message::new(format!("{inner}::"))
    }
}

fn signature(node: Node<'_>, file: &SourceFile) -> Message {
    let whole = file.text.of(node);
    let text = match (node.kind(), node.child_by_field_name("body")) {
        ("function_item" | "trait_item" | "mod_item", Some(body)) => whole
            .get(..body.start_byte().saturating_sub(node.start_byte()))
            .unwrap_or(whole),
        _ => whole,
    };
    Message::new(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

pub(crate) fn check(root: &Root) -> Result<(), Message> {
    let mut report = Message::default();
    for surface in surfaces(root)? {
        let recorded = fs::read_to_string(root.join(&surface.file)).unwrap_or_default();
        if recorded != surface.listing.as_str() {
            report.push_line(&format!(
                "{}: the public API of {} changed; run `cargo xtask api` and commit {}",
                surface.file, surface.package, surface.file
            ));
        }
    }
    if report.is_empty() {
        Ok(())
    } else {
        Err(report)
    }
}

pub(crate) fn record(root: &Root) -> Result<(), Message> {
    for surface in surfaces(root)? {
        write(&root.join(&surface.file), &surface.listing)?;
    }
    Ok(())
}
