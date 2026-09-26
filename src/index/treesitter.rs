use super::{
    Backend, Call, File, HL_COMMENT, HL_CONSTANT, HL_FUNCTION, HL_KEYWORD, HL_PLAIN, HL_PROPERTY,
    HL_STRING, HL_TYPE, Qual, Span, Symbol,
};
use crate::codec::fnv1a;
use std::collections::HashMap;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, StreamingIterator};

const CONTAINERS: [&str; 4] = ["impl", "mod", "trait", "class"];

fn language_for(ext: &str) -> Option<(Language, &'static str)> {
    Some(match ext {
        "rs" => (
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
        ),
        "odin" => (
            tree_sitter_odin::LANGUAGE.into(),
            tree_sitter_odin::HIGHLIGHTS_QUERY,
        ),
        "c" | "h" => (
            tree_sitter_c::LANGUAGE.into(),
            tree_sitter_c::HIGHLIGHT_QUERY,
        ),
        "py" => (
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::HIGHLIGHTS_QUERY,
        ),
        "js" | "mjs" | "cjs" => (
            tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::HIGHLIGHT_QUERY,
        ),
        "ts" => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        ),
        "tsx" => (
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        ),
        _ => return None,
    })
}

#[derive(Default)]
pub struct Parsers {
    parser: Parser,
    queries: HashMap<String, Option<(Language, Query)>>,
}

impl Parsers {
    fn query_for<'a>(
        queries: &'a mut HashMap<String, Option<(Language, Query)>>,
        ext: &str,
    ) -> Option<&'a (Language, Query)> {
        queries
            .entry(ext.to_owned())
            .or_insert_with(|| {
                let (lang, q) = language_for(ext)?;
                let query = Query::new(&lang, q).ok()?;
                Some((lang, query))
            })
            .as_ref()
    }
}

pub fn parse_file(parsers: &mut Parsers, path: String, text: &str, ext: &str) -> File {
    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let mut symbols = Vec::new();
    let mut hl = vec![Vec::new(); lines.len()];
    let mut imports = HashMap::new();
    let Parsers { parser, queries } = parsers;
    if let Some((lang, query)) = Parsers::query_for(queries, ext)
        && parser.set_language(lang).is_ok()
        && let Some(tree) = parser.parse(text, None)
    {
        collect(
            tree.root_node(),
            text.as_bytes(),
            &lines,
            0,
            None,
            &mut symbols,
            &mut imports,
        );
        hl = highlight(tree.root_node(), query, text, &lines);
    }
    File {
        path,
        lines,
        hl,
        symbols,
        imports,
        mtime: None,
        hash: fnv1a(text.bytes()),
        backend: Backend::TreeSitter,
        pending: false,
    }
}

fn class_of(capture: &str) -> u8 {
    match capture.split('.').next().unwrap_or("") {
        "keyword" | "include" | "repeat" | "conditional" | "storageclass" | "storage"
        | "exception" => HL_KEYWORD,
        "string" | "character" | "escape" => HL_STRING,
        "comment" => HL_COMMENT,
        "function" | "method" | "constructor" | "macro" => HL_FUNCTION,
        "type" | "namespace" | "module" => HL_TYPE,
        "number" | "constant" | "boolean" | "float" => HL_CONSTANT,
        "property" | "field" | "attribute" | "label" | "tag" => HL_PROPERTY,
        "variable" if capture.contains("builtin") => HL_CONSTANT,
        _ => HL_PLAIN,
    }
}

fn highlight(root: Node, query: &Query, text: &str, lines: &[String]) -> Vec<Vec<Span>> {
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let names = query.capture_names();
    let mut hl: Vec<Vec<Span>> = vec![Vec::new(); lines.len()];
    let mut cursor = QueryCursor::new();
    let mut caps = cursor.captures(query, root, text.as_bytes());
    while let Some((m, ci)) = caps.next() {
        let cap = m.captures[*ci];
        let class = class_of(names[cap.index as usize]);
        if class == HL_PLAIN {
            continue;
        }
        let (s, e) = (cap.node.start_byte(), cap.node.end_byte());
        let first = line_starts.partition_point(|&ls| ls <= s).saturating_sub(1);
        for li in first..lines.len() {
            let ls = line_starts[li];
            if ls >= e {
                break;
            }
            let le = ls + lines[li].len();
            let (a, b) = (s.max(ls), e.min(le));
            if a < b {
                hl[li].push(((a - ls) as u32, (b - ls) as u32, class));
            }
        }
    }
    for spans in &mut hl {
        spans.sort_by_key(|s| s.0);
        let mut end = 0;
        spans.retain(|s| {
            let keep = s.0 >= end;
            if keep {
                end = s.1;
            }
            keep
        });
    }
    hl
}

pub(super) fn bare_type(t: &str) -> String {
    let t = t
        .split('<')
        .next()
        .unwrap_or(t)
        .trim_start_matches(['&', '*', ' '])
        .trim_start_matches("mut ")
        .trim();
    t.rsplit("::").next().unwrap_or(t).trim().to_owned()
}

fn record_import(text: &str, imports: &mut HashMap<String, String>) {
    let text = text.trim().trim_end_matches(';');
    if let Some(q) = text.find(['"', '\'']) {
        let quoted: String = text[q + 1..]
            .chars()
            .take_while(|c| *c != '"' && *c != '\'')
            .collect();
        let module = quoted
            .rsplit(['/', ':', '\\'])
            .next()
            .unwrap_or(&quoted)
            .trim_end_matches(".js")
            .trim_end_matches(".ts")
            .to_owned();
        let head = &text[..q];
        let mut named = false;
        for tok in head
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|t| !t.is_empty())
        {
            if ["import", "from", "as", "type", "default"].contains(&tok) {
                continue;
            }
            imports.insert(tok.to_owned(), module.clone());
            named = true;
        }
        if !named {
            imports.insert(module.clone(), module);
        }
        return;
    }
    let body = text
        .trim_start_matches("pub ")
        .trim_start_matches("use ")
        .trim_start_matches("from ")
        .trim_start_matches("import ");
    let (path_part, items) = match body.split_once(" import ") {
        Some((p, i)) => (p.trim(), i.trim()),
        None => match body.find('{') {
            Some(b) => (
                body[..b].trim().trim_end_matches("::"),
                body[b + 1..].trim_end_matches('}'),
            ),
            None => match body.rsplit_once("::").or_else(|| body.rsplit_once('.')) {
                Some((p, last)) => (p, last),
                None => (body, body),
            },
        },
    };
    let module = path_part
        .rsplit(['.', ':'])
        .find(|s| !s.is_empty())
        .unwrap_or(path_part)
        .to_owned();
    for item in items.split(',') {
        let item = item.trim();
        if item.is_empty() || item == "*" {
            continue;
        }
        let (name, alias) = match item.split_once(" as ") {
            Some((n, a)) => (n.trim(), Some(a.trim())),
            None => (item, None),
        };
        let name = name.rsplit("::").next().unwrap_or(name).trim();
        if name.is_empty() || name == "self" {
            continue;
        }
        imports.insert(alias.unwrap_or(name).to_owned(), module.clone());
    }
}

fn impl_name(node: Node, src: &[u8]) -> String {
    let field = |f: &str| {
        node.child_by_field_name(f)
            .and_then(|n| n.utf8_text(src).ok())
            .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
    };
    match (field("trait"), field("type")) {
        (Some(t), Some(ty)) => format!("impl {t} for {ty}"),
        (None, Some(ty)) => format!("impl {ty}"),
        _ => String::new(),
    }
}

fn collect(
    parent: Node,
    src: &[u8],
    lines: &[String],
    depth: u8,
    owner: Option<&str>,
    out: &mut Vec<Symbol>,
    imports: &mut HashMap<String, String>,
) {
    let mut cursor = parent.walk();
    for node in parent.named_children(&mut cursor) {
        let kind = node.kind();
        if kind.contains("import") || kind == "use_declaration" {
            if let Ok(t) = node.utf8_text(src) {
                record_import(t, imports);
            }
            continue;
        }
        if kind.contains("comment") || kind.contains("package") || kind.contains("attribute") {
            continue;
        }
        if kind == "mod_item" && node.child_by_field_name("body").is_none() {
            continue;
        }

        let start = node.start_position().row;
        let mut end = node.end_position().row;
        if node.end_position().column == 0 && end > start {
            end -= 1;
        }

        let mut name_row = start;
        while name_row < end
            && lines
                .get(name_row)
                .is_some_and(|l| l.trim_start().starts_with('@'))
        {
            name_row += 1;
        }
        let first = lines.get(name_row).map(|l| l.trim()).unwrap_or("");
        let (name, guessed) = match node.child_by_field_name("name") {
            Some(n) => (n.utf8_text(src).unwrap_or("").to_owned(), false),
            None if kind == "impl_item" => (impl_name(node, src), false),
            None => (
                first
                    .split('{')
                    .next()
                    .unwrap_or(first)
                    .split("::")
                    .next()
                    .unwrap_or(first)
                    .trim()
                    .to_owned(),
                true,
            ),
        };
        if name.is_empty() || (guessed && name.len() > 80) {
            continue;
        }

        let container = depth == 0 && CONTAINERS.iter().any(|k| kind.contains(k));
        let own_type: Option<String> = if container {
            node.child_by_field_name("type")
                .and_then(|t| t.utf8_text(src).ok())
                .map(bare_type)
                .or_else(|| {
                    if kind.contains("class") {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
        } else {
            None
        };
        let mut calls = Vec::new();
        if !container {
            find_calls(node, src, &mut calls);
            calls.sort_by(|a, b| {
                a.name
                    .cmp(&b.name)
                    .then_with(|| format!("{:?}", a.qual).cmp(&format!("{:?}", b.qual)))
            });
            calls.dedup();
        }

        out.push(Symbol {
            name,
            kind: kind.to_owned(),
            start,
            end,
            depth,
            owner: if container {
                own_type.clone()
            } else {
                owner.map(str::to_owned)
            },
            calls,
            targets: Vec::new(),
            refs: Vec::new(),
            callees: Vec::new(),
            callers: Vec::new(),
        });

        if container && let Some(body) = node.child_by_field_name("body") {
            collect(body, src, lines, 1, own_type.as_deref(), out, imports);
        }
    }
}

fn find_calls(node: Node, src: &[u8], out: &mut Vec<Call>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind().contains("call") {
            let callee = child
                .child_by_field_name("function")
                .or_else(|| child.named_child(0));
            if let Some(text) = callee.and_then(|c| c.utf8_text(src).ok())
                && let Some(mut call) = parse_callee(text)
            {
                if call.qual == Qual::None {
                    let pk = node.kind();
                    if ["member", "selector", "scoped"]
                        .iter()
                        .any(|k| pk.contains(k))
                        && let Some(first) = node.named_child(0).filter(|f| f.id() != child.id())
                        && let Ok(t) = first.utf8_text(src)
                        && let Some(q) = t.rsplit(['.', ':']).next().filter(|q| !q.is_empty())
                    {
                        call.qual = qual_of(q);
                    }
                }
                out.push(call);
            }
        }
        find_calls(child, src, out);
    }
}

fn parse_callee(text: &str) -> Option<Call> {
    let mut clean = String::with_capacity(text.len());
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            _ if depth == 0 => clean.push(c),
            _ => {}
        }
    }
    let clean = clean.trim().trim_end_matches('!').trim();
    let segs: Vec<&str> = clean
        .split(['.', ':'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let name = *segs.last()?;
    if name.len() > 64
        || !name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
    {
        return None;
    }
    let qual = match segs.len() {
        0 | 1 => Qual::None,
        n => qual_of(segs[n - 2]),
    };
    Some(Call {
        name: name.to_owned(),
        qual,
    })
}

fn qual_of(q: &str) -> Qual {
    match q {
        "self" | "Self" | "this" | "super" => Qual::SelfRef,
        q => Qual::Some(bare_type(q)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impl_names_match_the_server() {
        let src = "pub struct S<R, RA>;\nimpl<R, RA> S<R, RA>\nwhere\n    R: Repo,\n{\n    fn new() {}\n}\nimpl<R, RA> Service<Form<Long>, (A, B, C)> for S<R, RA> {\n    fn go() {}\n}\n";
        let f = parse_file(&mut Parsers::default(), "x.rs".into(), src, "rs");
        let names: Vec<&str> = f.symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "S",
                "impl S<R, RA>",
                "new",
                "impl Service<Form<Long>, (A, B, C)> for S<R, RA>",
                "go"
            ]
        );
    }

    #[test]
    fn parses_callees() {
        assert_eq!(
            parse_callee("foo"),
            Some(Call {
                name: "foo".into(),
                qual: Qual::None
            })
        );
        assert_eq!(
            parse_callee("self.graph.focus"),
            Some(Call {
                name: "focus".into(),
                qual: Qual::Some("graph".into())
            })
        );
        assert_eq!(
            parse_callee("Self::new"),
            Some(Call {
                name: "new".into(),
                qual: Qual::SelfRef
            })
        );
        assert_eq!(
            parse_callee("Vec::<u8>::with_capacity"),
            Some(Call {
                name: "with_capacity".into(),
                qual: Qual::Some("Vec".into())
            })
        );
        assert_eq!(
            parse_callee("println!"),
            Some(Call {
                name: "println".into(),
                qual: Qual::None
            })
        );
        assert_eq!(
            parse_callee("pkg.proc"),
            Some(Call {
                name: "proc".into(),
                qual: Qual::Some("pkg".into())
            })
        );
        assert_eq!(parse_callee("(a)"), None);
    }

    #[test]
    fn records_imports() {
        let mut m = HashMap::new();
        record_import("use crate::index::{build, Index as Idx};", &mut m);
        record_import("use std::collections::HashMap;", &mut m);
        record_import("import \"../util\"", &mut m);
        record_import("import fmt \"core:fmt\"", &mut m);
        record_import("from index import build, link", &mut m);
        record_import("import { thing } from './mod.js'", &mut m);
        assert_eq!(m["build"], "index");
        assert_eq!(m["Idx"], "index");
        assert_eq!(m["HashMap"], "collections");
        assert_eq!(m["util"], "util");
        assert_eq!(m["fmt"], "fmt");
        assert_eq!(m["link"], "index");
        assert_eq!(m["thing"], "mod");
    }

    #[test]
    fn highlights_keywords_and_strings() {
        let src = "fn a() { let s = \"hi\"; } // c\n";
        let mut p = Parsers::default();
        let f = parse_file(&mut p, "x.rs".into(), src, "rs");
        let classes: Vec<u8> = f.hl[0].iter().map(|s| s.2).collect();
        assert!(classes.contains(&HL_KEYWORD), "{classes:?}");
        assert!(classes.contains(&HL_STRING), "{classes:?}");
        assert!(classes.contains(&HL_COMMENT), "{classes:?}");
        assert!(
            f.hl[0].windows(2).all(|w| w[0].1 <= w[1].0),
            "overlap: {:?}",
            f.hl[0]
        );
    }
}
