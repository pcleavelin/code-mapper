use std::collections::BTreeMap;
use std::rc::Rc;

use domain::{FileId, Line, SourceFile, Span, TextHash};
use ui::{Cell, Count, Glyph, Grid};

use crate::theme::{self, TEXT, WEAK};

pub(crate) const GUTTER: Count = Count::new(6);

pub(crate) fn build(file: &SourceFile, span: Span) -> Grid {
    let text = file.text();
    let widest = span
        .lines()
        .filter_map(|line| text.line(line))
        .map(|line| line.as_str().chars().count())
        .max()
        .unwrap_or(0);
    let rows = usize::try_from(span.count().value()).unwrap_or(0);
    let mut grid = Grid::new(Count::new(widest) + GUTTER, Count::new(rows));
    for (row, line) in span.lines().enumerate() {
        let row = Count::new(row);
        let gutter = format!("{:5} ", line.number());
        for (column, glyph) in gutter.chars().enumerate() {
            grid.set(
                Count::new(column),
                row,
                Cell {
                    glyph: Glyph::new(glyph),
                    color: WEAK,
                },
            );
        }
        let Some(source) = text.line(line) else {
            continue;
        };
        let spans = file.highlights_on(line);
        let mut at = 0;
        for (column, (byte, glyph)) in source.as_str().char_indices().enumerate() {
            let byte = u32::try_from(byte).unwrap_or(0);
            while spans
                .get(at)
                .is_some_and(|highlight| highlight.end.value() <= byte)
            {
                at += 1;
            }
            let color = match spans.get(at) {
                Some(highlight) if highlight.start.value() <= byte => {
                    theme::highlight(highlight.class)
                }
                _ => TEXT,
            };
            grid.set(
                Count::new(column) + GUTTER,
                row,
                Cell {
                    glyph: Glyph::new(glyph),
                    color,
                },
            );
        }
    }
    grid
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GridKey {
    file: FileId,
    hash: TextHash,
    start: Line,
    end: Line,
}

#[derive(Default)]
pub(crate) struct Grids(BTreeMap<GridKey, Rc<Grid>>);

impl Grids {
    const MOST: usize = 512;

    pub(crate) fn get(&mut self, id: FileId, file: &SourceFile, span: Span) -> Rc<Grid> {
        let key = GridKey {
            file: id,
            hash: file.hash(),
            start: span.start(),
            end: span.end(),
        };
        if let Some(grid) = self.0.get(&key) {
            return Rc::clone(grid);
        }
        if self.0.len() > Self::MOST {
            self.0.clear();
        }
        let grid = Rc::new(build(file, span));
        self.0.insert(key, Rc::clone(&grid));
        grid
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}
