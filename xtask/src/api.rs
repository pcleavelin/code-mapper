use std::fs;

use tree_sitter::Node;

use crate::files::write;
use crate::manifest::Manifest;
use crate::source::{Extension, NodeKind, SourceFile, public, rust_sources};
use crate::text::{CrateName, Literal, Message, RepoPath, Root};

const ITEM_KINDS: [NodeKind; 8] = [
    NodeKind::Function,
    NodeKind::Struct,
    NodeKind::Enum,
    NodeKind::Trait,
    NodeKind::Constant,
    NodeKind::Static,
    NodeKind::Module,
    NodeKind::MacroDefinition,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModuleStem {
    Library,
}

impl ModuleStem {
    const fn name(self) -> Literal {
        match self {
            Self::Library => Literal::new("lib"),
        }
    }
}

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
                if NodeKind::of(node).is_some_and(|kind| ITEM_KINDS.contains(&kind))
                    && public(node, &file.text)
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
        .trim_end_matches(Extension::Rust.name().as_str())
        .trim_end_matches(ModuleStem::Library.name().as_str())
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
    let text = match (NodeKind::of(node), node.child_by_field_name("body")) {
        (Some(NodeKind::Function | NodeKind::Trait | NodeKind::Module), Some(body)) => whole
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
