use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;

use domain::{
    Backend, ByteOffset, Call, Depth, FileText, Highlight, HighlightClass, Imports, Index, Line,
    Location, Qualifier, RelativePath, Root, Scope, SourceFile, Span, Symbol, SymbolKind,
    SymbolName, TextHash, TypeName,
};

use super::*;

const LEGACY: &[u8] = include_bytes!("tests/fixture.cache");

fn scratch(name: &str) -> PathBuf {
    let root = env::temp_dir().join(format!("io_cache_{name}_{}", process::id()));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(&root).unwrap();
    root
}

fn put(path: &Path, bytes: &[u8]) {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

fn index_at(root: &Path, imports: Imports) -> Index {
    let mut index = Index::new(Root::new(root));
    index.push(SourceFile::new(
        RelativePath::new("a.rs"),
        FileText::from("fn a() {}"),
        vec![Vec::new()],
        Vec::new(),
        imports,
        TextHash::new(1),
        Backend::TreeSitter,
    ));
    index
}

#[test]
fn the_same_index_writes_the_same_bytes() {
    let names: Vec<(String, String)> = (0..32)
        .map(|count| (format!("name{count}"), format!("module{count}")))
        .collect();
    let mut forward = Imports::new();
    for (name, module) in &names {
        forward.insert(name, module);
    }
    let mut backward = Imports::new();
    for (name, module) in names.iter().rev() {
        backward.insert(name, module);
    }
    let base = scratch("bytes");
    let (one, two) = (base.join("one"), base.join("two"));
    for dir in [&one, &two] {
        fs::create_dir_all(dir).unwrap();
    }
    CacheStore::save(&index_at(&one, forward)).unwrap();
    CacheStore::save(&index_at(&two, backward)).unwrap();
    let bytes = |dir: &Path| fs::read(dir.join(".codemap-cache")).unwrap();
    assert_eq!(bytes(&one), bytes(&two));
    drop(fs::remove_dir_all(&base));
}

#[test]
fn the_layout_is_the_legacy_one() {
    let root = scratch("layout");
    let mut symbol = Symbol::new(
        SymbolName::new("go"),
        SymbolKind::new("function_item"),
        Span::new(Line::new(2), Line::new(4)).unwrap(),
        Depth::new(1),
        Some(TypeName::new("A")),
        vec![
            Call {
                name: SymbolName::new("b"),
                qualifier: Qualifier::Plain,
            },
            Call {
                name: SymbolName::new("c"),
                qualifier: Qualifier::SelfReference,
            },
            Call {
                name: SymbolName::new("d"),
                qualifier: Qualifier::Named(Scope::new("m")),
            },
        ],
    );
    symbol.set_targets(vec![Location {
        file: RelativePath::new("t.rs"),
        line: Line::new(7),
    }]);
    let mut imports = Imports::new();
    imports.insert("x", "y");
    let mut index = Index::new(Root::new(&root));
    index.push(SourceFile::new(
        RelativePath::new("a.rs"),
        FileText::from("a\n"),
        vec![vec![Highlight {
            start: ByteOffset::new(0),
            end: ByteOffset::new(1),
            class: HighlightClass::Comment,
        }]],
        vec![symbol],
        imports,
        TextHash::new(0x0102_0304_0506_0708),
        Backend::Server,
    ));
    CacheStore::save(&index).unwrap();
    let mut expected: Vec<u8> = b"CMCH\x02\0\0\0\x01\0\0\0".to_vec();
    let text = |bytes: &mut Vec<u8>, text: &str| {
        bytes.extend_from_slice(&u32::try_from(text.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(text.as_bytes());
    };
    let number = |bytes: &mut Vec<u8>, number: u32| bytes.extend_from_slice(&number.to_le_bytes());
    text(&mut expected, "a.rs");
    expected.extend_from_slice(&0x0102_0304_0506_0708_u64.to_le_bytes());
    expected.push(1);
    number(&mut expected, 1);
    text(&mut expected, "go");
    text(&mut expected, "function_item");
    number(&mut expected, 2);
    number(&mut expected, 4);
    expected.push(1);
    text(&mut expected, "A");
    number(&mut expected, 3);
    text(&mut expected, "b");
    expected.push(0);
    text(&mut expected, "c");
    expected.push(1);
    text(&mut expected, "d");
    expected.push(2);
    text(&mut expected, "m");
    number(&mut expected, 1);
    text(&mut expected, "t.rs");
    number(&mut expected, 7);
    number(&mut expected, 0);
    number(&mut expected, 1);
    text(&mut expected, "x");
    text(&mut expected, "y");
    number(&mut expected, 1);
    number(&mut expected, 1);
    number(&mut expected, 0);
    number(&mut expected, 1);
    expected.push(3);
    assert_eq!(fs::read(root.join(".codemap-cache")).unwrap(), expected);

    let mut cache = CacheStore::load(&Root::new(&root)).unwrap();
    let loaded = cache.take(&RelativePath::new("a.rs")).unwrap();
    assert!(cache.is_empty());
    let original = index
        .file(index.find_file(&RelativePath::new("a.rs")).unwrap())
        .unwrap();
    assert_eq!(
        loaded.symbols().collect::<Vec<_>>(),
        original.symbols().collect::<Vec<_>>()
    );
    assert_eq!(loaded.highlights(), original.highlights());
    assert_eq!(loaded.imports(), original.imports());
    assert_eq!(loaded.hash(), original.hash());
    assert_eq!(loaded.backend(), Backend::Server);
    assert_eq!(loaded.text().all().len(), 0);
    drop(fs::remove_dir_all(&root));
}

#[test]
fn a_legacy_cache_loads_and_writes_back_the_same_bytes() {
    let root = scratch("legacy");
    put(&root.join(".codemap-cache"), LEGACY);
    let cache = CacheStore::load(&Root::new(&root)).unwrap();
    let paths: Vec<&str> = cache.files().map(|file| file.path().as_str()).collect();
    assert_eq!(
        paths,
        [
            "README.md",
            "c/lib.c",
            "c/lib.h",
            "src/main.rs",
            "src/shapes.rs",
            "src/store.rs",
            "tools/helpers.py",
            "tools/stats.py"
        ]
    );
    let mut index = Index::new(Root::new(&root));
    for file in cache.files() {
        index.push(file.clone());
    }
    fs::remove_dir_all(&root).unwrap();
    fs::create_dir_all(&root).unwrap();
    CacheStore::save(&index).unwrap();
    assert_eq!(fs::read(root.join(".codemap-cache")).unwrap(), LEGACY);
    drop(fs::remove_dir_all(&root));
}

#[test]
fn a_foreign_or_truncated_file_is_no_cache() {
    let root = scratch("foreign");
    put(&root.join(".codemap-cache"), b"CMCH\x01\0\0\0\0\0\0\0");
    assert!(CacheStore::load(&Root::new(&root)).is_none());
    put(&root.join(".codemap-cache"), &LEGACY[..LEGACY.len() - 1]);
    assert!(CacheStore::load(&Root::new(&root)).is_none());
    fs::remove_dir_all(&root).unwrap();
    assert!(CacheStore::load(&Root::new(&root)).is_none());
}
