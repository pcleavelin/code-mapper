use std::collections::BTreeMap;
use std::ops::Range;

use crate::text::{Line, LineCount, SourceLine, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alignment(Vec<Option<usize>>);

impl Alignment {
    pub fn between(old: &[SourceLine], new: &[SourceLine]) -> Self {
        let mut alignment = Self(vec![None; old.len()]);
        alignment.patience(old, new, 0..old.len(), 0..new.len(), (false, false));
        alignment
    }

    pub fn get(&self, line: Line) -> Option<Line> {
        self.0
            .get(line.position())
            .copied()
            .flatten()
            .and_then(Line::at)
    }

    fn set(&mut self, old: usize, new: usize) {
        if let Some(slot) = self.0.get_mut(old) {
            *slot = Some(new);
        }
    }

    fn patience(
        &mut self,
        old: &[SourceLine],
        new: &[SourceLine],
        mut before: Range<usize>,
        mut after: Range<usize>,
        paired: (bool, bool),
    ) {
        let same = |one: usize, other: usize| match (old.get(one), new.get(other)) {
            (Some(one), Some(other)) => one.trimmed() == other.trimmed(),
            _ => false,
        };
        while paired.0
            && before.start < before.end
            && after.start < after.end
            && same(before.start, after.start)
        {
            self.set(before.start, after.start);
            before.start += 1;
            after.start += 1;
        }
        while paired.1
            && before.start < before.end
            && after.start < after.end
            && same(before.end - 1, after.end - 1)
        {
            before.end -= 1;
            after.end -= 1;
            self.set(before.end, after.end);
        }
        let mut seen: BTreeMap<&str, (u32, usize, u32, usize)> = BTreeMap::new();
        for at in before.clone() {
            if let Some(line) = old.get(at) {
                let counts = seen.entry(line.trimmed()).or_default();
                counts.0 += 1;
                counts.1 = at;
            }
        }
        for at in after.clone() {
            if let Some(line) = new.get(at) {
                let counts = seen.entry(line.trimmed()).or_default();
                counts.2 += 1;
                counts.3 = at;
            }
        }
        let mut unique: Vec<(usize, usize)> = seen
            .into_values()
            .filter(|counts| counts.0 == 1 && counts.2 == 1)
            .map(|counts| (counts.1, counts.3))
            .collect();
        unique.sort_unstable();
        let run = Self::increasing(&unique);
        if run.is_empty() {
            return;
        }
        let (mut old_start, mut new_start, mut matched) = (before.start, after.start, paired.0);
        for (old_line, new_line) in run {
            self.set(old_line, new_line);
            self.patience(
                old,
                new,
                old_start..old_line,
                new_start..new_line,
                (matched, true),
            );
            (old_start, new_start, matched) = (old_line + 1, new_line + 1, true);
        }
        self.patience(
            old,
            new,
            old_start..before.end,
            new_start..after.end,
            (true, paired.1),
        );
    }

    fn increasing(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
        let mut tails: Vec<usize> = Vec::new();
        let mut previous: Vec<Option<usize>> = vec![None; pairs.len()];
        for (at, pair) in pairs.iter().enumerate() {
            let place = tails
                .partition_point(|tail| pairs.get(*tail).is_some_and(|other| other.1 < pair.1));
            if let Some(slot) = previous.get_mut(at) {
                *slot = place
                    .checked_sub(1)
                    .and_then(|below| tails.get(below).copied());
            }
            match tails.get_mut(place) {
                Some(tail) => *tail = at,
                None => tails.push(at),
            }
        }
        let mut run = Vec::new();
        let mut cursor = tails.last().copied();
        while let Some(at) = cursor {
            if let Some(pair) = pairs.get(at) {
                run.push(*pair);
            }
            cursor = previous.get(at).copied().flatten();
        }
        run.reverse();
        run
    }

    fn follow(&self, slice: Span, count: usize) -> Option<(Span, usize)> {
        let (start, end) = (slice.start().position(), slice.end().position());
        let kept = self.0.get(start..=end)?.iter().flatten().count();
        if kept == 0 {
            return None;
        }
        let first = self.0.get(start).copied().flatten().or_else(|| {
            self.0
                .get(..start)
                .and_then(|head| head.iter().rev().find_map(|found| *found))
                .map(|found| found + 1)
        });
        let last = self.0.get(end).copied().flatten().or_else(|| {
            self.0
                .get(end + 1..)
                .and_then(|tail| tail.iter().find_map(|found| *found))
                .map(|found| found.saturating_sub(1))
        });
        let span = Span::new(
            Line::at(first.unwrap_or(0))?,
            Line::at(last.unwrap_or(count.saturating_sub(1)))?,
        )?;
        Some((span, kept))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Followed {
    pub span: Span,
    pub kept: LineCount,
    pub alignment: Alignment,
}

pub fn follow(old: &[SourceLine], slice: Span, new: &[SourceLine]) -> Option<Followed> {
    let alignment = Alignment::between(old, new);
    let (span, kept) = alignment.follow(slice, new.len())?;
    Some(Followed {
        span,
        kept: LineCount::of(kept),
        alignment,
    })
}
