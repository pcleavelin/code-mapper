use std::iter::Peekable;
use std::vec::IntoIter;

pub(crate) const HEADER: &str = "codemap layout 1";
const ACROSS: &str = "across";
const DOWN: &str = "down";
const PANEL: &str = "panel";
const SHOWN: char = '*';
const INDENT: &str = "  ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WireDirection {
    Across,
    Down,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WireView {
    pub(crate) name: String,
    pub(crate) shown: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WireTree {
    Split {
        direction: WireDirection,
        share: u16,
        first: Box<WireTree>,
        second: Box<WireTree>,
    },
    Panel {
        views: Vec<WireView>,
    },
}

struct Row {
    depth: usize,
    words: Vec<String>,
}

fn row(line: &str) -> Option<Row> {
    let body = line.trim_start_matches(' ');
    let spaces = line.len() - body.len();
    spaces.is_multiple_of(INDENT.len()).then(|| Row {
        depth: spaces / INDENT.len(),
        words: body.split_whitespace().map(str::to_owned).collect(),
    })
}

pub(crate) fn parse(text: &str) -> Option<WireTree> {
    let mut lines = text.lines();
    if lines.next()? != HEADER {
        return None;
    }
    let mut rows = lines
        .filter(|line| !line.trim().is_empty())
        .map(row)
        .collect::<Option<Vec<Row>>>()?
        .into_iter()
        .peekable();
    let tree = tree(&mut rows, 0)?;
    rows.next().is_none().then_some(tree)
}

fn tree(rows: &mut Peekable<IntoIter<Row>>, depth: usize) -> Option<WireTree> {
    let row = rows.next().filter(|row| row.depth == depth)?;
    let mut words = row.words.into_iter();
    let head = words.next()?;
    if head == PANEL {
        let views = words
            .map(|word| match word.strip_suffix(SHOWN) {
                Some(name) => WireView {
                    name: name.to_owned(),
                    shown: true,
                },
                None => WireView {
                    name: word,
                    shown: false,
                },
            })
            .collect();
        return Some(WireTree::Panel { views });
    }
    let direction = if head == ACROSS {
        WireDirection::Across
    } else if head == DOWN {
        WireDirection::Down
    } else {
        return None;
    };
    let share = words.next()?.parse().ok()?;
    if words.next().is_some() {
        return None;
    }
    let first = tree(rows, depth + 1)?;
    let second = tree(rows, depth + 1)?;
    Some(WireTree::Split {
        direction,
        share,
        first: Box::new(first),
        second: Box::new(second),
    })
}

pub(crate) fn print(tree: &WireTree) -> String {
    let mut out = format!("{HEADER}\n");
    print_tree(tree, 0, &mut out);
    out
}

fn print_tree(tree: &WireTree, depth: usize, out: &mut String) {
    out.push_str(&INDENT.repeat(depth));
    match tree {
        WireTree::Panel { views } => {
            out.push_str(PANEL);
            for view in views {
                out.push(' ');
                out.push_str(&view.name);
                if view.shown {
                    out.push(SHOWN);
                }
            }
            out.push('\n');
        }
        WireTree::Split {
            direction,
            share,
            first,
            second,
        } => {
            let word = match direction {
                WireDirection::Across => ACROSS,
                WireDirection::Down => DOWN,
            };
            out.push_str(word);
            out.push(' ');
            out.push_str(&share.to_string());
            out.push('\n');
            print_tree(first, depth + 1, out);
            print_tree(second, depth + 1, out);
        }
    }
}
