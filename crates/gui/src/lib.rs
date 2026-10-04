mod action;
mod app;
mod authoring;
mod dump;
mod field;
mod graph;
mod grid;
mod ids;
mod keys;
mod menu;
mod model;
mod nav;
mod palette;
mod panels;
mod peek;
mod runtime;
mod settings;
mod status;
mod text;
mod theme;
mod views;
mod welcome;
mod widgets;
mod wizard;
mod work;

use domain::Root;
use platform::{StartError, Title};

use crate::app::App;

pub fn run(root: &Root) -> Result<(), StartError> {
    let title = Title::new(format!("codemap - {}", root.as_path().display()));
    platform::run(title, App::new(root))
}

#[cfg(test)]
mod tests;
