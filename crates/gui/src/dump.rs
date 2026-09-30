use std::fmt::{self, Write as _};
use std::io::{self, Write as _};

use domain::Row;
use platform::ScriptLine;
use ui::{Id, Ui};

use crate::field::{Fields, Which};
use crate::graph::{GraphState, Hit, Node};
use crate::ids;
use crate::model::{Model, Scrolls, StepViews, ViewFlag};
use crate::nav::Nav;
use crate::palette::Palette;
use crate::panels::Panels;
use crate::peek::Peek;
use crate::status::Status;
use crate::text::Tag;
use crate::work::{Services, WorkState};

#[derive(Default)]
pub(crate) struct DumpLines(String);

impl DumpLines {
    fn line(&mut self, arguments: fmt::Arguments<'_>) {
        self.0.push_str("DUMP ");
        self.0.write_fmt(arguments).unwrap_or_default();
        self.0.push('\n');
    }

    pub(crate) fn rect(&mut self, ui: &Ui, line: &ScriptLine) {
        if let Some(name) = line.word(1) {
            self.line(format_args!(
                "rect {name} = {:?}",
                ui.interaction(Id::from_name(name)).rect()
            ));
        }
    }

    pub(crate) fn print(&self) {
        let mut output = io::stderr().lock();
        drop(output.write_all(self.0.as_bytes()));
    }
}

pub(crate) fn report(arguments: fmt::Arguments<'_>) {
    let mut output = io::stderr().lock();
    drop(writeln!(output, "{arguments}"));
}

pub(crate) struct Context<'dump> {
    pub(crate) model: &'dump Model,
    pub(crate) ui: &'dump Ui,
    pub(crate) services: &'dump Services,
}

pub(crate) trait Dump {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines);
}

struct Optional<T>(Option<T>);

impl<T: fmt::Display> fmt::Display for Optional<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(value) => write!(formatter, "Some({value})"),
            None => formatter.write_str("None"),
        }
    }
}

struct Quoted<T>(T);

impl<T: fmt::Display> fmt::Display for Quoted<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self.0.to_string())
    }
}

impl Dump for Nav {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        let index = &context.model.index;
        let focus = self
            .focus()
            .and_then(|symbol| index.symbol(symbol))
            .map(|symbol| Quoted(symbol.name().as_str()));
        let file = self
            .file()
            .and_then(|file| index.file(file))
            .map(|file| Quoted(file.path().as_str()));
        let selection = self
            .lines()
            .map(|chosen| format!("({}, {})", chosen.from.value(), chosen.to.value()));
        lines.line(format_args!(
            "tab={:?} path={} step={} focus={} file={} sel={}",
            self.tab(),
            Optional(self.path()),
            Optional(self.step()),
            Optional(focus),
            Optional(file),
            Optional(selection)
        ));
        let model = context.model;
        if self.target() != self.step() || model.step_grab.is_some() || model.new_path.is_some() {
            let grab = model.step_grab.map(|grab| {
                format!(
                    "{}{}",
                    grab.key.step,
                    if grab.is_moving() { " moving" } else { "" }
                )
            });
            lines.line(format_args!(
                "authoring target={} grab={} new_path={}",
                Optional(self.target()),
                Optional(grab),
                Optional(model.new_path.map(Tag::kind))
            ));
        }
    }
}

impl Dump for Scrolls {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        for (name, id) in [
            ("document", ids::document()),
            ("listing", ids::listing()),
            ("paths", ids::paths()),
            ("output", ids::output()),
        ] {
            if let Some(placement) = context.ui.placement(id) {
                lines.line(format_args!(
                    "scroll {name} off={} rect={:?} content={:?}",
                    self.get(id),
                    placement.rect,
                    placement.content
                ));
            }
        }
    }
}

impl Dump for Panels {
    fn dump(&self, _: &Context<'_>, lines: &mut DumpLines) {
        lines.line(format_args!("panels {self}"));
    }
}

struct PeekText<'dump>(&'dump Peek);

impl fmt::Display for PeekText<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Peek::Symbol(symbol) => write!(
                formatter,
                "Sym(SymRef {{ file: {}, sym: {} }})",
                symbol.file(),
                symbol.symbol()
            ),
            Peek::Line { file, line } => write!(formatter, "Line({file}, {})", line.value()),
            Peek::Outside {
                file,
                line,
                first,
                grid,
            } => write!(
                formatter,
                "Outside({}, {}, {}, {} rows)",
                file.display(),
                line.value(),
                first.value(),
                grid.rows()
            ),
        }
    }
}

impl Dump for Status {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        let model = context.model;
        lines.line(format_args!(
            "tip={} peek={} status={}",
            Optional(model.tip_shown.as_ref().map(|tip| Quoted(tip.as_str()))),
            Optional(model.peek.as_ref().map(PeekText)),
            Quoted(self)
        ));
    }
}

impl Dump for Fields {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        let listed = context
            .model
            .listed_rows()
            .iter()
            .filter(|row| matches!(row, Row::Path { .. }))
            .count();
        lines.line(format_args!(
            "paths filter={} listed={listed}",
            Quoted(self.get(Which::PathFilter).text().as_str())
        ));
    }
}

impl Dump for StepViews {
    fn dump(&self, _context: &Context<'_>, lines: &mut DumpLines) {
        let mut views = String::new();
        for (key, view) in self.customized() {
            let flags: Vec<&str> = [
                (ViewFlag::Whole, "whole"),
                (ViewFlag::Hidden, "hidden"),
                (ViewFlag::Folded, "folded"),
                (ViewFlag::Expanded, "expanded"),
            ]
            .into_iter()
            .filter(|(flag, _)| view.flags.has(*flag))
            .map(|(_, name)| name)
            .collect();
            write!(
                views,
                " {}:{}({} +{}/+{})",
                key.path,
                key.step,
                flags.join(" "),
                view.context.above,
                view.context.below
            )
            .unwrap_or_default();
        }
        if !views.is_empty() {
            lines.line(format_args!("step-views{views}"));
        }
    }
}

impl Dump for WorkState {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        lines.line(format_args!(
            "backend progress={} indexing={} unmerged={} reindexing={} linking={}",
            Quoted(self.progress().as_str()),
            self.indexing(),
            self.unmerged(),
            context.services.reindexing(),
            context.services.linking()
        ));
    }
}

fn node_name(model: &Model, node: Node) -> ui::Label {
    let number = model
        .graph
        .built()
        .step
        .get(&node)
        .map_or_else(String::new, |step| format!("{} ", step.number.as_str()));
    let name = model
        .index
        .symbol(node.symbol)
        .map_or("", |symbol| symbol.name().as_str());
    ui::Label::new(format!("{number}{name}"))
}

impl Dump for GraphState {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        let Context { model, ui, .. } = *context;
        let canvas = ids::GRAPH_CANVAS.id();
        lines.line(format_args!(
            "graph zoom={:.3} pan={:?} canvas={:?} camera={}",
            self.zoom().get(),
            self.pan(),
            ui.placement(canvas).map(|placement| placement.rect),
            self.camera()
        ));
        let pointer = ui.pointer();
        lines.line(format_args!(
            "input mouse={:?} down={:?} hot_is_canvas={} active_is_canvas={} drag={}",
            pointer.mouse,
            pointer.down,
            ui.hot() == Some(canvas),
            ui.active() == Some(canvas),
            self.drag_text()
        ));
        for hit in self.hits() {
            if let Hit::Body(node) = hit.hit {
                lines.line(format_args!(
                    "node {} rect={:?}",
                    node_name(model, node).as_str(),
                    hit.rect
                ));
            }
        }
        for hit in self.hits() {
            let Hit::Button(node, button) = hit.hit else {
                continue;
            };
            let header = self.built().header(model, self, node);
            let Some(labelled) = header
                .buttons
                .into_iter()
                .find(|labelled| labelled.button == button)
            else {
                continue;
            };
            let name = model
                .index
                .symbol(node.symbol)
                .map_or("", |symbol| symbol.name().as_str());
            lines.line(format_args!(
                "button {name} '{}' rect={:?}",
                labelled.label.spelled(),
                hit.rect
            ));
        }
        for hit in self.hits() {
            if let Hit::Parent(node) = hit.hit {
                lines.line(format_args!(
                    "parent {} rect={:?}",
                    node_name(model, node).as_str(),
                    hit.rect
                ));
            }
        }
    }
}

impl Dump for Option<Palette> {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        let Some(palette) = self else {
            return;
        };
        let chosen = palette.entries.get(palette.selected.get()).map(|entry| {
            format!(
                "{} {}",
                entry.kind.tag().as_str(),
                Quoted(entry.name.as_str())
            )
        });
        lines.line(format_args!(
            "palette query={} rows={} selected={} top={} chosen={}",
            Quoted(context.model.fields.get(Which::Palette).text().as_str()),
            palette.entries.len(),
            palette.selected.get(),
            palette.top.get(),
            Optional(chosen)
        ));
    }
}
