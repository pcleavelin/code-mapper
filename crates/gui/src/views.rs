mod bars;
mod center;
mod docked;
mod document;
mod left;
mod output;
mod tooltip;
mod xrefs;

use crate::graph::GraphFrame;
use crate::model::{Model, Tab};
use crate::widgets::{Container, Frame};

use crate::dock::Edge;
pub(crate) use docked::dock_input;

pub(crate) fn build(model: &Model, frame: &mut Frame<'_>, graph: Option<GraphFrame>) {
    frame.start(Container::Window);
    bars::top_bar(model, frame);
    frame.start(Container::Body);
    frame.start(Container::DockRow);
    let sizes = model.dock.sizes(frame.ui.size(), model.metrics.cell);
    docked::docked(model, frame, Edge::Left, &sizes);
    frame.start(Container::Center);
    match model.nav.tab() {
        Tab::Path => document::path_document(model, frame),
        Tab::Diff => center::diff_view(model, frame),
        Tab::Graph => center::graph_tab(model, frame, graph),
        Tab::Listing => center::listing(model, frame),
        Tab::Results => center::results_view(model, frame),
    }
    frame.finish();
    docked::docked(model, frame, Edge::Right, &sizes);
    frame.finish();
    docked::docked(model, frame, Edge::Bottom, &sizes);
    frame.finish();
    bars::status_bar(model, frame);
    frame.finish();
    docked::drag_band(model, frame);
    tooltip::tooltip(model, frame);
}
