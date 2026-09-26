use std::mem;

use crate::color::Color;
use crate::geometry::{Count, FontSize};
use crate::icon::Icon;
use crate::input::Glyph;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Label(String);

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn spelled(&self) -> String {
        self.0
            .chars()
            .map(|character| {
                Icon::of(Glyph::new(character)).map_or_else(
                    || String::from(character),
                    |icon| format!("[{}]", icon.name()),
                )
            })
            .collect()
    }

    pub fn columns(&self) -> usize {
        self.0
            .chars()
            .map(|character| Glyph::new(character).columns())
            .sum()
    }

    pub fn wrap(&self, columns: usize) -> Vec<Self> {
        let columns = columns.max(1);
        let mut out = Vec::new();
        for paragraph in self.0.split('\n') {
            let mut line = String::new();
            let mut length = 0;
            for word in paragraph.split(' ') {
                let word_length: usize = word
                    .chars()
                    .map(|character| Glyph::new(character).columns())
                    .sum();
                if length > 0 && length + 1 + word_length > columns {
                    out.push(Self(mem::take(&mut line)));
                    length = 0;
                }
                if length > 0 {
                    line.push(' ');
                    length += 1;
                }
                if word_length > columns {
                    for character in word.chars() {
                        if length >= columns {
                            out.push(Self(mem::take(&mut line)));
                            length = 0;
                        }
                        line.push(character);
                        length += Glyph::new(character).columns();
                    }
                } else {
                    line.push_str(word);
                    length += word_length;
                }
            }
            out.push(Self(line));
        }
        out
    }
}

impl From<&str> for Label {
    fn from(text: &str) -> Self {
        Self(text.to_owned())
    }
}

impl From<String> for Label {
    fn from(text: String) -> Self {
        Self(text)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Run {
    pub text: Label,
    pub color: Color,
}

impl Run {
    pub fn new(text: impl Into<Label>, color: Color) -> Self {
        Self {
            text: text.into(),
            color,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wrap {
    None,
    Words,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Text {
    pub runs: Vec<Run>,
    pub size: FontSize,
    pub wrap: Wrap,
}

impl Text {
    pub(crate) fn columns(&self) -> Count {
        Count::new(self.runs.iter().map(|run| run.text.columns()).sum())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub glyph: Glyph,
    pub color: Color,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Grid {
    columns: Count,
    rows: Count,
    cells: Vec<Option<Cell>>,
}

impl Grid {
    pub fn new(columns: Count, rows: Count) -> Self {
        Self {
            columns,
            rows,
            cells: vec![None; columns.get() * rows.get()],
        }
    }

    pub const fn columns(&self) -> Count {
        self.columns
    }

    pub const fn rows(&self) -> Count {
        self.rows
    }

    pub fn set(&mut self, column: Count, row: Count, cell: Cell) {
        if column < self.columns
            && row < self.rows
            && let Some(slot) = self
                .cells
                .get_mut(row.get() * self.columns.get() + column.get())
        {
            *slot = Some(cell);
        }
    }

    pub fn row(&self, row: Count) -> impl Iterator<Item = Option<Cell>> + '_ {
        self.cells
            .iter()
            .skip(row.get() * self.columns.get())
            .take(self.columns.get())
            .copied()
    }
}
