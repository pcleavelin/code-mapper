use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use domain::{
    Author, Backend, Change, Changed, Depth, FileId, FileText, Followed, GroupName, Index,
    Language, Line, LineCount, Map, MapError, Note, Path, PathKind, PathName, Pruning,
    RelativePath, Revision, Row, SourceFile, SourceLine, Span, Step, StepAddress, StepNumber,
    Symbol, SymbolId, SymbolName, SymbolQuery, TextFragment, follow,
};
use index::Servers;
use io_map::{MapStore, MapText};
use io_vcs::Vcs;
use regex::Regex;

use crate::convert::{
    Count, Edit, Filter, GroupPlacement, Levels, LineNumber, Lines, LinkView, Placement, Query,
    Request, StepIndex, Under, line_range, step_count, step_id, step_index,
};
use crate::failure::{Failure, StepPlace};
use crate::output::Output;
use crate::wire::{self, FollowReport, Title};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LeftStale {
    FileGone,
    MissingAt {
        file: RelativePath,
        revision: Revision,
    },
    TextGone(Revision),
    NoSymbol {
        symbol: Option<SymbolName>,
        file: RelativePath,
    },
    NoneKept,
    UnderHalfKept {
        kept: LineCount,
        length: LineCount,
    },
}

struct Target {
    file: FileId,
    span: Span,
}

struct Found {
    file: FileId,
    span: Span,
}

struct StaleCount {
    steps: Count,
    links: Count,
}

pub(crate) struct Run<'a> {
    pub(crate) index: &'a mut Index,
    pub(crate) map: &'a mut Map,
    pub(crate) author: Author,
    pub(crate) servers: Option<&'a mut Servers>,
    pub(crate) output: &'a mut Output,
}

impl Run<'_> {
    pub(crate) fn run(&mut self, request: Request) -> Result<Option<Changed>, Failure> {
        match request {
            Request::Query(query) => self.query(query).map(|()| None),
            Request::Edit(edit) => self.edit(edit).map(Some),
            Request::Repin(revision) => self.repin(revision),
        }
    }

    fn query(&mut self, query: Query) -> Result<(), Failure> {
        match query {
            Query::Files(filter) => self.files(&filter),
            Query::Symbols(filter) => self.symbols(&filter),
            Query::Show { file, start, end } => self.show(&file, start, end)?,
            Query::Search(regex) => self.search(&compile(&regex)?),
            Query::Notes(regex) => self.notes(&compile(&regex)?),
            Query::Callees(symbol) => self.callees(&symbol)?,
            Query::Callers(symbol) => self.callers(&symbol)?,
            Query::References(symbol) => self.references(&symbol)?,
            Query::Index(filter) => self.index_pending(&filter)?,
            Query::Tree { symbol, levels } => self.tree(&symbol, levels)?,
            Query::Roots(count) => self.roots(count),
            Query::Paths(name) => self.paths(name.as_ref())?,
            Query::Path { name, view } => self.path(&name, view)?,
            Query::Groups => self.groups(),
            Query::Stale => {
                self.stale();
            }
            Query::Check => self.check()?,
            Query::Uncovered(filter) => self.uncovered(&filter),
            Query::Coverage => self.coverage(),
            Query::Diff => self.diff()?,
        }
        Ok(())
    }

    fn edit(&mut self, edit: Edit) -> Result<Changed, Failure> {
        match edit {
            Edit::PathNew {
                name,
                kind,
                note,
                group,
            } => self.path_new(&name?, kind, note, group),
            Edit::PathGroup { name, group } => self.path_group(&name, group),
            Edit::GroupRename { old, new } => self.group_rename(old.as_ref(), new.as_ref()),
            Edit::PathNote { name, note } => self.path_note(&name, note),
            Edit::StepNote { name, step, note } => self.step_note(&name, step, note),
            Edit::StepLink { name, step, target } => self.step_link(&name, step, &target),
            Edit::StepUnlink { name, step } => self.step_unlink(&name, step),
            Edit::NoteReplace {
                name,
                step,
                old,
                new,
            } => self.note_edit(&name, step, &old, &new),
            Edit::PathRename { name, new } => self.path_rename(&name, new),
            Edit::PathAdd { name, placement } => self.path_add(&name, &placement),
            Edit::PathPin {
                name,
                step,
                file,
                lines,
            } => self.path_pin(&name, step, &file, lines),
            Edit::PathMove { name, step, under } => self.path_move(&name, step, under),
            Edit::PathSwap { name, one, other } => self.path_swap(&name, one, other),
            Edit::PathRemove { name, step } => self.path_remove(&name, step),
            Edit::Promote {
                symbol,
                levels,
                name,
                pruning,
            } => self.promote(&symbol, levels, name, pruning),
        }
    }

    fn files(&mut self, filter: &Filter) {
        for file in self
            .index
            .files()
            .filter(|file| filter.matches(file.path().as_str()))
        {
            wire::file_row(self.output, file);
        }
    }

    fn symbols(&mut self, filter: &Filter) {
        for file in self.index.files() {
            for symbol in file.symbols().filter(|symbol| {
                filter.matches(symbol.name().as_str()) || filter.matches(file.path().as_str())
            }) {
                wire::symbol_row(self.output, file, symbol);
            }
        }
    }

    fn show(
        &mut self,
        file: &RelativePath,
        start: Option<LineNumber>,
        end: Option<LineNumber>,
    ) -> Result<(), Failure> {
        let id = find_file(self.index, file)?;
        if let Some(source) = self.index.file(id)
            && let Some(span) = LineNumber::visible(source, start, end)
        {
            wire::numbered_lines(self.output, source, span);
        }
        Ok(())
    }

    fn search(&mut self, regex: &Regex) {
        for file in self.index.files() {
            for (line, text) in (0..).map(Line::new).zip(file.text().all()) {
                if regex.is_match(text.as_str()) {
                    wire::hit_line(self.output, file, line, text);
                }
            }
        }
    }

    fn notes(&mut self, regex: &Regex) {
        for path in self.map.paths() {
            for text in path
                .note()
                .into_iter()
                .flat_map(Note::lines)
                .filter(|text| regex.is_match(text))
            {
                wire::path_note_line(self.output, path.name(), text);
            }
            for (position, step) in path.steps().iter().enumerate() {
                for text in step
                    .note()
                    .into_iter()
                    .flat_map(Note::lines)
                    .filter(|text| regex.is_match(text))
                {
                    let position = StepIndex::new(position);
                    wire::step_note_line(self.output, path.name(), position, step, text);
                }
            }
        }
    }

    fn callees(&mut self, symbol: &SymbolName) -> Result<(), Failure> {
        let files = find_symbols(self.index, symbol)?
            .into_iter()
            .filter_map(|id| self.index.file(id.file()))
            .map(|file| file.path().clone())
            .collect();
        self.index_files(files);
        for id in find_symbols(self.index, symbol)? {
            wire::symbol_line(self.output, self.index, id, Depth::new(0));
            let callees = self
                .index
                .symbol(id)
                .map(Symbol::callees)
                .unwrap_or_default();
            for callee in callees {
                wire::symbol_line(self.output, self.index, *callee, Depth::new(1));
            }
        }
        Ok(())
    }

    fn callers(&mut self, symbol: &SymbolName) -> Result<(), Failure> {
        for id in find_symbols(self.index, symbol)? {
            wire::symbol_line(self.output, self.index, id, Depth::new(0));
            let asked = self
                .servers
                .as_deref_mut()
                .and_then(|servers| servers.incoming_calls(self.index, id));
            let callers: Vec<SymbolId> = match asked {
                Some(calls) => {
                    let mut found: Vec<SymbolId> = calls
                        .iter()
                        .filter_map(|call| self.index.by_line(&call.file, call.line))
                        .collect();
                    found.dedup();
                    found
                }
                None => self
                    .index
                    .symbol(id)
                    .map(|found| found.callers().to_vec())
                    .unwrap_or_default(),
            };
            for caller in callers {
                wire::symbol_line(self.output, self.index, caller, Depth::new(1));
            }
        }
        Ok(())
    }

    fn references(&mut self, symbol: &SymbolName) -> Result<(), Failure> {
        for id in find_symbols(self.index, symbol)? {
            wire::symbol_line(self.output, self.index, id, Depth::new(0));
            let asked = self
                .servers
                .as_deref_mut()
                .and_then(|servers| servers.references(self.index, id));
            let source = self.index.file(id.file());
            let references = match asked {
                Some(references) => references,
                None if self.servers.is_none()
                    && source.is_some_and(|found| found.backend() == Backend::Server) =>
                {
                    self.index
                        .symbol(id)
                        .map(|found| found.references().to_vec())
                        .unwrap_or_default()
                }
                None => {
                    let program = source
                        .and_then(|found| Language::of(found.path()))
                        .map(Language::program);
                    wire::server_missing(self.output, program);
                    Vec::new()
                }
            };
            for location in &references {
                let text = self
                    .index
                    .find_file(&location.file)
                    .and_then(|file| self.index.file(file))
                    .and_then(|file| file.text().line(location.line));
                wire::reference_row(self.output, location, text);
            }
        }
        Ok(())
    }

    fn index_pending(&mut self, filter: &Filter) -> Result<(), Failure> {
        if self.servers.is_none() {
            return Err(Failure::ServersInBackground);
        }
        let files: Vec<RelativePath> = self
            .index
            .files()
            .filter(|file| file.is_pending() && filter.matches(file.path().as_str()))
            .map(|file| file.path().clone())
            .collect();
        let asked = Count::new(files.len());
        self.index_files(files);
        let left = Count::new(self.index.files().filter(|file| file.is_pending()).count());
        wire::index_report(self.output, asked, left);
        Ok(())
    }

    fn tree(&mut self, symbol: &SymbolName, levels: Levels) -> Result<(), Failure> {
        self.index_tree(symbol, levels.depth())?;
        for id in find_symbols(self.index, symbol)? {
            for entry in self.index.call_tree(id, levels.depth()) {
                wire::symbol_line(self.output, self.index, entry.symbol, entry.depth);
            }
        }
        Ok(())
    }

    fn roots(&mut self, count: Count) {
        for id in self.index.roots().into_iter().take(count.value()) {
            wire::root_row(self.output, self.index, id);
        }
    }

    fn paths(&mut self, name: Option<&TextFragment>) -> Result<(), Failure> {
        let names: Vec<PathName> = match name {
            Some(name) => vec![find_path(self.map, name)?],
            None => self
                .map
                .rows()
                .into_iter()
                .filter_map(|row| match row {
                    Row::Path { name: named, .. } => Some(named),
                    Row::Group { .. } => None,
                })
                .collect(),
        };
        let mut group: Option<&GroupName> = None;
        for path in names.iter().filter_map(|named| self.map.path(named)) {
            if name.is_none() && path.group() != group {
                group = path.group();
                wire::group_header(self.output, group);
            }
            wire::path_row(self.output, path);
            for placed in path.tree_order() {
                if let (Some(step), Some(position)) =
                    (path.step(&placed.step), step_index(path, &placed.step))
                {
                    wire::step_row(self.output, self.index, step, position, placed.depth);
                }
            }
        }
        Ok(())
    }

    fn path(&mut self, name: &TextFragment, view: LinkView) -> Result<(), Failure> {
        let name = find_path(self.map, name)?;
        let inlined = (view == LinkView::Inlined).then(|| vec![name.clone()]);
        let Some(path) = self.map.path(&name) else {
            return Ok(());
        };
        wire::path_title(self.output, path);
        let from = places(self.map, self.map.links_to(&name));
        wire::linked_from(self.output, &from);
        let mut document = Document {
            index: self.index,
            map: self.map,
            output: self.output,
            inlined,
        };
        document.steps(path, Depth::new(0), &[]);
        Ok(())
    }

    fn groups(&mut self) {
        for row in self.map.rows() {
            if let Row::Group {
                group,
                depth,
                paths,
            } = row
            {
                wire::group_row(self.output, &group, depth, paths);
            }
        }
    }

    fn stale(&mut self) -> StaleCount {
        let mut steps = Count::default();
        for path in self.map.paths() {
            for (position, step) in path.steps().iter().enumerate() {
                if !step.is_stale() {
                    continue;
                }
                let position = StepIndex::new(position);
                steps = steps.next();
                wire::stale_row(self.output, self.index, path.name(), position, step);
                let source = self
                    .index
                    .find_file(step.file())
                    .and_then(|file| self.index.file(file));
                if let Some(span) = source.and_then(|source| moved_to(source.text(), step)) {
                    wire::same_text(self.output, path.name(), position, step.file(), span);
                } else if let (None, Some(symbol)) = (step.resolved_symbol(), step.symbol()) {
                    let query = SymbolQuery::from(symbol.as_str());
                    for id in self.index.find_symbols(&query) {
                        if let (Some(file), Some(found)) =
                            (self.index.file(id.file()), self.index.symbol(id))
                        {
                            let span = found.span();
                            wire::same_name(self.output, path.name(), position, file.path(), span);
                        }
                    }
                }
            }
        }
        let dangling = self.map.dangling_links();
        for address in &dangling {
            let Some(path) = self.map.path(&address.path) else {
                continue;
            };
            if let (Some(position), Some(link)) = (
                step_index(path, &address.step),
                path.step(&address.step).and_then(Step::link),
            ) {
                wire::dangling(self.output, path.name(), position, link);
            }
        }
        StaleCount {
            steps,
            links: Count::new(dangling.len()),
        }
    }

    fn check(&mut self) -> Result<(), Failure> {
        let count = self.stale();
        if count.steps.is_zero() && count.links.is_zero() {
            wire::all_fresh(self.output);
            return Ok(());
        }
        Err(Failure::Stale {
            steps: count.steps,
            links: count.links,
        })
    }

    fn uncovered(&mut self, filter: &Filter) {
        let mut found: Vec<(LineCount, SymbolId)> = Vec::new();
        let coverage = self.map.coverage();
        for id in self.index.symbol_ids() {
            let (Some(file), Some(symbol)) = (self.index.file(id.file()), self.index.symbol(id))
            else {
                continue;
            };
            let matched =
                filter.matches(symbol.name().as_str()) || filter.matches(file.path().as_str());
            if matched && !coverage.covers(file.path(), symbol.span()) {
                found.push((symbol.span().count(), id));
            }
        }
        found.sort_by_key(|pair| Reverse(pair.0));
        for (_, id) in found {
            if let (Some(file), Some(symbol)) = (self.index.file(id.file()), self.index.symbol(id))
            {
                wire::uncovered_row(self.output, file, symbol);
            }
        }
    }

    fn coverage(&mut self) {
        let (mut covered, mut symbols) = (Count::default(), Count::default());
        let coverage = self.map.coverage();
        for file in self
            .index
            .files()
            .filter(|file| file.symbols().next().is_some())
        {
            let here = Count::new(
                file.symbols()
                    .filter(|symbol| coverage.covers(file.path(), symbol.span()))
                    .count(),
            );
            let all = Count::new(file.symbols().count());
            wire::coverage_row(self.output, file, here, all);
            covered = covered.plus(here);
            symbols = symbols.plus(all);
        }
        wire::all_coverage(self.output, covered, symbols);
    }

    fn diff(&mut self) -> Result<(), Failure> {
        let vcs = Vcs::detect(self.index.root())
            .ok()
            .ok_or(Failure::NoRepository)?;
        let parent = vcs.parent();
        let directory = MapStore::relative_directory();
        let text = vcs
            .map_at(&parent, &directory)
            .ok()
            .ok_or_else(|| Failure::NoMapAt {
                directory,
                revision: parent.clone(),
                program: vcs.program(),
            })?;
        let base = MapStore::base(&MapText::new(text.as_str()), &parent).map_err(Failure::Parse)?;
        for difference in self.map.diff(&base) {
            let name = difference.name();
            match difference.change() {
                Change::Same => continue,
                Change::Added => {
                    let steps = Count::new(difference.steps().len());
                    wire::path_added(self.output, name, steps);
                    continue;
                }
                Change::Removed => {
                    let steps = Count::new(difference.removed().len());
                    wire::path_removed(self.output, name, steps);
                    continue;
                }
                Change::Changed => {
                    wire::path_changed(self.output, name, difference.note_changed());
                }
            }
            let name = find_path(self.map, &TextFragment::new(name.as_str()))?;
            let Some(path) = self.map.path(&name) else {
                continue;
            };
            for (position, change) in difference.steps().iter().enumerate() {
                if let (Some(kind), Some(step)) = (change.change, path.steps().get(position)) {
                    let position = StepIndex::new(position);
                    wire::step_change(self.output, self.index, kind, position, step);
                }
            }
            for step in difference.removed() {
                wire::step_removed(self.output, step);
            }
        }
        Ok(())
    }

    fn repin(&mut self, revision: Option<Revision>) -> Result<Option<Changed>, Failure> {
        let vcs = Vcs::detect(self.index.root())
            .ok()
            .ok_or(Failure::NoRepository)?;
        let revision = revision.unwrap_or_else(|| vcs.parent());
        let mut olds: BTreeMap<RelativePath, Option<FileText>> = BTreeMap::new();
        let stale: Vec<(PathName, StepIndex, Step)> = self
            .map
            .paths()
            .iter()
            .flat_map(|path| {
                path.steps()
                    .iter()
                    .enumerate()
                    .filter(|pair| pair.1.is_stale())
                    .map(|pair| (path.name().clone(), StepIndex::new(pair.0), pair.1.clone()))
            })
            .collect();
        let (mut pinned, mut left) = (Count::default(), Count::default());
        for (name, position, step) in stale {
            let old = olds
                .entry(step.file().clone())
                .or_insert_with(|| vcs.file_at(&revision, step.file()).ok());
            let repin = Repin {
                index: self.index,
                step: &step,
                revision: &revision,
                path: &name,
                position,
            };
            match repin.follow(old.as_ref(), self.output) {
                Ok(found) => {
                    let _pinned = self.map.pin(
                        self.index,
                        &name,
                        step.id(),
                        found.file,
                        found.span,
                        self.author,
                    )?;
                    pinned = pinned.next();
                }
                Err(reason) => {
                    wire::left_stale(self.output, &name, position, &reason);
                    left = left.next();
                }
            }
        }
        wire::repin_report(self.output, pinned, left);
        Ok((!pinned.is_zero()).then_some(Changed))
    }

    fn path_new(
        &mut self,
        name: &PathName,
        kind: PathKind,
        note: Option<Note>,
        group: GroupPlacement,
    ) -> Result<Changed, Failure> {
        let _added = self.map.add_path(name.clone(), kind, self.author)?;
        if let Some(note) = note {
            let _set = self.map.set_path_note(name, Some(note))?;
        }
        if let GroupPlacement::At(group) = group {
            let _placed = self.map.set_group(name, group)?;
        }
        Ok(Changed)
    }

    fn path_group(
        &mut self,
        name: &TextFragment,
        group: Option<GroupName>,
    ) -> Result<Changed, Failure> {
        let name = find_path(self.map, name)?;
        let _placed = self.map.set_group(&name, group)?;
        let placed = self.map.path(&name).and_then(Path::group);
        wire::group_place(self.output, &name, placed);
        Ok(Changed)
    }

    fn group_rename(
        &mut self,
        old: Option<&GroupName>,
        new: Option<&GroupName>,
    ) -> Result<Changed, Failure> {
        let moved = self.map.rename_group(old, new)?;
        wire::paths_moved(self.output, moved);
        Ok(Changed)
    }

    fn path_note(&mut self, name: &TextFragment, note: Option<Note>) -> Result<Changed, Failure> {
        let name = find_path(self.map, name)?;
        Ok(self.map.set_path_note(&name, note)?)
    }

    fn step_note(
        &mut self,
        name: &TextFragment,
        step: StepIndex,
        note: Option<Note>,
    ) -> Result<Changed, Failure> {
        let found = find_step(self.map, name, step)?;
        Ok(self.map.set_step_note(&found.path, &found.step, note)?)
    }

    fn step_link(
        &mut self,
        name: &TextFragment,
        position: StepIndex,
        target: &TextFragment,
    ) -> Result<Changed, Failure> {
        let found = find_step(self.map, name, position)?;
        if target.as_str().is_empty() {
            return Err(Failure::NoLinkTarget);
        }
        let link = PathName::new(target.as_str())
            .ok()
            .filter(|link| self.map.path(link).is_some())
            .ok_or_else(|| Failure::NoSuchPath(target.clone()))?;
        let _linked = self.map.set_link(&found.path, &found.step, Some(link))?;
        wire::step_linked(self.output, position, target);
        Ok(Changed)
    }

    fn step_unlink(
        &mut self,
        name: &TextFragment,
        position: StepIndex,
    ) -> Result<Changed, Failure> {
        let found = find_step(self.map, name, position)?;
        let linked = self.map.step(&found.path, &found.step).and_then(Step::link);
        if linked.is_none() {
            return Err(Failure::NoLink(position));
        }
        let _removed = self.map.set_link(&found.path, &found.step, None)?;
        wire::link_removed(self.output, position);
        Ok(Changed)
    }

    fn note_edit(
        &mut self,
        name: &TextFragment,
        step: Option<StepIndex>,
        old: &TextFragment,
        new: &TextFragment,
    ) -> Result<Changed, Failure> {
        let note = match step {
            None => {
                let path = find_path(self.map, name)?;
                self.map.edit_path_note(&path, old, new)?
            }
            Some(position) => {
                let found = find_step(self.map, name, position)?;
                self.map
                    .edit_step_note(&found.path, &found.step, old, new)?
            }
        };
        wire::note_text(self.output, note.as_ref());
        Ok(Changed)
    }

    fn path_rename(
        &mut self,
        name: &TextFragment,
        new: Result<PathName, MapError>,
    ) -> Result<Changed, Failure> {
        let old = find_path(self.map, name)?;
        let links = Count::new(self.map.links_to(&old).len());
        let new = new?;
        let _moved = self.map.rename(&old, new.clone())?;
        if !links.is_zero() {
            wire::links_moved(self.output, links, &new);
        }
        Ok(Changed)
    }

    fn path_add(&mut self, name: &TextFragment, placement: &Placement) -> Result<Changed, Failure> {
        let guessed = placement.under.unwrap_or_else(|| {
            PathName::new(name.as_str())
                .ok()
                .and_then(|name| self.map.path(&name).map(step_count))
                .map_or(Under::new(-1), Count::last)
        });
        self.index_parent(name, guessed);
        let path = find_path(self.map, name)?;
        let steps = self.map.path(&path).map(step_count).unwrap_or_default();
        let under = placement.under.unwrap_or_else(|| steps.last());
        if !under.fits(steps) {
            return Err(Failure::NoPlaceUnder { under, steps });
        }
        let parent = under
            .step()
            .and_then(|under| self.map.path(&path).and_then(|found| step_id(found, under)));
        let target = placement.target.as_str();
        let (file, span) = if let Some(lines) = placement.lines {
            let file = find_file(self.index, &RelativePath::new(target))?;
            let span = self
                .index
                .file(file)
                .and_then(|source| line_range(source, lines.start, lines.end))
                .ok_or(Failure::LineRange)?;
            (file, span)
        } else {
            let symbol = find_symbol(self.index, &SymbolName::new(target))?;
            let span = self
                .index
                .symbol(symbol)
                .map(Symbol::span)
                .ok_or(MapError::NoSuchSymbol)?;
            (symbol.file(), span)
        };
        let added =
            self.map
                .add_step(self.index, &path, file, span, self.author, parent.as_ref())?;
        let address = StepAddress { path, step: added };
        if let Some(found) = self.map.path(&address.path)
            && let (Some(step), Some(position)) =
                (found.step(&address.step), step_index(found, &address.step))
        {
            let parent_index = step.parent().and_then(|id| step_index(found, id));
            let named = step.symbol().filter(|_| placement.lines.is_none());
            wire::step_added(self.output, position, named, Under::of(parent_index));
            if placement.lines.is_some() {
                wire::absolute_note(self.output, step);
            }
        }
        self.call_note(&address);
        Ok(Changed)
    }

    fn path_pin(
        &mut self,
        name: &TextFragment,
        position: StepIndex,
        file: &RelativePath,
        lines: Lines,
    ) -> Result<Changed, Failure> {
        let found = find_step(self.map, name, position)?;
        let file = find_file(self.index, file)?;
        let span = self
            .index
            .file(file)
            .and_then(|source| line_range(source, lines.start, lines.end))
            .ok_or(Failure::LineRange)?;
        let _pinned = self.map.pin(
            self.index,
            &found.path,
            &found.step,
            file,
            span,
            self.author,
        )?;
        if let Some(step) = self.map.step(&found.path, &found.step) {
            wire::pinned(self.output, self.index, position, step);
            wire::absolute_note(self.output, step);
        }
        Ok(Changed)
    }

    fn path_move(
        &mut self,
        name: &TextFragment,
        position: StepIndex,
        under: Under,
    ) -> Result<Changed, Failure> {
        self.index_parent(name, under);
        let found = find_step(self.map, name, position)?;
        let parent = match under.step() {
            None => None,
            Some(parent) => Some(
                self.map
                    .path(&found.path)
                    .and_then(|path| step_id(path, parent))
                    .ok_or(MapError::NoSuchParent)?,
            ),
        };
        let _moved = self
            .map
            .reparent(&found.path, &found.step, parent.as_ref())?;
        wire::moved_under(self.output, position, under);
        self.call_note(&found);
        Ok(Changed)
    }

    fn path_swap(
        &mut self,
        name: &TextFragment,
        one: StepIndex,
        other: StepIndex,
    ) -> Result<Changed, Failure> {
        let found = find_step(self.map, name, one.max(other))?;
        let path = self.map.path(&found.path).ok_or(Failure::NoSuchStep)?;
        let first = step_id(path, one).ok_or(Failure::NoSuchStep)?;
        let second = step_id(path, other).ok_or(Failure::NoSuchStep)?;
        Ok(self.map.swap(&found.path, &first, &second)?)
    }

    fn path_remove(
        &mut self,
        name: &TextFragment,
        step: Option<StepIndex>,
    ) -> Result<Changed, Failure> {
        let name = find_path(self.map, name)?;
        match step {
            Some(position) => {
                let id = self
                    .map
                    .path(&name)
                    .and_then(|path| step_id(path, position))
                    .ok_or(Failure::NoSuchStep)?;
                let _removed = self.map.remove_step(&name, &id)?;
            }
            None => match self.map.remove_path(&name) {
                Ok(_) => {}
                Err(MapError::LinkedFrom { path, steps }) => {
                    let steps = places(self.map, steps);
                    return Err(Failure::LinkedFrom { path, steps });
                }
                Err(error) => return Err(error.into()),
            },
        }
        Ok(Changed)
    }

    fn promote(
        &mut self,
        symbol: &SymbolName,
        levels: Option<Levels>,
        name: Result<Option<PathName>, MapError>,
        pruning: Pruning,
    ) -> Result<Changed, Failure> {
        let depth = levels.map_or(Map::PROMOTE_DEPTH, Levels::depth);
        self.index_tree(symbol, depth)?;
        if pruning == Pruning::Pruned {
            let language = find_symbol(self.index, symbol)
                .ok()
                .and_then(|root| self.index.file(root.file()))
                .and_then(SourceFile::language);
            let pending: Vec<RelativePath> = self
                .index
                .files()
                .filter(|file| file.is_pending() && file.language() == language)
                .map(|file| file.path().clone())
                .collect();
            self.index_files(pending);
        }
        let root = find_symbol(self.index, symbol)?;
        let promoted = self
            .map
            .promote(self.index, root, depth, name?, self.author, pruning)?;
        if let Some(path) = self.map.path(&promoted.name) {
            wire::promote_report(self.output, self.index, path, &promoted);
        }
        Ok(Changed)
    }

    fn call_note(&mut self, address: &StepAddress) {
        let Some(path) = self.map.path(&address.path) else {
            return;
        };
        if path.kind() != PathKind::Flow {
            return;
        }
        let Some(step) = path.step(&address.step) else {
            return;
        };
        let Some(parent) = step.parent().and_then(|parent| path.step(parent)) else {
            return;
        };
        let (Some(from), Some(to), Some(position)) = (
            parent.resolved_symbol(),
            step.resolved_symbol(),
            step_index(path, &address.step),
        ) else {
            return;
        };
        let function = self
            .index
            .symbol(to)
            .is_some_and(|symbol| wire::is_function(symbol.kind()));
        let calls = self
            .index
            .symbol(from)
            .is_some_and(|symbol| symbol.callees().contains(&to));
        if from != to && function && !calls {
            wire::call_note(self.output, parent, step, position);
        }
    }

    fn index_files(&mut self, files: Vec<RelativePath>) {
        let Some(servers) = self.servers.as_deref_mut() else {
            return;
        };
        let files: Vec<RelativePath> = files
            .into_iter()
            .filter(|path| {
                self.index
                    .find_file(path)
                    .and_then(|file| self.index.file(file))
                    .is_some_and(SourceFile::is_pending)
            })
            .collect();
        if files.is_empty() {
            return;
        }
        servers.index(self.index, &files);
        self.map.resolve_all(self.index);
    }

    fn index_tree(&mut self, symbol: &SymbolName, depth: Depth) -> Result<(), Failure> {
        let mut asked: BTreeSet<RelativePath> = BTreeSet::new();
        loop {
            let found: Vec<RelativePath> = find_symbols(self.index, symbol)?
                .into_iter()
                .flat_map(|id| self.index.call_tree(id, depth))
                .filter_map(|entry| self.index.file(entry.symbol.file()))
                .filter(|file| file.is_pending() && !asked.contains(file.path()))
                .map(|file| file.path().clone())
                .collect();
            if found.is_empty() || self.servers.is_none() {
                return Ok(());
            }
            asked.extend(found.iter().cloned());
            self.index_files(found);
        }
    }

    fn index_parent(&mut self, name: &TextFragment, under: Under) {
        let file = PathName::new(name.as_str())
            .ok()
            .and_then(|name| self.map.path(&name))
            .zip(under.step())
            .and_then(|found| step_id(found.0, found.1).and_then(|id| found.0.step(&id).cloned()))
            .map(|step| step.file().clone());
        self.index_files(file.into_iter().collect());
    }
}

struct Document<'a> {
    index: &'a Index,
    map: &'a Map,
    output: &'a mut Output,
    inlined: Option<Vec<PathName>>,
}

impl Document<'_> {
    fn steps(&mut self, path: &Path, base: Depth, prefix: &[StepNumber]) {
        let mut previous = Depth::new(0);
        for numbered in path.numbered(self.index) {
            let (Some(step), Some(position)) =
                (path.step(&numbered.step), step_index(path, &numbered.step))
            else {
                continue;
            };
            let depth = Depth::new(base.value().saturating_add(numbered.depth.value()));
            if numbered.depth < previous
                && let Some(parent) = path.parent_label(&numbered.step)
            {
                wire::back_in(self.output, depth, &parent);
            }
            previous = numbered.depth;
            let title = Title {
                depth,
                prefix,
                number: &numbered.number,
                nested: (base.value() > 0).then_some(path.name()),
                position,
            };
            wire::step_title(self.output, self.index, step, &title);
            if let Some(source) = self
                .index
                .find_file(step.file())
                .and_then(|file| self.index.file(file))
            {
                wire::numbered_lines(self.output, source, step.span());
            }
            self.inline_link(step, depth, prefix, &numbered.number);
        }
    }

    fn inline_link(
        &mut self,
        step: &Step,
        depth: Depth,
        prefix: &[StepNumber],
        number: &StepNumber,
    ) {
        let map = self.map;
        let (Some(link), Some(inlined)) = (step.link(), self.inlined.as_mut()) else {
            return;
        };
        let Some(target) = map.path(link) else {
            return;
        };
        if inlined.contains(link) {
            wire::inlined_before(self.output, depth, link);
            return;
        }
        inlined.push(link.clone());
        let mut deeper: Vec<StepNumber> = prefix.to_vec();
        deeper.push(number.clone());
        self.steps(target, depth.deeper(), &deeper);
        if let Some(shown) = self.inlined.as_mut() {
            shown.pop();
        }
        wire::end_of(self.output, depth, link);
    }
}

struct Repin<'a> {
    index: &'a Index,
    step: &'a Step,
    revision: &'a Revision,
    path: &'a PathName,
    position: StepIndex,
}

impl Repin<'_> {
    fn targets(&self, here: Option<FileId>) -> Vec<Target> {
        let named = |file: FileId, name: &SymbolName| -> Vec<Target> {
            self.index
                .file(file)
                .map(|source| {
                    source
                        .symbols()
                        .filter(|symbol| symbol.name() == name)
                        .map(|symbol| Target {
                            file,
                            span: symbol.span(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        match (here, self.step.symbol()) {
            (Some(file), None) => {
                let span = self
                    .index
                    .file(file)
                    .and_then(|source| source.text().span())
                    .unwrap_or(Span::line(Line::new(0)));
                vec![Target { file, span }]
            }
            (None, None) => Vec::new(),
            (_, Some(name)) => here
                .map(|file| named(file, name))
                .filter(|targets| !targets.is_empty())
                .unwrap_or_else(|| {
                    self.index
                        .file_entries()
                        .flat_map(|entry| named(entry.id, name))
                        .collect()
                }),
        }
    }

    fn follow(&self, old: Option<&FileText>, output: &mut Output) -> Result<Found, LeftStale> {
        let step = self.step;
        let here = self.index.find_file(step.file());
        let targets = self.targets(here);
        if here.is_none() && targets.is_empty() {
            return Err(LeftStale::FileGone);
        }
        let old = old.ok_or_else(|| LeftStale::MissingAt {
            file: step.file().clone(),
            revision: self.revision.clone(),
        })?;
        let text_gone = || LeftStale::TextGone(self.revision.clone());
        let anchor = step.anchor();
        let length =
            u32::try_from(i64::from(anchor.end().value()) - i64::from(anchor.start().value()) + 1)
                .ok()
                .filter(|length| *length > 0)
                .map(LineCount::new)
                .ok_or_else(text_gone)?;
        let pinned_at = step.span().start();
        let start = (0..=old.count().value().saturating_sub(length.value()))
            .map(Line::new)
            .filter(|line| {
                old.count() >= length
                    && span_of(*line, length).and_then(|span| old.hash(span)) == Some(anchor.hash())
            })
            .min_by_key(|line| line.distance(pinned_at))
            .ok_or_else(text_gone)?;
        let window_start = Line::new(start.value().saturating_sub(3));
        let window_end = start
            .value()
            .saturating_add(length.value())
            .saturating_add(3)
            .min(old.count().value());
        let window = Span::new(window_start, Line::new(window_end.saturating_sub(1)))
            .and_then(|span| old.lines(span))
            .ok_or_else(text_gone)?;
        let slice = span_of(Line::new(start.value() - window_start.value()), length)
            .ok_or_else(text_gone)?;
        if targets.is_empty() {
            return Err(LeftStale::NoSymbol {
                symbol: step.symbol().cloned(),
                file: step.file().clone(),
            });
        }
        let (target, lines, followed) = targets
            .iter()
            .filter_map(|target| {
                let lines = self.index.file(target.file)?.text().lines(target.span)?;
                let followed = follow(window, slice, lines)?;
                Some((target, lines, followed))
            })
            .max_by_key(|candidate| {
                let at = candidate.0.span.start().value() + candidate.2.span.start().value();
                (candidate.2.kept, Reverse(Line::new(at).distance(pinned_at)))
            })
            .ok_or(LeftStale::NoneKept)?;
        let kept = followed.kept;
        if kept.value().saturating_mul(2) < length.value() {
            return Err(LeftStale::UnderHalfKept { kept, length });
        }
        let base = target.span.start().value();
        let new = Span::new(
            Line::new(base + followed.span.start().value()),
            Line::new(base + followed.span.end().value()),
        )
        .ok_or(LeftStale::NoneKept)?;
        let source = self.index.file(target.file).ok_or(LeftStale::FileGone)?;
        let same = new.count() == length && source.text().hash(new) == Some(anchor.hash());
        let report = FollowReport {
            path: self.path,
            position: self.position,
            file: step.file(),
            old: span_of(start, length).ok_or_else(text_gone)?,
            revision: self.revision,
            moved: (Some(target.file) != here).then_some(source.path()),
            new,
            kept,
            length,
            same,
        };
        wire::follow_report(output, &report);
        if !same {
            changed_lines(output, window, slice, lines, &followed);
        }
        Ok(Found {
            file: target.file,
            span: new,
        })
    }
}

fn changed_lines(
    output: &mut Output,
    window: &[SourceLine],
    slice: Span,
    lines: &[SourceLine],
    followed: &Followed,
) {
    let text = |source: &[SourceLine], line: Line| -> Option<SourceLine> {
        usize::try_from(line.value())
            .ok()
            .and_then(|at| source.get(at))
            .cloned()
    };
    let (mut old, mut new) = (slice.start(), followed.span.start());
    let (old_end, new_end) = (slice.end(), followed.span.end());
    while old <= old_end || new <= new_end {
        let paired = followed.alignment.get(old);
        if old <= old_end && paired.is_none() {
            if let Some(line) = text(window, old) {
                wire::removed_line(output, &line);
            }
            old = Line::new(old.value() + 1);
        } else if new <= new_end && (old > old_end || paired.is_some_and(|other| new < other)) {
            if let Some(line) = text(lines, new) {
                wire::added_line(output, &line);
            }
            new = Line::new(new.value() + 1);
        } else {
            old = Line::new(old.value() + 1);
            new = Line::new(new.value() + 1);
        }
    }
}

fn span_of(start: Line, length: LineCount) -> Option<Span> {
    let last = start.value().checked_add(length.value().checked_sub(1)?)?;
    Span::new(start, Line::new(last))
}

fn moved_to(text: &FileText, step: &Step) -> Option<Span> {
    let anchor = step.anchor();
    let length = u32::try_from(anchor.end().value() - anchor.start().value()).ok()?;
    let end = text.count().value().checked_sub(length)?;
    (0..end)
        .map(Line::new)
        .filter_map(|start| Span::new(start, Line::new(start.value() + length)))
        .find(|span| *span != step.span() && text.hash(*span) == Some(anchor.hash()))
}

fn compile(regex: &TextFragment) -> Result<Regex, Failure> {
    Regex::new(regex.as_str()).map_err(Failure::Regex)
}

pub(crate) fn map_failure(map: &Map, error: MapError) -> Failure {
    match error {
        MapError::LinkedFrom { path, steps } => Failure::LinkedFrom {
            path,
            steps: places(map, steps),
        },
        other => Failure::Map(other),
    }
}

fn places(map: &Map, addresses: Vec<StepAddress>) -> Vec<StepPlace> {
    addresses
        .into_iter()
        .filter_map(|address| {
            let index = map
                .path(&address.path)
                .and_then(|path| step_index(path, &address.step))?;
            Some(StepPlace {
                path: address.path,
                index,
            })
        })
        .collect()
}

fn find_file(index: &Index, path: &RelativePath) -> Result<FileId, Failure> {
    index
        .find_file(path)
        .or_else(|| {
            index
                .file_entries()
                .find(|entry| entry.item.path().ends_with(path.as_str()))
                .map(|entry| entry.id)
        })
        .ok_or_else(|| Failure::NoSuchFile(path.clone()))
}

fn find_symbols(index: &Index, symbol: &SymbolName) -> Result<Vec<SymbolId>, Failure> {
    let found = index.find_symbols(&SymbolQuery::from(symbol.as_str()));
    if found.is_empty() {
        return Err(Failure::NoSuchSymbol(symbol.clone()));
    }
    Ok(found)
}

fn find_symbol(index: &Index, symbol: &SymbolName) -> Result<SymbolId, Failure> {
    let found = find_symbols(index, symbol)?;
    match found.as_slice() {
        [one] => Ok(*one),
        _ => Err(Failure::SymbolClash {
            query: symbol.clone(),
            candidates: found.iter().map(|id| wire::candidate(index, *id)).collect(),
        }),
    }
}

fn find_path(map: &Map, name: &TextFragment) -> Result<PathName, Failure> {
    PathName::new(name.as_str())
        .ok()
        .filter(|found| map.path(found).is_some())
        .ok_or_else(|| Failure::NoSuchPath(name.clone()))
}

fn find_step(map: &Map, name: &TextFragment, position: StepIndex) -> Result<StepAddress, Failure> {
    let path = find_path(map, name)?;
    let step = map
        .path(&path)
        .and_then(|found| step_id(found, position))
        .ok_or(Failure::NoSuchStep)?;
    Ok(StepAddress { path, step })
}
