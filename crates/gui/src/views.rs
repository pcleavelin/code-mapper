mod authoring;
mod bars;
mod center;
mod document;
mod left;
mod output;
mod palette;
mod panel_tree;
mod tooltip;
mod xrefs;

use crate::graph::GraphFrame;
use crate::model::Model;
use crate::widgets::{Container, Frame};

pub(crate) use authoring::outline_input;
pub(crate) use panel_tree::panel_input;

pub(crate) fn build(model: &Model, frame: &mut Frame<'_>, graph: Option<GraphFrame>) {
    let mut graph = graph;
    frame.start(Container::Window);
    bars::top_bar(model, frame);
    frame.start(Container::Body);
    panel_tree::tree(model, frame, &mut graph);
    frame.finish();
    bars::status_bar(model, frame);
    frame.finish();
    panel_tree::drag_band(model, frame);
    authoring::outline_band(model, frame);
    panel_tree::picker(model, frame);
    palette::palette(model, frame);
    tooltip::tooltip(model, frame);
}
