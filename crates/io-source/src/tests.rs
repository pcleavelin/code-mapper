use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{env, process};

use super::*;

fn put(root: &Path, path: &str, bytes: &[u8]) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(full)
        .unwrap()
        .write_all(bytes)
        .unwrap();
}

fn scratch(name: &str) -> PathBuf {
    let root = env::temp_dir().join(format!("io_source_{name}_{}", process::id()));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn the_walk_skips_ignored_hidden_and_binary_files() {
    let root = scratch("walk");
    put(&root, "src/a.rs", b"fn a() {\n\tb();\n}\n");
    put(&root, ".gitignore", b"build/\n");
    put(&root, "build/gen.rs", b"fn ignored() {}\n");
    put(&root, ".hidden/x.rs", b"fn hidden() {}\n");
    put(&root, "data.bin", b"\0\x01binary");
    put(&root, "late.bin", &[b"x".repeat(2000), vec![0]].concat());
    put(&root, "big.txt", &b"y".repeat((4 << 20) + 1));
    put(&root, "edge.txt", &b"z".repeat(4 << 20));
    let mut read = walk(&Root::new(&root));
    read.sort_by(|one, other| one.path.cmp(&other.path));
    let paths: Vec<&str> = read.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["edge.txt", "late.bin", "src/a.rs"]);
    let source = read
        .iter()
        .find(|file| file.path.as_str() == "src/a.rs")
        .unwrap();
    assert_eq!(source.contents.as_str(), "fn a() {\n    b();\n}\n");
    assert_eq!(
        source.contents.hash(),
        TextHash::of(b"fn a() {\n    b();\n}\n")
    );
    assert_eq!(source.contents.text().whole_hash(), source.contents.hash());
    assert!(source.modified.is_some());
    let stamps = Stamps::new(
        read.iter()
            .map(|file| Stamp {
                path: file.path.clone(),
                modified: file.modified,
            })
            .collect(),
    );
    assert!(!stamps.changed(&Root::new(&root)));
    fs::rename(root.join("src/a.rs"), root.join("moved.txt")).unwrap();
    assert!(stamps.changed(&Root::new(&root)));
    drop(fs::remove_dir_all(&root));
}

#[test]
fn a_file_outside_the_root_reads_whole_or_empty() {
    let root = scratch("outside");
    put(&root, "x.rs", b"\tfn x() {}\n");
    assert_eq!(read_outside(&root.join("x.rs")).as_str(), "    fn x() {}\n");
    assert_eq!(read_outside(&root.join("missing.rs")).as_str(), "");
    drop(fs::remove_dir_all(&root));
}
