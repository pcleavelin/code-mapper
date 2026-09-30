use std::collections::BTreeMap;
use std::fmt;
use std::iter;
use std::mem;

use clap::{Args, CommandFactory, Parser};
use domain::{
    Author, Cut, Depth, GroupName, Index, Line, LineCount, Location, MapError, Note, ParentLabel,
    Path, PathCount, PathKind, PathName, Program, Promoted, RelativePath, Revision, SourceFile,
    SourceLine, Span, Step, StepChange, StepNumber, Stop, Symbol, SymbolId, SymbolKind, SymbolName,
    SymbolQuery, TextFragment,
};
use features::Feature;
use index::{ServerNotice, StartError};
use io_map::{Fault, MapLoadError, MapSaveError, MapStore, MapVersion, Origin, ParseError};

use crate::convert::{Count, StepIndex, Under, step_index};
use crate::exec::LeftStale;
use crate::failure::{Candidate, Failure, StepPlace};
use crate::output::{Output, write_line};

const PROGRAM: &str = "codemap";

const KINDS: [(PathKind, &str); 3] = [
    (PathKind::Flow, "flow"),
    (PathKind::Layer, "layer"),
    (PathKind::Type, "type"),
];

const FUNCTION_KINDS: [&str; 5] = ["function", "method", "macro", "constructor", "proc"];

fn command_name(feature: Feature) -> &'static str {
    feature.spec().name().as_str()
}

fn summary(feature: Feature) -> &'static str {
    feature.spec().summary().as_str()
}

#[derive(Parser, Debug)]
#[command(
    name = PROGRAM,
    disable_help_flag = true,
    override_usage = "codemap <root>                     open the GUI\n       codemap <root> <command> [args]    text mode (same commands work in the GUI Console)"
)]
pub(crate) enum Command {
    #[command(name = command_name(Feature::Files), about = summary(Feature::Files))]
    Files(FilterArguments),
    #[command(name = command_name(Feature::Symbols), about = summary(Feature::Symbols))]
    Symbols(FilterArguments),
    #[command(name = command_name(Feature::Show), about = summary(Feature::Show))]
    Show(ShowArguments),
    #[command(name = command_name(Feature::Search), about = summary(Feature::Search))]
    Search(RegexArguments),
    #[command(name = command_name(Feature::Notes), about = summary(Feature::Notes))]
    Notes(RegexArguments),
    #[command(name = command_name(Feature::Callers), about = summary(Feature::Callers))]
    Callers(SymbolArguments),
    #[command(name = command_name(Feature::Callees), about = summary(Feature::Callees))]
    Callees(SymbolArguments),
    #[command(name = command_name(Feature::Refs), about = summary(Feature::Refs))]
    Refs(SymbolArguments),
    #[command(name = command_name(Feature::Index), about = summary(Feature::Index))]
    Index(FilterArguments),
    #[command(name = command_name(Feature::Tree), about = summary(Feature::Tree))]
    Tree(TreeArguments),
    #[command(name = command_name(Feature::Roots), about = summary(Feature::Roots))]
    Roots(RootsArguments),
    #[command(name = command_name(Feature::Paths), about = summary(Feature::Paths))]
    Paths(PathsArguments),
    #[command(name = command_name(Feature::Path), about = summary(Feature::Path))]
    Path(PathArguments),
    #[command(name = command_name(Feature::PathNew), about = summary(Feature::PathNew))]
    PathNew(PathNewArguments),
    #[command(name = command_name(Feature::PathGroup), about = summary(Feature::PathGroup))]
    PathGroup(PathGroupArguments),
    #[command(name = command_name(Feature::Groups), about = summary(Feature::Groups))]
    Groups,
    #[command(name = command_name(Feature::GroupRename), about = summary(Feature::GroupRename))]
    GroupRename(GroupRenameArguments),
    #[command(name = command_name(Feature::PathNote), about = summary(Feature::PathNote))]
    PathNote(PathNoteArguments),
    #[command(name = command_name(Feature::StepNote), about = summary(Feature::StepNote))]
    StepNote(StepNoteArguments),
    #[command(name = command_name(Feature::StepLink), about = summary(Feature::StepLink))]
    StepLink(StepLinkArguments),
    #[command(name = command_name(Feature::StepUnlink), about = summary(Feature::StepUnlink))]
    StepUnlink(StepArguments),
    #[command(name = command_name(Feature::NoteEdit), about = summary(Feature::NoteEdit))]
    #[command(allow_negative_numbers = true)]
    NoteEdit(NoteEditArguments),
    #[command(name = command_name(Feature::PathRename), about = summary(Feature::PathRename))]
    PathRename(PathRenameArguments),
    #[command(name = command_name(Feature::PathAdd), about = summary(Feature::PathAdd))]
    #[command(allow_negative_numbers = true)]
    PathAdd(PathAddArguments),
    #[command(name = command_name(Feature::PathPin), about = summary(Feature::PathPin))]
    PathPin(PathPinArguments),
    #[command(name = command_name(Feature::PathMove), about = summary(Feature::PathMove))]
    #[command(allow_negative_numbers = true)]
    PathMove(PathMoveArguments),
    #[command(name = command_name(Feature::PathSwap), about = summary(Feature::PathSwap))]
    PathSwap(PathSwapArguments),
    #[command(name = command_name(Feature::PathRm), about = summary(Feature::PathRm))]
    PathRm(PathRmArguments),
    #[command(name = command_name(Feature::Promote), about = summary(Feature::Promote))]
    Promote(PromoteArguments),
    #[command(name = command_name(Feature::Stale), about = summary(Feature::Stale))]
    Stale,
    #[command(name = command_name(Feature::Check), about = summary(Feature::Check))]
    Check,
    #[command(name = command_name(Feature::Repin), about = summary(Feature::Repin))]
    Repin(RepinArguments),
    #[command(name = command_name(Feature::Uncovered), about = summary(Feature::Uncovered))]
    Uncovered(FilterArguments),
    #[command(name = command_name(Feature::Coverage), about = summary(Feature::Coverage))]
    Coverage,
    #[command(name = command_name(Feature::Diff), about = summary(Feature::Diff))]
    Diff,
}

#[derive(Args, Debug)]
pub(crate) struct FilterArguments {
    pub(crate) filter: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct ShowArguments {
    pub(crate) file: String,
    pub(crate) start: Option<usize>,
    pub(crate) end: Option<usize>,
}

#[derive(Args, Debug)]
pub(crate) struct RegexArguments {
    pub(crate) regex: String,
}

#[derive(Args, Debug)]
pub(crate) struct SymbolArguments {
    pub(crate) symbol: String,
}

#[derive(Args, Debug)]
pub(crate) struct TreeArguments {
    pub(crate) symbol: String,
    #[arg(default_value_t = 4)]
    pub(crate) depth: usize,
}

#[derive(Args, Debug)]
pub(crate) struct RootsArguments {
    #[arg(value_name = "N", default_value_t = 30)]
    pub(crate) count: usize,
}

#[derive(Args, Debug)]
pub(crate) struct PathsArguments {
    pub(crate) name: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct PathArguments {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) inline: bool,
}

#[derive(Args, Debug)]
pub(crate) struct PathNewArguments {
    pub(crate) name: String,
    #[arg(value_parser = parse_kind)]
    pub(crate) kind: PathKind,
    pub(crate) note: Option<String>,
    #[arg(long)]
    pub(crate) group: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct PathGroupArguments {
    pub(crate) name: String,
    pub(crate) group: String,
}

#[derive(Args, Debug)]
pub(crate) struct GroupRenameArguments {
    pub(crate) old: String,
    pub(crate) new: String,
}

#[derive(Args, Debug)]
pub(crate) struct PathNoteArguments {
    pub(crate) name: String,
    pub(crate) note: String,
}

#[derive(Args, Debug)]
pub(crate) struct StepNoteArguments {
    pub(crate) name: String,
    pub(crate) index: usize,
    pub(crate) note: String,
}

#[derive(Args, Debug)]
pub(crate) struct StepLinkArguments {
    pub(crate) name: String,
    pub(crate) index: usize,
    pub(crate) target: String,
}

#[derive(Args, Debug)]
pub(crate) struct StepArguments {
    pub(crate) name: String,
    pub(crate) index: usize,
}

#[derive(Args, Debug)]
pub(crate) struct NoteEditArguments {
    pub(crate) name: String,
    pub(crate) index: i64,
    pub(crate) old: String,
    pub(crate) new: String,
}

#[derive(Args, Debug)]
pub(crate) struct PathRenameArguments {
    pub(crate) name: String,
    pub(crate) new: String,
}

#[derive(Args, Debug)]
pub(crate) struct PathAddArguments {
    pub(crate) name: String,
    pub(crate) target: String,
    #[arg(value_name = "NUMS", num_args = 0..=3)]
    pub(crate) numbers: Vec<i64>,
}

#[derive(Args, Debug)]
pub(crate) struct PathPinArguments {
    pub(crate) name: String,
    pub(crate) index: usize,
    pub(crate) file: String,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Args, Debug)]
pub(crate) struct PathMoveArguments {
    pub(crate) name: String,
    pub(crate) index: usize,
    pub(crate) under: i64,
}

#[derive(Args, Debug)]
pub(crate) struct PathSwapArguments {
    pub(crate) name: String,
    #[arg(value_name = "A")]
    pub(crate) one: usize,
    #[arg(value_name = "B")]
    pub(crate) other: usize,
}

#[derive(Args, Debug)]
pub(crate) struct PathRmArguments {
    pub(crate) name: String,
    pub(crate) index: Option<usize>,
}

#[derive(Args, Debug)]
pub(crate) struct PromoteArguments {
    pub(crate) symbol: String,
    pub(crate) depth: Option<usize>,
    pub(crate) name: Option<String>,
    #[arg(long)]
    pub(crate) all: bool,
}

#[derive(Args, Debug)]
pub(crate) struct RepinArguments {
    #[arg(value_name = "REV")]
    pub(crate) revision: Option<String>,
}

impl Command {
    pub(crate) fn feature(&self) -> Feature {
        match self {
            Self::Files(_) => Feature::Files,
            Self::Symbols(_) => Feature::Symbols,
            Self::Show(_) => Feature::Show,
            Self::Search(_) => Feature::Search,
            Self::Notes(_) => Feature::Notes,
            Self::Callers(_) => Feature::Callers,
            Self::Callees(_) => Feature::Callees,
            Self::Refs(_) => Feature::Refs,
            Self::Index(_) => Feature::Index,
            Self::Tree(_) => Feature::Tree,
            Self::Roots(_) => Feature::Roots,
            Self::Paths(_) => Feature::Paths,
            Self::Path(_) => Feature::Path,
            Self::PathNew(_) => Feature::PathNew,
            Self::PathGroup(_) => Feature::PathGroup,
            Self::Groups => Feature::Groups,
            Self::GroupRename(_) => Feature::GroupRename,
            Self::PathNote(_) => Feature::PathNote,
            Self::StepNote(_) => Feature::StepNote,
            Self::StepLink(_) => Feature::StepLink,
            Self::StepUnlink(_) => Feature::StepUnlink,
            Self::NoteEdit(_) => Feature::NoteEdit,
            Self::PathRename(_) => Feature::PathRename,
            Self::PathAdd(_) => Feature::PathAdd,
            Self::PathPin(_) => Feature::PathPin,
            Self::PathMove(_) => Feature::PathMove,
            Self::PathSwap(_) => Feature::PathSwap,
            Self::PathRm(_) => Feature::PathRm,
            Self::Promote(_) => Feature::Promote,
            Self::Stale => Feature::Stale,
            Self::Check => Feature::Check,
            Self::Repin(_) => Feature::Repin,
            Self::Uncovered(_) => Feature::Uncovered,
            Self::Coverage => Feature::Coverage,
            Self::Diff => Feature::Diff,
        }
    }
}

fn parse_kind(text: &str) -> Result<PathKind, String> {
    KINDS
        .iter()
        .find(|entry| entry.1 == text)
        .map(|entry| entry.0)
        .ok_or_else(|| {
            let names: Vec<&str> = KINDS.iter().map(|entry| entry.1).collect();
            format!("expected one of {}", names.join(", "))
        })
}

pub(crate) fn parse<'a>(arguments: impl Iterator<Item = &'a str>) -> Result<Command, clap::Error> {
    Command::try_parse_from(iter::once(PROGRAM).chain(arguments))
}

pub(crate) fn help() -> Output {
    Output::of(Command::command().render_help().to_string())
}

#[cfg(test)]
pub(crate) fn subcommands() -> Vec<(String, String)> {
    let mut command = Command::command();
    command.build();
    command
        .get_subcommands()
        .map(|sub| {
            (
                sub.get_name().to_owned(),
                sub.get_about().map(ToString::to_string).unwrap_or_default(),
            )
        })
        .collect()
}

pub(crate) fn render_error(error: &clap::Error) -> Output {
    Output::of(error.render().to_string())
}

pub(crate) fn words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => quoted = !quoted,
            space if space.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    words.push(mem::take(&mut current));
                }
            }
            other => current.push(other),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

pub(crate) fn is_function(kind: &SymbolKind) -> bool {
    FUNCTION_KINDS.iter().any(|word| kind.contains(word))
}

fn kind_name(kind: PathKind) -> &'static str {
    KINDS
        .iter()
        .find(|entry| entry.0 == kind)
        .map_or("", |entry| entry.1)
}

fn author_tag(author: Author) -> &'static str {
    match author {
        Author::Human => "",
        Author::Agent => " (ai)",
    }
}

fn symbol_text(symbol: Option<&SymbolName>) -> &str {
    symbol.map_or("", SymbolName::as_str)
}

fn padding(depth: u32) -> String {
    "  ".repeat(usize::try_from(depth).unwrap_or(0))
}

fn span_text(span: Span) -> String {
    format!("{}-{}", span.start().number(), span.end().number())
}

pub(crate) fn symbol_label(index: &Index, id: SymbolId) -> String {
    let (Some(file), Some(symbol)) = (index.file(id.file()), index.symbol(id)) else {
        return String::new();
    };
    format!(
        "{} {}:{}",
        symbol.name(),
        file.path(),
        span_text(symbol.span())
    )
}

fn unique_name(index: &Index, id: SymbolId) -> String {
    let (Some(file), Some(symbol)) = (index.file(id.file()), index.symbol(id)) else {
        return symbol_label(index, id);
    };
    let name = symbol.name();
    let stem = file.path().stem();
    let path = file.path();
    let mut candidates = Vec::new();
    if let Some(owner) = symbol.owner() {
        candidates.push(format!("{owner}::{name}"));
    }
    candidates.push(format!("{stem}:{name}"));
    if let Some(owner) = symbol.owner() {
        candidates.push(format!("{stem}:{owner}::{name}"));
    }
    candidates.push(format!("{path}:{name}"));
    if let Some(owner) = symbol.owner() {
        candidates.push(format!("{path}:{owner}::{name}"));
    }
    candidates
        .into_iter()
        .find(|query| index.find_symbols(&SymbolQuery::from(query.as_str())) == [id])
        .unwrap_or_else(|| symbol_label(index, id))
}

pub(crate) fn candidate(index: &Index, id: SymbolId) -> Candidate {
    Candidate {
        name: Output::of(unique_name(index, id)),
        label: Output::of(symbol_label(index, id)),
    }
}

pub(crate) fn place(index: &Index, step: &Step) -> String {
    match index.find_file(step.file()) {
        None => format!("{} (file gone)", step.file()),
        Some(_) if step.symbol().is_some() && step.resolved_symbol().is_none() => {
            format!("{} (symbol gone)", step.file())
        }
        Some(_) => format!("{}:{}", step.file(), span_text(step.span())),
    }
}

fn link_tag(step: &Step) -> String {
    step.link()
        .map_or_else(String::new, |link| format!("  → {link}"))
}

fn label(path: &PathName, index: StepIndex) -> String {
    format!("{path}[{index}]")
}

pub(crate) fn numbered_lines(output: &mut Output, file: &SourceFile, span: Span) {
    for line in span.lines() {
        if let Some(text) = file.text().line(line) {
            write_line!(output, "{:5} {}", line.number(), text);
        }
    }
}

pub(crate) fn file_row(output: &mut Output, file: &SourceFile) {
    write_line!(
        output,
        "{} ({} lines, {} symbols)",
        file.path(),
        file.text().count(),
        file.symbols().count()
    );
}

pub(crate) fn symbol_row(output: &mut Output, file: &SourceFile, symbol: &Symbol) {
    write_line!(
        output,
        "{}:{} {} {}{} ({} calls, {} callers)",
        file.path(),
        span_text(symbol.span()),
        symbol.kind(),
        if symbol.depth().value() > 0 { "  " } else { "" },
        symbol.name(),
        symbol.callees().len(),
        symbol.callers().len()
    );
}

pub(crate) fn hit_line(output: &mut Output, file: &SourceFile, line: Line, text: &SourceLine) {
    write_line!(output, "{}:{}: {}", file.path(), line.number(), text);
}

pub(crate) fn path_note_line(output: &mut Output, path: &PathName, text: &str) {
    write_line!(output, "{path}: {text}");
}

pub(crate) fn step_note_line(
    output: &mut Output,
    path: &PathName,
    position: StepIndex,
    step: &Step,
    text: &str,
) {
    write_line!(
        output,
        "{} {}:{}: {text}",
        label(path, position),
        step.file(),
        step.span().start().number()
    );
}

pub(crate) fn symbol_line(output: &mut Output, index: &Index, id: SymbolId, depth: Depth) {
    write_line!(
        output,
        "{}{}",
        padding(depth.value()),
        symbol_label(index, id)
    );
}

pub(crate) fn root_row(output: &mut Output, index: &Index, id: SymbolId) {
    write_line!(
        output,
        "{} ({} calls)",
        symbol_label(index, id),
        index.symbol(id).map_or(0, |symbol| symbol.callees().len())
    );
}

pub(crate) fn reference_row(output: &mut Output, location: &Location, text: Option<&SourceLine>) {
    write_line!(
        output,
        "  {}:{}: {}",
        location.file,
        location.line.number(),
        text.map_or("", SourceLine::trimmed)
    );
}

pub(crate) fn server_missing(output: &mut Output, program: Option<Program>) {
    write_line!(
        output,
        "  (references need {})",
        program.map_or_else(
            || "no server for this language".to_owned(),
            |program| program.to_string()
        )
    );
}

pub(crate) fn index_report(output: &mut Output, asked: Count, left: Count) {
    write_line!(
        output,
        "{asked} files asked for; {left} files in the repo still wait for a server"
    );
}

pub(crate) fn group_header(output: &mut Output, group: Option<&GroupName>) {
    write_line!(
        output,
        "== {}",
        group.map_or("(top level)", GroupName::as_str)
    );
}

pub(crate) fn path_row(output: &mut Output, path: &Path) {
    write_line!(
        output,
        "{} [{}]{} ({} steps){}",
        path.name(),
        kind_name(path.kind()),
        author_tag(path.author()),
        path.steps().len(),
        path.note()
            .map_or_else(String::new, |note| format!(": {note}"))
    );
}

pub(crate) fn step_row(
    output: &mut Output,
    index: &Index,
    step: &Step,
    position: StepIndex,
    depth: Depth,
) {
    write_line!(
        output,
        "  {}[{position}] {}{} {}{}{}{}",
        padding(depth.value()),
        if step.is_stale() { "! " } else { "" },
        place(index, step),
        symbol_text(step.symbol()),
        author_tag(step.author()),
        link_tag(step),
        step.note()
            .map_or_else(String::new, |note| format!("  -- {note}"))
    );
}

pub(crate) fn path_title(output: &mut Output, path: &Path) {
    write_line!(
        output,
        "# {} [{}]{}{}",
        path.name(),
        kind_name(path.kind()),
        author_tag(path.author()),
        path.group()
            .map_or_else(String::new, |group| format!("  in {group}"))
    );
    if let Some(note) = path.note() {
        write_line!(output, "{note}");
    }
}

pub(crate) fn linked_from(output: &mut Output, places: &[StepPlace]) {
    if places.is_empty() {
        return;
    }
    let list: Vec<String> = places
        .iter()
        .map(|place| label(&place.path, place.index))
        .collect();
    write_line!(output, "linked from: {}", list.join(", "));
}

pub(crate) fn back_in(output: &mut Output, depth: Depth, parent: &ParentLabel) {
    let name = match parent {
        ParentLabel::Symbol(symbol) => symbol.to_string(),
        ParentLabel::Line { file, line } => format!("{file}:{}", line.number()),
        ParentLabel::TopLevel => "top level".to_owned(),
    };
    write_line!(output, "\n{}-- back in {name} --", padding(depth.value()));
}

pub(crate) struct Title<'a> {
    pub(crate) depth: Depth,
    pub(crate) prefix: &'a [StepNumber],
    pub(crate) number: &'a StepNumber,
    pub(crate) nested: Option<&'a PathName>,
    pub(crate) position: StepIndex,
}

pub(crate) fn step_title(output: &mut Output, index: &Index, step: &Step, title: &Title<'_>) {
    let prefix = title
        .prefix
        .iter()
        .map(|number| format!("{number} › "))
        .collect::<Vec<String>>()
        .concat();
    let label = match title.nested {
        None => format!("[{}]", title.position),
        Some(path) => format!("[{}]", label(path, title.position)),
    };
    write_line!(
        output,
        "\n== {}{prefix}{} {label} {}{} {}{}{}",
        padding(title.depth.value()),
        title.number,
        if step.is_stale() { "STALE " } else { "" },
        place(index, step),
        symbol_text(step.symbol()),
        author_tag(step.author()),
        link_tag(step)
    );
    if let Some(note) = step.note() {
        write_line!(output, "-- {note}");
    }
}

pub(crate) fn inlined_before(output: &mut Output, depth: Depth, link: &PathName) {
    write_line!(
        output,
        "\n{}-- {link} is inlined above --",
        padding(depth.value())
    );
}

pub(crate) fn end_of(output: &mut Output, depth: Depth, link: &PathName) {
    write_line!(output, "\n{}-- end of {link} --", padding(depth.value()));
}

pub(crate) fn group_place(output: &mut Output, path: &PathName, group: Option<&GroupName>) {
    let place = group.map_or_else(
        || "at the top level".to_owned(),
        |group| format!("in {group}"),
    );
    write_line!(output, "'{path}' is {place}");
}

pub(crate) fn group_row(output: &mut Output, group: &GroupName, depth: Depth, paths: PathCount) {
    write_line!(
        output,
        "{}{} ({paths} paths)",
        padding(depth.value()),
        group.last_segment()
    );
}

pub(crate) fn paths_moved(output: &mut Output, moved: PathCount) {
    write_line!(output, "{moved} paths moved");
}

pub(crate) fn step_linked(output: &mut Output, position: StepIndex, target: &TextFragment) {
    write_line!(output, "step [{position}] links to '{target}'");
}

pub(crate) fn link_removed(output: &mut Output, position: StepIndex) {
    write_line!(output, "step [{position}] unlinked");
}

pub(crate) fn note_text(output: &mut Output, note: Option<&Note>) {
    write_line!(output, "{}", note.map_or("", Note::as_str));
}

pub(crate) fn links_moved(output: &mut Output, links: Count, new: &PathName) {
    write_line!(output, "{links} links now point at '{new}'");
}

pub(crate) fn step_added(
    output: &mut Output,
    position: StepIndex,
    symbol: Option<&SymbolName>,
    parent: Under,
) {
    match symbol {
        Some(symbol) => write_line!(output, "step [{position}] {symbol} added under [{parent}]"),
        None => write_line!(output, "step [{position}] added under [{parent}]"),
    }
}

pub(crate) fn absolute_note(output: &mut Output, step: &Step) {
    if step.symbol().is_none() {
        write_line!(
            output,
            "note: lines {} are not inside one symbol; pinned as absolute lines, which go stale with any edit above them",
            span_text(step.span())
        );
    }
}

pub(crate) fn call_note(output: &mut Output, parent: &Step, step: &Step, position: StepIndex) {
    write_line!(
        output,
        "note: {} does not call {}; in a flow a step goes under the step that calls it (path-move <name> {position} <under>)",
        symbol_text(parent.symbol()),
        symbol_text(step.symbol())
    );
}

pub(crate) fn moved_under(output: &mut Output, position: StepIndex, under: Under) {
    write_line!(output, "step [{position}] now under [{under}]");
}

pub(crate) fn pinned(output: &mut Output, index: &Index, position: StepIndex, step: &Step) {
    write_line!(output, "step [{position}] pinned to {}", place(index, step));
}

const fn cut_word(cut: Cut) -> &'static str {
    match cut {
        Cut::Test => "test",
        Cut::Accessor => "accessor (3 lines or fewer, 3 or more callers)",
        Cut::Trivial => "trivial body (a field or Self)",
    }
}

const fn stop_word(stop: Stop) -> &'static str {
    match stop {
        Stop::Mapped => "mapped by another path",
        Stop::Shared => "shared (3 or more callers)",
        Stop::OtherPackage => "in another package",
    }
}

fn names_by<T: Copy + Ord>(
    index: &Index,
    reasons: &BTreeMap<SymbolId, T>,
) -> BTreeMap<T, Vec<String>> {
    let mut grouped: BTreeMap<T, Vec<String>> = BTreeMap::new();
    for (symbol, reason) in reasons {
        if let Some(found) = index.symbol(*symbol) {
            grouped
                .entry(*reason)
                .or_default()
                .push(found.name().as_str().to_owned());
        }
    }
    grouped
}

pub(crate) fn promote_report(output: &mut Output, index: &Index, path: &Path, promoted: &Promoted) {
    write_line!(
        output,
        "path '{}' now has {} steps",
        path.name(),
        path.steps().len()
    );
    let cut = names_by(index, &promoted.cut);
    if !cut.is_empty() {
        write_line!(output, "left out (promote --all keeps them):");
        for (reason, names) in cut {
            write_line!(output, "  {}: {}", cut_word(reason), names.join(", "));
        }
    }
    let stopped = names_by(index, &promoted.stopped);
    if !stopped.is_empty() {
        write_line!(output, "kept as a leaf, not followed:");
        for (reason, names) in stopped {
            write_line!(output, "  {}: {}", stop_word(reason), names.join(", "));
        }
    }
    if !promoted.links.is_empty() {
        write_line!(output, "link each mapped leaf to the path that maps it:");
        for link in &promoted.links {
            if let Some(position) = step_index(path, &link.step) {
                write_line!(
                    output,
                    "  step-link {} {position} {}",
                    path.name(),
                    link.target
                );
            }
        }
    }
}

pub(crate) fn stale_row(
    output: &mut Output,
    index: &Index,
    path: &PathName,
    position: StepIndex,
    step: &Step,
) {
    write_line!(
        output,
        "{} {} {}",
        label(path, position),
        place(index, step),
        symbol_text(step.symbol())
    );
}

pub(crate) fn same_text(
    output: &mut Output,
    path: &PathName,
    position: StepIndex,
    file: &RelativePath,
    span: Span,
) {
    let (start, end) = (span.start().number(), span.end().number());
    write_line!(
        output,
        "  same text at {file}:{start}-{end}   path-pin {path} {position} {file} {start} {end}"
    );
}

pub(crate) fn same_name(
    output: &mut Output,
    path: &PathName,
    position: StepIndex,
    file: &RelativePath,
    span: Span,
) {
    let (start, end) = (span.start().number(), span.end().number());
    write_line!(
        output,
        "  same name at {file}:{start}-{end}   path-pin {path} {position} {file} {start} {end}"
    );
}

pub(crate) fn dangling(output: &mut Output, path: &PathName, position: StepIndex, link: &PathName) {
    write_line!(
        output,
        "{} links to a missing path '{link}'   step-link {path} {position} <path> | step-unlink {path} {position}",
        label(path, position)
    );
}

pub(crate) fn all_fresh(output: &mut Output) {
    write_line!(output, "ok");
}

pub(crate) struct FollowReport<'a> {
    pub(crate) path: &'a PathName,
    pub(crate) position: StepIndex,
    pub(crate) file: &'a RelativePath,
    pub(crate) old: Span,
    pub(crate) revision: &'a Revision,
    pub(crate) moved: Option<&'a RelativePath>,
    pub(crate) new: Span,
    pub(crate) kept: LineCount,
    pub(crate) length: LineCount,
    pub(crate) same: bool,
}

pub(crate) fn follow_report(output: &mut Output, report: &FollowReport<'_>) {
    write_line!(
        output,
        "{} {}:{} in {} -> {}{}  {}/{} lines kept{}",
        label(report.path, report.position),
        report.file,
        span_text(report.old),
        report.revision,
        report
            .moved
            .map_or_else(String::new, |file| format!("{file}:")),
        span_text(report.new),
        report.kept,
        report.length,
        if report.same { ", text unchanged" } else { "" }
    );
}

pub(crate) fn removed_line(output: &mut Output, text: &SourceLine) {
    write_line!(output, "  - {text}");
}

pub(crate) fn added_line(output: &mut Output, text: &SourceLine) {
    write_line!(output, "  + {text}");
}

pub(crate) fn left_stale(
    output: &mut Output,
    path: &PathName,
    position: StepIndex,
    reason: &LeftStale,
) {
    write_line!(output, "{} left stale: {reason}", label(path, position));
}

pub(crate) fn repin_report(output: &mut Output, pinned: Count, left: Count) {
    write_line!(
        output,
        "{pinned} re-pinned, {left} left stale. Reread the note of every step printed with changed lines."
    );
}

pub(crate) fn uncovered_row(output: &mut Output, file: &SourceFile, symbol: &Symbol) {
    write_line!(
        output,
        "{}:{} {} {} ({} lines)",
        file.path(),
        span_text(symbol.span()),
        symbol.kind(),
        symbol.name(),
        symbol.span().count()
    );
}

pub(crate) fn coverage_row(output: &mut Output, file: &SourceFile, covered: Count, symbols: Count) {
    write_line!(output, "{}: {covered}/{symbols}", file.path());
}

pub(crate) fn all_coverage(output: &mut Output, covered: Count, symbols: Count) {
    write_line!(output, "total: {covered}/{symbols}");
}

pub(crate) fn path_changed(output: &mut Output, name: &PathName, header: bool) {
    write_line!(
        output,
        "~ {name}{}",
        if header {
            "  (note, kind or group changed)"
        } else {
            ""
        }
    );
}

pub(crate) fn path_added(output: &mut Output, name: &PathName, steps: Count) {
    write_line!(output, "+ {name} ({steps} steps)");
}

pub(crate) fn path_removed(output: &mut Output, name: &PathName, steps: Count) {
    write_line!(output, "- {name} ({steps} steps)");
}

pub(crate) fn step_change(
    output: &mut Output,
    index: &Index,
    change: StepChange,
    position: StepIndex,
    step: &Step,
) {
    let mark = if change == StepChange::Added {
        '+'
    } else {
        '~'
    };
    let tag = match change {
        StepChange::Added => "new",
        StepChange::Repinned => "re-pinned",
        StepChange::NoteEdited => "note edited",
        StepChange::Relinked => "link changed",
    };
    write_line!(
        output,
        "    {mark} [{position}] {} {}  {tag}",
        place(index, step),
        symbol_text(step.symbol())
    );
}

pub(crate) fn step_removed(output: &mut Output, step: &Step) {
    write_line!(
        output,
        "    - {} {} (removed)",
        step.file(),
        symbol_text(step.symbol())
    );
}

pub(crate) fn start_error(error: &StartError) -> Output {
    Output::of(match error {
        StartError::Missing(program) => format!("{program} not on PATH"),
        StartError::Failed(program) => format!("{program} would not start"),
    })
}

pub(crate) fn notice(notice: &ServerNotice) -> Output {
    Output::of(match notice {
        ServerNotice::ToIndex { program, files } => format!("{program}: {files} files to index"),
        ServerNotice::Unavailable(error) => {
            format!(
                "{}: its files keep the tree-sitter resolver",
                start_error(error).as_str()
            )
        }
    })
}

impl fmt::Display for LeftStale {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileGone => formatter.write_str("its file is gone"),
            Self::MissingAt { file, revision } => write!(formatter, "{file} is not in {revision}"),
            Self::TextGone(revision) => write!(formatter, "its text is not in {revision}"),
            Self::NoSymbol { symbol, file } => {
                write!(
                    formatter,
                    "no symbol {} in {file}",
                    symbol_text(symbol.as_ref())
                )
            }
            Self::NoneKept => formatter.write_str("none of its lines survive"),
            Self::UnderHalfKept { kept, length } => {
                write!(formatter, "{kept}/{length} lines survive")
            }
        }
    }
}

fn map_error(error: &MapError) -> String {
    match error {
        MapError::InvalidName(name) => format!(
            "'{name}' cannot name a path: use letters, digits, '.', '_' and '-', not starting with '.'"
        ),
        MapError::NameTaken(name) => format!("a path named '{name}' already exists"),
        MapError::CaseClash { name, other } => {
            format!("'{name}' differs from the path '{other}' only in letter case")
        }
        MapError::NoGroupGiven => "no group given".to_owned(),
        MapError::NoSuchGroup(group) => format!("no such group: {group}"),
        MapError::NoSuchPath(name) => format!("no such path: {name}"),
        MapError::NoSuchStep(_) => "no such step".to_owned(),
        MapError::NoSuchParent => "no such parent step".to_owned(),
        MapError::NoSuchFile => "no such file".to_owned(),
        MapError::NoSuchSymbol => "no such symbol".to_owned(),
        MapError::OutsideFile => "line range out of bounds".to_owned(),
        MapError::LinkToOwnPath => "a step cannot link to its own path".to_owned(),
        MapError::LinkedFrom { path, steps } => {
            let list: Vec<String> = steps
                .iter()
                .map(|address| format!("{}[{}]", address.path, address.step))
                .collect();
            linked_from_error(path, &list)
        }
        MapError::UnderItself => "a step cannot go under itself or its own descendants".to_owned(),
        MapError::NoteLacks(old) => format!("the note does not contain '{old}'"),
        MapError::SecondStep(address) => format!("a second step {}", address.step),
        MapError::UnknownParent { step, parent } => format!(
            "step {} has parent {parent}, which is not a step of '{}'",
            step.step, step.path
        ),
    }
}

fn linked_from_error(path: &PathName, steps: &[String]) -> String {
    format!(
        "'{path}' is linked from {}; unlink those steps first",
        steps.join(", ")
    )
}

fn origin(origin: &Origin) -> String {
    match origin {
        Origin::File(file) => file.display().to_string().replace('\\', "/"),
        Origin::Revision(revision) => {
            format!("{} at {revision}", MapStore::relative_directory())
        }
    }
}

fn fault(fault: &Fault) -> String {
    let version = MapVersion::CURRENT.as_str();
    match fault {
        Fault::Conflict => "an unresolved merge conflict".to_owned(),
        Fault::Version(line) => format!("'{line}' is not '{version}'; regenerate this map"),
        Fault::NoVersion => format!("expected '{version}' first"),
        Fault::UnknownAuthor(value) => format!("unknown author '{value}'"),
        Fault::UnknownKind(value) => format!("unknown kind '{value}'"),
        Fault::UnknownPathField(key) => format!("unknown field '{key}' of a path"),
        Fault::UnknownStepField(key) => format!("unknown field '{key}' of a step"),
        Fault::SecondStep(value) => format!("a second step {value}"),
        Fault::Order => "order takes a number".to_owned(),
        Fault::Lines => "lines takes two numbers".to_owned(),
        Fault::Hash => "hash takes 16 hex digits".to_owned(),
        Fault::InvalidName(name) => map_error(&MapError::InvalidName(name.clone())),
        Fault::InvalidStepId(value) => format!("'{value}' is not a step id"),
        Fault::MissingField(key) => format!("a step with no '{key}' line"),
        Fault::NoPathLine => "a path with no 'path' line".to_owned(),
        Fault::UnknownParent { path, step, parent } => {
            format!("step {step} has parent {parent}, which is not a step of '{path}'")
        }
        Fault::Map(error) => map_error(error),
    }
}

fn parse_error(error: &ParseError) -> String {
    match error.line {
        Some(line) => format!(
            "{}:{}: {}",
            origin(&error.origin),
            line.number(),
            fault(&error.fault)
        ),
        None => format!("{}: {}", origin(&error.origin), fault(&error.fault)),
    }
}

fn load_error(error: &MapLoadError) -> String {
    match error {
        MapLoadError::OldFormat(directory) => format!(
            "{} is a map in the old single-file format; regenerate it",
            directory.display()
        ),
        MapLoadError::Unreadable { file, error } => format!("{}: {error}", file.display()),
        MapLoadError::Parse(error) => parse_error(error),
        MapLoadError::OnePathPerFile(file) => format!(
            "{}: a map file holds exactly one path",
            origin(&Origin::File(file.clone()))
        ),
        MapLoadError::Misplaced { file, path } => format!(
            "{}: holds the path '{path}', which belongs in {path}.cmap",
            origin(&Origin::File(file.clone()))
        ),
    }
}

fn save_error(error: &MapSaveError) -> String {
    match error {
        MapSaveError::CreateDirectory { error, .. }
        | MapSaveError::Write { error, .. }
        | MapSaveError::Remove { error, .. } => format!("save failed: {error}"),
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Map(error) => formatter.write_str(&map_error(error)),
            Self::LinkedFrom { path, steps } => {
                let list: Vec<String> = steps
                    .iter()
                    .map(|place| label(&place.path, place.index))
                    .collect();
                formatter.write_str(&linked_from_error(path, &list))
            }
            Self::NoSuchPath(name) => write!(formatter, "no such path: {name}"),
            Self::NoSuchStep => formatter.write_str("no such step"),
            Self::NoSuchFile(file) => write!(formatter, "no such file: {file}"),
            Self::NoSuchSymbol(symbol) => write!(formatter, "no such symbol: {symbol}"),
            Self::SymbolClash { query, candidates } => {
                let list: Vec<String> = candidates
                    .iter()
                    .map(|choice| format!("{:<40} {}", choice.name.as_str(), choice.label))
                    .collect();
                write!(
                    formatter,
                    "ambiguous: {query}; use one of\n  {}",
                    list.join("\n  ")
                )
            }
            Self::LineRange => formatter.write_str("line range out of bounds"),
            Self::NoPlaceUnder { under, steps } => write!(
                formatter,
                "no step [{under}] to go under: the path has {steps} steps (-1 = root)"
            ),
            Self::NoLink(position) => write!(formatter, "step [{position}] has no link"),
            Self::NoLinkTarget => formatter
                .write_str("step-link needs the path to link to; step-unlink removes a link"),
            Self::Regex(error) => write!(formatter, "{error}"),
            Self::ServersInBackground => {
                formatter.write_str("the GUI asks the servers for every file in the background")
            }
            Self::NoRepository => formatter.write_str("not in a jj or git repo"),
            Self::NoMapAt {
                directory,
                revision,
                program,
            } => write!(formatter, "no {directory} in {revision} ({program})"),
            Self::Parse(error) => formatter.write_str(&parse_error(error)),
            Self::Load(error) => formatter.write_str(&load_error(error)),
            Self::Save(error) => formatter.write_str(&save_error(error)),
            Self::Stale { steps, links } => match (steps.is_zero(), links.is_zero()) {
                (false, true) => write!(formatter, "{steps} stale steps"),
                (true, false) => write!(formatter, "{links} broken links"),
                _ => write!(formatter, "{steps} stale steps, {links} broken links"),
            },
        }
    }
}
