use std::env;
use std::fs;
use std::path::Path;
use std::process;

use domain::{Index, Map, Root, SplitDirection};
use io_map::MapStore;

use super::*;
use crate::model::{Model, Readable};
use crate::theme::{BACKGROUND, SELECTED, TEXT};

fn app() -> App {
    let root = Root::new(Path::new("/nowhere"));
    let model = Model::new(
        Index::default(),
        Map::default(),
        MapStore::new(&root),
        Readable::Reads,
    );
    App::of_model(model, &root)
}

fn brightness(color: ui::Color) -> u32 {
    u32::from(color.red()) + u32::from(color.green()) + u32::from(color.blue())
}

#[test]
fn choosing_top_to_bottom_turns_the_graph_once_and_is_kept_as_the_setting() {
    let mut app = app();
    app.apply(Action::Settings(SettingsAct::Graph(Direction::Down)));
    assert_eq!(app.model.graph.direction(), Direction::Down);
    app.apply(Action::Settings(SettingsAct::Graph(Direction::Down)));
    assert_eq!(app.model.graph.direction(), Direction::Down);
    assert_eq!(app.model.settings.graph_direction(), SplitDirection::Down);
}

#[test]
fn a_changed_setting_is_written_to_the_settings_file_and_an_unchanged_one_is_not() {
    let folder = env::temp_dir().join(format!("gui-settings-{}", process::id()));
    drop(fs::remove_dir_all(&folder));
    let file = folder.join("settings");
    let mut app = app();
    app.settings = KeptSettings::load(Some(SettingsStore::at(file.clone())));
    app.keep_settings();
    assert!(!file.exists());
    app.apply(Action::Settings(SettingsAct::Theme(Theme::Light)));
    app.apply(Action::Settings(SettingsAct::Size(
        BaseFontSize::default().larger(),
    )));
    app.keep_settings();
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "codemap settings 1\ntheme light\nsize 15\ngraph-direction right\n"
    );
    fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_light_theme_puts_dark_text_on_a_light_background_and_keeps_each_tint() {
    let light = theme::repaint(Theme::Light);
    assert!(brightness(light.paint(TEXT)) < brightness(light.paint(BACKGROUND)));
    assert!(brightness(light.paint(BACKGROUND)) > brightness(BACKGROUND));
    let tint = SELECTED.with_alpha(160);
    assert_eq!(light.paint(tint).alpha(), 160);
    assert_ne!(light.paint(tint).with_alpha(255), SELECTED);
    assert_eq!(theme::repaint(Theme::Dark).paint(tint), tint);
}
