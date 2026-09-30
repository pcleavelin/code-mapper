mod action;
mod app;
mod atlas;
mod authoring;
mod dataflow;
mod derived;
mod dump;
mod field;
mod graph;
mod grid;
mod ids;
mod keys;
mod model;
mod nav;
mod palette;
mod panels;
mod peek;
mod runtime;
mod sequence;
mod status;
mod text;
mod theme;
mod types;
mod views;
mod widgets;
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
