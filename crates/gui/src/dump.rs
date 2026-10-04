use std::fmt::{self, Write as _};
use std::io::{self, Write as _};
use std::iter;

use domain::Row;
use platform::ScriptLine;
use ui::{Axis, Count, Id, Ui};

use crate::field::{Fields, Which};
use crate::graph::{GraphState, Hit, Node};
use crate::ids;
use crate::model::{Model, Scrolls, StepViews, ViewFlag};
use crate::nav::Nav;
use crate::palette::Palette;
use crate::panels::{Direction, Panels};
use crate::peek::Peek;
use crate::settings::{SettingsMenu, ShownFont, ThemeName};
use crate::status::Status;
use crate::text::Tag;
use crate::wizard::{Line, Mode, StepState, Tick, Wizard, verdict_words};
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
            "tab={:?} tour={} step={} focus={} file={} sel={}",
            self.tab(),
            Optional(self.tour()),
            Optional(self.step()),
            Optional(focus),
            Optional(file),
            Optional(selection)
        ));
        let model = context.model;
        if self.target() != self.step() || model.step_grab.is_some() {
            let grab = model.step_grab.map(|grab| {
                format!(
                    "{}{}",
                    grab.key.step,
                    if grab.is_moving() { " moving" } else { "" }
                )
            });
            lines.line(format_args!(
                "authoring target={} grab={}",
                Optional(self.target()),
                Optional(grab)
            ));
        }
    }
}

impl Dump for Scrolls {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        for (name, id) in [
            ("document", ids::document()),
            ("source", ids::source()),
            ("tours", ids::tours()),
            ("console", ids::console()),
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
        let code = ids::DOCUMENT_CODE.as_str();
        let steps = (0..).map(|step| (format!("{code}/{step}"), Id::from_name(code).nth(step)));
        let source = (String::from("lines"), ids::LINES.id());
        for (name, id) in iter::once(source).chain(steps.take(DUMPED_STEP_BLOCKS.get())) {
            if let Some(placement) = context.ui.placement(id)
                && let Some(bar) =
                    placement.scrollbar(Axis::Horizontal, placement.scroll_offset.horizontal)
            {
                lines.line(format_args!(
                    "across {name} thumb={:?} off={} rect={:?} content={:?}",
                    bar.thumb,
                    placement.scroll_offset.horizontal,
                    placement.rect,
                    placement.content
                ));
            }
        }
    }
}

const DUMPED_STEP_BLOCKS: Count = Count::new(8);

impl Dump for Option<Wizard> {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        let Some(wizard) = self else {
            return;
        };
        let index = &context.model.index;
        let name_of = |symbol: domain::SymbolId| {
            index
                .symbol(symbol)
                .map_or_else(String::new, |found| found.name().as_str().to_owned())
        };
        let outline = wizard.outline();
        let rows = wizard.rows();
        let model = context.model;
        match wizard.mode() {
            Mode::Build => lines.line(format_args!(
                "wizard page={} kind={} name={} start={} steps={} rows={} refusal={}",
                wizard.page().word().as_str(),
                Tag::kind(wizard.kind()),
                model.typed_name().as_str(),
                Optional(wizard.start().map(name_of)),
                outline.ticked().len(),
                rows.len(),
                Optional(wizard.refusal().map(|why| why.as_str().to_owned()))
            )),
            Mode::EditTour(tour) => {
                let changes = wizard.changes();
                lines.line(format_args!(
                    "wizard edit tour={tour} name={} kind={} group={} changes={} rows={} refusal={}",
                    model.typed_name().as_str(),
                    Tag::kind(wizard.kind()),
                    model.fields.get(Which::WizardGroup).text().as_str(),
                    changes.len(),
                    rows.len(),
                    Optional(wizard.refusal().map(|why| why.as_str().to_owned()))
                ));
                for change in changes {
                    lines.line(format_args!("wizard change {change:?}"));
                }
            }
        }
        for (row, shaped) in rows.iter().enumerate() {
            match &shaped.line {
                Line::Branch(id) => {
                    let Some(branch) = outline.branch(*id) else {
                        continue;
                    };
                    let number = match &branch.state {
                        StepState::Existing { number, .. } => number.to_string(),
                        StepState::New => "new".to_owned(),
                    };
                    lines.line(format_args!(
                        "wizard row {row} depth={} {} {number} {} {:?} {} note={}",
                        branch.depth(),
                        if branch.tick == Tick::Ticked {
                            "x"
                        } else {
                            "-"
                        },
                        branch
                            .symbol()
                            .map_or_else(|| "(lines)".to_owned(), name_of),
                        shaped.expander,
                        verdict_words(branch.verdict()).as_str(),
                        model.fields.get(Which::StepNote(*id)).text().as_str()
                    ));
                }
                Line::Fold(fold) => lines.line(format_args!(
                    "wizard row {row} fold {:?} {} left out: {}",
                    fold.shown,
                    fold.members.len(),
                    verdict_words(domain::Verdict::Cut(fold.cut)).as_str()
                )),
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
            .filter(|row| matches!(row, Row::Tour { .. }))
            .count();
        lines.line(format_args!(
            "tours filter={} listed={listed}",
            Quoted(self.get(Which::TourFilter).text().as_str())
        ));
        lines.line(format_args!(
            "field focused={}",
            Optional(
                self.focused()
                    .map(|which| which.control().element().as_str())
            )
        ));
        if let Some(which) = self.focused() {
            let field = self.get(which);
            let selection = field.text_selection().map_or_else(String::new, |chosen| {
                format!(" selection={}..{}", chosen.from.get(), chosen.to.get())
            });
            lines.line(format_args!(
                "field {which:?} caret={}{selection} first={} rows={} text={}",
                field.caret().get(),
                field.first(),
                field.rows(which.shape()).len(),
                Quoted(field.text().as_str())
            ));
        }
    }
}

impl Dump for StepViews {
    fn dump(&self, _context: &Context<'_>, lines: &mut DumpLines) {
        let mut views = String::new();
        for (key, view) in self.customized() {
            let flags: Vec<&str> = [
                (ViewFlag::Whole, "whole"),
                (ViewFlag::Hidden, "hidden"),
                (ViewFlag::Collapsed, "collapsed"),
                (ViewFlag::Inlined, "inlined"),
            ]
            .into_iter()
            .filter(|(flag, _)| view.flags.has(*flag))
            .map(|(_, name)| name)
            .collect();
            write!(
                views,
                " {}:{}({} +{}/+{})",
                key.tour,
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
        let placed = ui
            .placement(ids::GRAPH_MINIMAP.id())
            .map(|placement| placement.rect);
        match (self.minimap(), placed) {
            (Some(minimap), Some(rect)) => lines.line(format_args!(
                "minimap rect={rect:?} camera={:?} dragged={}",
                minimap.camera_on(rect),
                self.drags_minimap()
            )),
            _ => lines.line(format_args!("minimap hidden")),
        }
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

impl Dump for domain::Map {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        let model = context.model;
        let Some(tour) = model.nav.tour().and_then(|slot| model.tour(slot)) else {
            return;
        };
        let noted: Vec<String> = tour
            .steps()
            .iter()
            .filter_map(|step| {
                Some(format!(
                    "{}={:?}",
                    step.symbol().map_or("(lines)", |name| name.as_str()),
                    step.note()?.as_str()
                ))
            })
            .collect();
        lines.line(format_args!(
            "map tour={} kind={} group={} steps={} unsaved={:?} note={:?} noted=[{}]",
            tour.name(),
            Tag::kind(tour.kind()),
            tour.group().map_or("", domain::GroupName::as_str),
            tour.steps().len(),
            model.disk.dirty,
            tour.note().map_or("", domain::Note::as_str),
            noted.join(", ")
        ));
    }
}

impl Dump for Option<SettingsMenu> {
    fn dump(&self, context: &Context<'_>, lines: &mut DumpLines) {
        let settings = &context.model.settings;
        lines.line(format_args!(
            "settings open={} theme={} font={} size={} graph_direction={}",
            self.is_some(),
            ThemeName::new(settings.theme()),
            ShownFont::new(settings),
            settings.size().get(),
            Direction::of_saved(settings.graph_direction())
        ));
    }
}
