use std::env;
use std::fs;
use std::process;

use crate::{remove, write};

#[test]
fn write_replaces_the_whole_file_and_leaves_no_temporary() {
    let directory = env::temp_dir().join(format!("io-store-test-{}", process::id()));
    fs::create_dir_all(&directory).unwrap();
    let target = directory.join("out.txt");
    write(&target, "first version, longer").unwrap();
    write(&target, b"second").unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "second");
    let names: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1);
    remove(&target).unwrap();
    assert!(!target.exists());
    fs::remove_dir(&directory).unwrap();
}
