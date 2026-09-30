use std::fs;
use std::path::PathBuf;
use std::process;

use super::*;

const SAMPLE: &str = "codemap layout 1
down 820
  right 220
    panel Paths* Symbols Files
    right 740
      panel Path Diff Graph* Source Search
      panel References*
  panel Console*
";

fn scratch(name: &str) -> PathBuf {
    let folder = env::temp_dir().join(format!("io-layout-{name}-{}", process::id()));
    drop(fs::remove_dir_all(&folder));
    folder
}

#[test]
fn a_layout_reads_and_prints_back_byte_for_byte() {
    let wire = wire::parse(SAMPLE).unwrap();
    let layout = convert::layout(&wire).unwrap();
    assert_eq!(wire::print(&convert::wire(&layout)), SAMPLE);
}

#[test]
fn a_save_creates_the_folder_and_a_load_returns_the_same_tree() {
    let folder = scratch("round");
    let store = LayoutStore::at(folder.join("codemap").join("layout"));
    let layout = convert::layout(&wire::parse(SAMPLE).unwrap()).unwrap();
    store.save(&layout).unwrap();
    assert_eq!(fs::read_to_string(store.path()).unwrap(), SAMPLE);
    assert_eq!(store.load(), Some(layout));
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn anything_malformed_reads_as_no_layout() {
    for text in [
        "",
        "codemap layout 2\npanel Paths\n",
        "codemap layout 1\nsideways 500\n  panel A\n  panel B\n",
        "codemap layout 1\ndown 1001\n  panel A\n  panel B\n",
        "codemap layout 1\ndown 500\n  panel A\n",
        "codemap layout 1\npanel A\npanel B\n",
        "codemap layout 1\n panel A\n",
        "codemap layout 1\npanel A-B\n",
    ] {
        let parsed = wire::parse(text).and_then(|wire| convert::layout(&wire));
        assert_eq!(parsed, None, "{text:?}");
    }
    let missing = LayoutStore::at(scratch("missing").join("layout"));
    assert_eq!(missing.load(), None);
}
