use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use domain::{Language, Line, Location, RelativePath};
use serde_json::json;

use crate::answer::{Character, HoverLine, HoverText, Inline, RangeEnd, Reply};
use crate::convert::{self, Uri, relative};
use crate::session::{LspSession, WhyUnanswered, read_reply};
use crate::wire::{self, LocationShape};

#[test]
fn uris_round_trip() {
    let root = if cfg!(windows) {
        Path::new("C:\\Users\\me\\my repo")
    } else {
        Path::new("/home/Me/my repo")
    };
    let uri = Uri::of(&root.join("src").join("a.rs"));
    assert!(uri.as_str().ends_with("/my%20repo/src/a.rs"), "{uri:?}");
    let rel = |text: &str| -> Option<RelativePath> { relative(&Uri::new(text).path()?, root) };
    assert_eq!(rel(uri.as_str()), Some(RelativePath::new("src/a.rs")));
    assert_eq!(
        rel(&uri.as_str().to_ascii_lowercase()).is_some(),
        cfg!(windows)
    );
    assert_eq!(rel("file:///elsewhere/x.rs"), None);
}

#[cfg(unix)]
#[test]
fn a_definition_through_the_real_path_of_a_symlinked_root_stays_inside() {
    use std::env;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = env::temp_dir().join(format!("codemap-rel-{nanos}"));
    let link = env::temp_dir().join(format!("codemap-rel-link-{nanos}"));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(root.join("src")).unwrap();
    let file = root.join("src").join("a.rs");
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&file)
        .unwrap()
        .write_all(b"fn a() {}\n")
        .unwrap();
    symlink(&root, &link).unwrap();
    let real = file.canonicalize().unwrap();
    assert_eq!(relative(&real, &link), Some(RelativePath::new("src/a.rs")));
}

#[test]
fn a_root_ending_in_a_separator_still_holds_its_files() {
    let root = if cfg!(windows) {
        Path::new("C:\\Users\\me\\repo\\")
    } else {
        Path::new("/home/me/repo/")
    };
    let file = Uri::of(&root.join("src").join("a.rs"));
    assert_eq!(
        Uri::new(file.as_str())
            .path()
            .and_then(|path| relative(&path, root)),
        Some(RelativePath::new("src/a.rs"))
    );
}

#[test]
fn messages_are_framed_by_length() {
    let message = json!({"jsonrpc": "2.0", "id": 3, "result": [1, 2]});
    let mut framed = b"Content-Type: x\r\n\r\n".to_vec();
    framed.extend(wire::frame(&message));
    framed.extend(wire::frame(&json!({"method": "m"})));
    let mut reader = Cursor::new(framed);
    assert_eq!(wire::read_message(&mut reader), Some(message));
    assert_eq!(
        wire::read_message(&mut reader),
        Some(json!({"method": "m"}))
    );
    assert_eq!(wire::read_message(&mut reader), None);
}

#[test]
fn answers_are_told_from_server_requests() {
    let answer = json!({"id": 4, "result": {"a": 1}});
    assert_eq!(wire::answer_id(&answer), Some(4));
    assert_eq!(wire::outcome(answer), Ok(json!({"a": 1})));
    assert_eq!(
        wire::outcome(json!({"id": 5, "error": {"message": "no"}})),
        Err("no".to_owned())
    );
    assert_eq!(wire::outcome(json!({"id": 6})), Ok(serde_json::Value::Null));
    assert_eq!(
        wire::answer_id(&json!({"id": 4, "method": "workspace/configuration"})),
        None
    );
}

fn spelled(hover: Option<HoverText>) -> Vec<String> {
    let inlines = |inlines: &[Inline]| -> String {
        inlines
            .iter()
            .map(|inline| match inline {
                Inline::Prose(words) => words.as_str().to_owned(),
                Inline::Code(words) => format!("{{{}}}", words.as_str()),
            })
            .collect()
    };
    hover
        .map(|hover| {
            hover
                .lines()
                .iter()
                .map(|line| match line {
                    HoverLine::Prose(found) => format!("text {}", inlines(found)),
                    HoverLine::Heading(found) => format!("head {}", inlines(found)),
                    HoverLine::Fenced(text) => format!("code {}", text.as_str()),
                    HoverLine::Rule => "rule".to_owned(),
                    HoverLine::Blank => String::new(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn hover_of(markdown: &str) -> Option<HoverText> {
    let answer = json!({"contents": {"kind": "markdown", "value": markdown}});
    HoverText::from_parts(&wire::hover_parts(&answer))
}

#[test]
fn hover_drops_fences_and_leading_blank_lines() {
    assert_eq!(
        spelled(hover_of("```rust\nfn a()\n```\n\n\n---\ndoc")),
        ["code fn a()", "", "rule", "text doc"]
    );
    let list = json!({"contents": ["plain", {"value": "marked"}]});
    assert_eq!(
        spelled(HoverText::from_parts(&wire::hover_parts(&list))),
        ["text plain", "text marked"]
    );
    assert_eq!(
        HoverText::from_parts(&wire::hover_parts(&serde_json::Value::Null)),
        None
    );
}

#[test]
fn hover_markdown_is_read_into_code_links_and_plain_words() {
    let markdown = "```rust\ncore::option\n```\n\n```rust\npub enum Option<T>\n```\n\n---\n\nThe `Option` type. See [the module level documentation](https://doc.rust-lang.org/stable/core/option/index.html) for more.\n\n# Examples\n\n**Bold** and _em_ keep snake_case, a * b and [`Vec`][vec].\n\n```\nlet x = 1;\n```\n\n``a ` b`` \\*not em\\* and [no link] here";
    assert_eq!(
        spelled(hover_of(markdown)),
        [
            "code core::option",
            "",
            "code pub enum Option<T>",
            "",
            "rule",
            "",
            "text The {Option} type. See the module level documentation for more.",
            "",
            "head Examples",
            "",
            "text Bold and em keep snake_case, a * b and {Vec}.",
            "",
            "code let x = 1;",
            "",
            "text {a ` b} *not em* and [no link] here",
        ]
    );
}

#[test]
fn locations_are_relative_sorted_and_unique() {
    let root = Path::new("/r");
    let answer = json!([
        {"to": {"uri": "file:///r/b.rs", "selectionRange": {"start": {"line": 2}}}},
        {"to": {"uri": "file:///r/a.rs", "selectionRange": {"start": {"line": 9}}}},
        {"to": {"uri": "file:///r/b.rs", "selectionRange": {"start": {"line": 2}}}},
        {"to": {"uri": "file:///elsewhere/c.rs", "selectionRange": {"start": {"line": 1}}}},
        {"to": {"uri": "file:///r/d.rs"}}
    ]);
    let found = convert::locations(wire::locations(&answer, &LocationShape::Outgoing), root);
    let expected = [("a.rs", 9), ("b.rs", 2)].map(|(file, line)| Location {
        file: RelativePath::new(file),
        line: Line::new(line),
    });
    if cfg!(windows) {
        return;
    }
    assert_eq!(found, expected);
}

#[test]
fn definitions_read_links_and_locations() {
    let root = Path::new("/r");
    let link = json!([{"targetUri": "file:///r/x.rs", "targetSelectionRange": {"start": {"line": 3, "character": 7}}, "uri": "file:///r/no.rs"}]);
    let found = convert::definition(&wire::definition(&link).unwrap(), root).unwrap();
    if !cfg!(windows) {
        assert_eq!(found.file, Some(RelativePath::new("x.rs")));
    }
    assert_eq!(
        (found.line, found.character),
        (Line::new(3), Character::new(7))
    );
    let plain = json!({"uri": "file:///r/y.rs", "range": {"start": {"line": 1, "character": 0}}});
    assert_eq!(
        convert::definition(&wire::definition(&plain).unwrap(), root)
            .unwrap()
            .line,
        Line::new(1)
    );
    assert!(wire::definition(&json!([])).is_none());
}

#[test]
fn outlines_keep_the_server_shape() {
    let answer = json!([{
        "name": "impl A", "kind": 19,
        "range": {"start": {"line": 1, "character": 0}, "end": {"line": 5, "character": 0}},
        "selectionRange": {"start": {"line": 1, "character": 5}},
        "children": [{"name": "go", "kind": 6, "range": {"end": {"line": 3, "character": 5}}, "selectionRange": {"start": {"line": 2, "character": 7}}}]
    }, {"name": "", "range": {"end": {"line": 8}}}]);
    let outlines: Vec<_> = wire::symbols(&answer)
        .into_iter()
        .map(convert::outline)
        .collect();
    assert_eq!(outlines.len(), 2);
    let first = &outlines[0];
    assert_eq!(first.name.as_str(), "impl A");
    assert_eq!(first.kind.value(), 19);
    assert_eq!(first.end, Line::new(5));
    assert_eq!(first.range_end, RangeEnd::LineStart);
    assert_eq!(first.selection.character, Character::new(5));
    assert_eq!(first.children[0].range_end, RangeEnd::Inside);
    assert_eq!(first.children[0].selection.line, Line::new(2));
    assert_eq!(outlines[1].range_end, RangeEnd::Inside);
    assert_eq!(outlines[1].kind.value(), 0);
}

#[test]
#[ignore = "needs rust-analyzer on PATH"]
fn rust_analyzer_answers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut session = LspSession::start(Language::Rust, &root).expect("start");
    session.wait_ready(Duration::from_secs(120));
    let outlines = session.outlines(&[RelativePath::new("src/map.rs")]);
    let Some(Reply::Given(first)) = outlines.first() else {
        panic!("no answer for src/map.rs");
    };
    let names: Vec<&str> = first.iter().map(|outline| outline.name.as_str()).collect();
    assert!(names.contains(&"Map"), "{names:?}");
    session.shutdown();
}

#[test]
fn a_refused_or_lost_request_is_unanswered_and_a_null_answer_is_an_empty_one() {
    let symbols = |value: &serde_json::Value| wire::symbols(value).len();
    assert_eq!(
        read_reply(Err(WhyUnanswered::Refused), symbols),
        Reply::Unanswered
    );
    assert_eq!(
        read_reply(Err(WhyUnanswered::Gone), symbols),
        Reply::Unanswered
    );
    assert_eq!(
        read_reply(Ok(serde_json::Value::Null), symbols),
        Reply::Given(0)
    );
}

#[test]
fn hover_markdown_keeps_what_only_looks_like_markup() {
    let markdown = "\n\nfoo_ and snake_case_ and *args, ![logo](x.png) and __bold__\n```\n~~~ stays code\n```\n[vec]: https://doc.rust-lang.org/vec\n\n```one``` line\n";
    assert_eq!(
        spelled(hover_of(markdown)),
        [
            "text foo_ and snake_case_ and *args, logo and bold",
            "code ~~~ stays code",
            "",
            "text {one} line",
        ]
    );
}
