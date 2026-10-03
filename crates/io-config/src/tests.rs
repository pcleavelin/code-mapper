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
    let folder = env::temp_dir().join(format!("io-config-{name}-{}", process::id()));
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

const SETTINGS_SAMPLE: &str = "codemap settings 1
theme light
font JetBrains Mono
size 17
graph-direction down
";

#[test]
fn settings_read_and_print_back_byte_for_byte() {
    let wire = wire::parse_settings(SETTINGS_SAMPLE).unwrap();
    let settings = convert::settings(&wire);
    assert_eq!(settings.theme(), domain::Theme::Light);
    assert_eq!(
        settings.font().map(domain::FontFamily::as_str),
        Some("JetBrains Mono")
    );
    assert_eq!(settings.size().get(), 17);
    assert_eq!(settings.graph_direction(), domain::SplitDirection::Down);
    assert_eq!(
        wire::print_settings(&convert::wire_settings(&settings)),
        SETTINGS_SAMPLE
    );
}

#[test]
fn the_bundled_font_is_written_as_no_font_line() {
    let settings = domain::Settings::default();
    assert_eq!(
        wire::print_settings(&convert::wire_settings(&settings)),
        "codemap settings 1\ntheme dark\nsize 14\ngraph-direction right\n"
    );
}

#[test]
fn a_bad_line_keeps_that_setting_at_its_default_and_the_rest_as_written() {
    let text = "codemap settings 1\ntheme purple\nsize 99\ncolour red\ngraph-direction down\n";
    let settings = convert::settings(&wire::parse_settings(text).unwrap());
    assert_eq!(
        settings,
        domain::Settings::default().with_graph_direction(domain::SplitDirection::Down)
    );
}

#[test]
fn another_header_reads_as_no_settings() {
    assert_eq!(
        wire::parse_settings("codemap settings 2\ntheme light\n"),
        None
    );
    assert_eq!(wire::parse_settings(""), None);
    let missing = SettingsStore::at(scratch("settings-missing").join("settings"));
    assert_eq!(missing.load(), None);
}

#[test]
fn a_settings_save_creates_the_folder_and_a_load_returns_the_same_settings() {
    let folder = scratch("settings-round");
    let store = SettingsStore::at(folder.join("codemap").join("settings"));
    let settings = convert::settings(&wire::parse_settings(SETTINGS_SAMPLE).unwrap());
    store.save(&settings).unwrap();
    assert_eq!(fs::read_to_string(store.path()).unwrap(), SETTINGS_SAMPLE);
    assert_eq!(store.load(), Some(settings));
    fs::remove_dir_all(folder).unwrap();
}
