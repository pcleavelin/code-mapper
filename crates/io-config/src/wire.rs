use std::iter::Peekable;
use std::vec::IntoIter;

pub(crate) const HEADER: &str = "codemap layout 1";
const RIGHT: &str = "right";
const DOWN: &str = "down";
const PANEL: &str = "panel";
const SHOWN: char = '*';
const INDENT: &str = "  ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WireDirection {
    Right,
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
    let direction = if head == RIGHT {
        WireDirection::Right
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
                WireDirection::Right => RIGHT,
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

const SETTINGS_HEADER: &str = "codemap settings 1";
const THEME: &str = "theme";
const FONT: &str = "font";
const SIZE: &str = "size";
const GRAPH: &str = "graph";
const DARK: &str = "dark";
const LIGHT: &str = "light";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WireTheme {
    Dark,
    Light,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WireSetting {
    Theme(WireTheme),
    Font(String),
    Size(u16),
    Graph(WireDirection),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WireSettings {
    pub(crate) entries: Vec<WireSetting>,
}

fn setting(line: &str) -> Option<WireSetting> {
    let line = line.trim();
    let (key, value) = line.split_once(' ').unwrap_or((line, ""));
    let value = value.trim();
    if key == THEME {
        if value == DARK {
            return Some(WireSetting::Theme(WireTheme::Dark));
        }
        return (value == LIGHT).then_some(WireSetting::Theme(WireTheme::Light));
    }
    if key == FONT {
        return Some(WireSetting::Font(value.to_owned()));
    }
    if key == SIZE {
        return value.parse().ok().map(WireSetting::Size);
    }
    if key == GRAPH {
        if value == RIGHT {
            return Some(WireSetting::Graph(WireDirection::Right));
        }
        return (value == DOWN).then_some(WireSetting::Graph(WireDirection::Down));
    }
    None
}

pub(crate) fn parse_settings(text: &str) -> Option<WireSettings> {
    let mut lines = text.lines();
    if lines.next()? != SETTINGS_HEADER {
        return None;
    }
    Some(WireSettings {
        entries: lines.filter_map(setting).collect(),
    })
}

pub(crate) fn print_settings(settings: &WireSettings) -> String {
    let mut out = format!("{SETTINGS_HEADER}\n");
    for entry in &settings.entries {
        let line = match entry {
            WireSetting::Theme(WireTheme::Dark) => format!("{THEME} {DARK}"),
            WireSetting::Theme(WireTheme::Light) => format!("{THEME} {LIGHT}"),
            WireSetting::Font(family) => format!("{FONT} {family}"),
            WireSetting::Size(size) => format!("{SIZE} {size}"),
            WireSetting::Graph(WireDirection::Right) => format!("{GRAPH} {RIGHT}"),
            WireSetting::Graph(WireDirection::Down) => format!("{GRAPH} {DOWN}"),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}
