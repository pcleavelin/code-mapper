use std::collections::{BTreeMap, BTreeSet};

use super::{Depth, Index, SymbolId, TreeEntry};
use crate::text::{LineCount, SourceLine};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cut {
    Test,
    Accessor,
    Trivial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stop {
    Mapped,
    Shared,
    OtherPackage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Kept,
    Cut(Cut),
    Stopped(Stop),
    Cycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Planned {
    pub entry: TreeEntry,
    pub verdict: Verdict,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrunedTree {
    pub entries: Vec<TreeEntry>,
    pub cut: BTreeMap<SymbolId, Cut>,
    pub stopped: BTreeMap<SymbolId, Stop>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CallerCount(usize);

const ACCESSOR_LINES: LineCount = LineCount::new(3);
const ACCESSOR_CALLERS: CallerCount = CallerCount(3);
const SHARED_CALLERS: CallerCount = CallerCount(2);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Word(&'static str);

const TEST_WORD: Word = Word("tests");
const SELF_FIELD: [Word; 2] = [Word("&self."), Word("self.")];
const SELF_BUILT: [Word; 3] = [Word("Self("), Word("Self {"), Word("Self{")];

struct Body(String);

fn trivial(body: &Body) -> bool {
    let body = body.0.trim().trim_end_matches(';').trim();
    if body.is_empty() || body.contains('\n') {
        return false;
    }
    let field = SELF_FIELD
        .iter()
        .filter_map(|prefix| body.strip_prefix(prefix.0))
        .any(|rest| {
            !rest.is_empty()
                && rest
                    .chars()
                    .all(|character| character.is_alphanumeric() || "_.".contains(character))
        });
    field || SELF_BUILT.iter().any(|prefix| body.starts_with(prefix.0))
}

impl Index {
    fn callers_of(&self, symbol: SymbolId) -> CallerCount {
        CallerCount(self.symbol(symbol).map_or(0, |found| found.callers().len()))
    }

    pub fn in_tests(&self, symbol: SymbolId) -> bool {
        let (Some(file), Some(found)) = (self.file(symbol.file()), self.symbol(symbol)) else {
            return false;
        };
        let path = file.path();
        path.as_str()
            .split('/')
            .any(|segment| segment == TEST_WORD.0)
            || path.stem() == TEST_WORD.0
            || found
                .owner()
                .is_some_and(|owner| owner.as_str() == TEST_WORD.0)
    }

    fn cut(&self, symbol: SymbolId) -> Option<Cut> {
        let file = self.file(symbol.file())?;
        let found = self.symbol(symbol)?;
        if self.in_tests(symbol) {
            return Some(Cut::Test);
        }
        if found.span().count() <= ACCESSOR_LINES && self.callers_of(symbol).0 >= ACCESSOR_CALLERS.0
        {
            return Some(Cut::Accessor);
        }
        let text = file
            .text()
            .lines(found.span())?
            .iter()
            .map(SourceLine::as_str)
            .collect::<Vec<_>>()
            .join("\n");
        let inner = text
            .split_once('{')
            .and_then(|(_, rest)| rest.rsplit_once('}'))
            .map(|(inner, _)| inner)?;
        trivial(&Body(inner.to_owned())).then_some(Cut::Trivial)
    }

    fn stop(
        &self,
        parent: SymbolId,
        callee: SymbolId,
        mapped: &impl Fn(SymbolId) -> bool,
    ) -> Option<Stop> {
        if mapped(callee) {
            return Some(Stop::Mapped);
        }
        if self.callers_of(callee).0 > SHARED_CALLERS.0 {
            return Some(Stop::Shared);
        }
        let package = |symbol: SymbolId| {
            self.file(symbol.file())
                .map(|file| file.path().package().to_owned())
        };
        (package(parent) != package(callee)).then_some(Stop::OtherPackage)
    }

    pub fn unplaced_callees(
        &self,
        path: &[SymbolId],
        placed: &BTreeSet<SymbolId>,
    ) -> Vec<SymbolId> {
        let Some(found) = path.last().and_then(|caller| self.symbol(*caller)) else {
            return Vec::new();
        };
        found
            .callees()
            .iter()
            .copied()
            .filter(|callee| path.contains(callee) || !placed.contains(callee))
            .collect()
    }

    pub fn plan_callees(
        &self,
        path: &[SymbolId],
        placed: &BTreeSet<SymbolId>,
        mapped: &impl Fn(SymbolId) -> bool,
    ) -> Vec<Planned> {
        let Some(caller) = path.last().copied() else {
            return Vec::new();
        };
        let depth = path
            .iter()
            .fold(Depth::default(), |depth, _| depth.deeper());
        self.unplaced_callees(path, placed)
            .into_iter()
            .map(|callee| Planned {
                entry: TreeEntry {
                    symbol: callee,
                    depth,
                },
                verdict: if path.contains(&callee) {
                    Verdict::Cycle
                } else if let Some(cut) = self.cut(callee) {
                    Verdict::Cut(cut)
                } else {
                    self.stop(caller, callee, mapped)
                        .map_or(Verdict::Kept, Verdict::Stopped)
                },
            })
            .collect()
    }

    pub fn planned_tree(
        &self,
        root: SymbolId,
        deepest: Depth,
        mapped: impl Fn(SymbolId) -> bool,
    ) -> Vec<Planned> {
        let mut planned = Vec::new();
        let mut placed = BTreeSet::new();
        let mut path: Vec<SymbolId> = Vec::new();
        let mut stack = vec![Planned {
            entry: TreeEntry {
                symbol: root,
                depth: Depth::default(),
            },
            verdict: Verdict::Kept,
        }];
        while let Some(next) = stack.pop() {
            let symbol = next.entry.symbol;
            if next.verdict != Verdict::Cycle && !placed.insert(symbol) {
                continue;
            }
            planned.push(next);
            if next.verdict != Verdict::Kept || next.entry.depth >= deepest {
                continue;
            }
            path.truncate(next.entry.depth.position());
            path.push(symbol);
            let children = self.plan_callees(&path, &placed, &mapped);
            stack.extend(children.into_iter().rev());
        }
        planned
    }

    pub fn pruned_tree(
        &self,
        root: SymbolId,
        deepest: Depth,
        mapped: impl Fn(SymbolId) -> bool,
    ) -> PrunedTree {
        let mut tree = PrunedTree::default();
        for planned in self.planned_tree(root, deepest, mapped) {
            let entry = planned.entry;
            match planned.verdict {
                Verdict::Kept => tree.entries.push(entry),
                Verdict::Stopped(stop) => {
                    tree.entries.push(entry);
                    tree.stopped.insert(entry.symbol, stop);
                }
                Verdict::Cut(cut) => {
                    tree.cut.entry(entry.symbol).or_insert(cut);
                }
                Verdict::Cycle => {}
            }
        }
        tree
    }
}
