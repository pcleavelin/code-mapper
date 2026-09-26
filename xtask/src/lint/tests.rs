use crate::lint::{Rule, Workspace, lint};
use crate::source::{SourceFile, SourceText};
use crate::text::{Content, RepoPath};
use crate::vocabulary::Vocabulary;

const WORDS: &str = "
id
key
width
gap
ui
line
number
get
new
count
value
span
start
end
path
text
make
read
write
store
shape
wire
file
test
record
rows
first
second
item
self
main
old
here
name
point
x
y
";

fn rules_hit(files: &[(&str, &str)]) -> Vec<Rule> {
    let parsed: Vec<SourceFile> = files
        .iter()
        .map(|(path, text)| SourceFile::parse(RepoPath::new(path), SourceText::new(*text)).unwrap())
        .collect();
    let workspace = Workspace::of(&parsed, Vocabulary::parse(&Content::new(WORDS)).unwrap());
    let mut rules: Vec<Rule> = lint(&workspace, &parsed)
        .into_iter()
        .map(|finding| finding.rule)
        .collect();
    rules.dedup();
    rules
}

fn strict(text: &str) -> Vec<Rule> {
    rules_hit(&[("crates/domain/src/line.rs", text)])
}

#[test]
fn comments_are_found_in_every_form() {
    assert_eq!(strict("// line\nfn main() {}"), [Rule::Comment]);
    assert_eq!(strict("/// line\nfn main() {}"), [Rule::Comment]);
    assert_eq!(strict("//! line\nfn main() {}"), [Rule::Comment]);
    assert_eq!(strict("fn main() { /* line */ }"), [Rule::Comment]);
    assert_eq!(strict("fn main() { let text = \"// not a comment\"; }"), []);
}

#[test]
fn primitives_in_signatures() {
    assert_eq!(strict("fn get(line: u32) {}"), [Rule::Primitive]);
    assert_eq!(strict("fn get() -> usize { 0 }"), [Rule::Primitive]);
    assert_eq!(strict("fn get(text: &str) {}"), [Rule::Primitive]);
    assert_eq!(strict("fn get(text: String) {}"), [Rule::Primitive]);
    assert_eq!(strict("fn get(first: Vec<u8>) {}"), [Rule::Primitive]);
    assert_eq!(strict("fn get(point: (Line, Line)) {}"), [Rule::Primitive]);
    assert_eq!(strict("fn get(value: bool) {}"), [Rule::Primitive]);
    assert_eq!(strict("struct Span { start: usize }"), [Rule::Primitive]);
    assert_eq!(strict("enum Shape { Line(u32) }"), [Rule::Primitive]);
    assert_eq!(strict("const COUNT: usize = 3;"), [Rule::Primitive]);
    assert_eq!(strict("struct Span(Line, u32);"), [Rule::Primitive]);
}

#[test]
fn named_types_newtypes_tests_and_trait_impls_pass() {
    assert_eq!(strict("fn get(line: Line) -> Option<Span> { None }"), []);
    assert_eq!(strict("fn get() -> bool { true }"), []);
    assert_eq!(strict("fn get(test: impl Fn(&Line) -> bool) {}"), []);
    assert_eq!(
        strict("fn get(test: impl Fn(u32) -> bool) {}"),
        [Rule::Primitive]
    );
    assert_eq!(
        strict("struct Line(u32);\nimpl Line { const FIRST: u32 = 0; }"),
        []
    );
    assert_eq!(
        strict("struct Span;\nimpl Span { const FIRST: u32 = 0; }"),
        [Rule::Primitive]
    );
    assert_eq!(
        strict(
            "struct Line(u32);\nimpl Line { fn get(self) -> u32 { self.0 } fn new(value: u32) -> Self { Self(value) } }"
        ),
        []
    );
    assert_eq!(
        strict("struct Line;\nimpl From<Line> for Count { fn from(value: Line) -> u32 { 0 } }"),
        []
    );
    assert_eq!(
        strict("#[cfg(test)]\nmod test { fn make(count: usize) {} }"),
        []
    );
    assert_eq!(strict("fn main() { let rows = |count: usize| count; }"), []);
    assert_eq!(
        rules_hit(&[("tests/store.rs", "fn make(count: usize) {}")]),
        []
    );
}

#[test]
fn newtype_fields_are_private() {
    assert_eq!(strict("struct Line(pub u32);"), [Rule::NewtypeField]);
    assert_eq!(strict("struct Line(u32);"), []);
}

#[test]
fn indexing_and_slicing() {
    assert_eq!(
        strict("fn main() { let first = rows[0]; }"),
        [Rule::Indexing]
    );
    assert_eq!(
        strict("fn main() { let first = &text[1..]; }"),
        [Rule::Indexing]
    );
    assert_eq!(strict("fn main() { let first = rows.get(0); }"), []);
    assert_eq!(strict("#[test]\nfn test() { let first = rows[0]; }"), []);
}

#[test]
fn sentinels_for_absence() {
    assert_eq!(strict("fn main() { if name == \"\" {} }"), [Rule::Absence]);
    assert_eq!(strict("fn main() { if count != -1 {} }"), [Rule::Absence]);
    assert_eq!(
        strict("fn main() { let end = usize::MAX; }"),
        [Rule::Absence]
    );
    assert_eq!(strict("fn main() { if name.is_some() {} }"), []);
}

#[test]
fn string_literals_are_not_compared() {
    let found = |text: &str| strict(&format!("fn main() {{ {text} }}"));
    assert_eq!(
        found("match name { \"x\" => {} _ => {} }"),
        [Rule::Compared]
    );
    assert_eq!(
        found("match name { \"x\" | \"y\" => {} _ => {} }"),
        [Rule::Compared]
    );
    assert_eq!(found("if let Some(\"x\") = name {}"), [Rule::Compared]);
    assert_eq!(found("if name == \"x\" {}"), [Rule::Compared]);
    assert_eq!(found("if \"x\" != name {}"), [Rule::Compared]);
    assert_eq!(
        found("let end = name.starts_with(\"x\");"),
        [Rule::Compared]
    );
    assert_eq!(
        found("let end = name.split_once(r\"x\");"),
        [Rule::Compared]
    );
    assert_eq!(
        found("let end = matches!(name, \"x\" | \"y\");"),
        [Rule::Compared]
    );
    assert_eq!(found("if name == Key::X.name().as_str() {}"), []);
    assert_eq!(
        found("let end = name.starts_with(Key::X.name().as_str());"),
        []
    );
    assert_eq!(found("let end = format!(\"x {name}\");"), []);
    assert_eq!(found("let end = matches!(\"x\", _);"), []);
    assert_eq!(found("let end = Key::new(\"x\");"), []);
    assert_eq!(found("match key { Key::X => \"x\", Key::Y => \"y\" };"), []);
    assert_eq!(strict("#[test]\nfn test() { if name == \"x\" {} }"), []);
}

#[test]
fn vocabulary_words_and_synonyms() {
    let files = [("crates/domain/src/line.rs", "fn get_line_number() {}")];
    assert_eq!(rules_hit(&files), []);
    assert_eq!(strict("fn fetch_line() {}"), [Rule::Vocabulary]);
    assert_eq!(strict("fn get_lines(entries: Rows) {}"), [Rule::Vocabulary]);
    assert_eq!(strict("fn get_lines(numbers: Rows) {}"), []);
    let parsed = [SourceFile::parse(
        RepoPath::new("crates/domain/src/line.rs"),
        SourceText::new("fn get_idx() {}"),
    )
    .unwrap()];
    let workspace = Workspace::of(
        &parsed,
        Vocabulary::parse(&Content::new("get\nidx -> index\nindex")).unwrap(),
    );
    let findings = lint(&workspace, &parsed);
    assert!(
        findings
            .iter()
            .any(|finding| finding.detail.as_str().contains("is written `index`"))
    );
}

#[test]
fn wire_types_stay_in_their_crate() {
    let files = [
        (
            "crates/io-map/src/wire.rs",
            "pub struct Record { pub text: String }",
        ),
        (
            "crates/io-map/src/store.rs",
            "pub fn read() -> Record { todo() }",
        ),
    ];
    assert_eq!(rules_hit(&files), [Rule::WireLeak]);
    let private = [
        (
            "crates/io-map/src/wire.rs",
            "pub struct Record { pub text: String }",
        ),
        (
            "crates/io-map/src/store.rs",
            "fn read() -> Record { todo() }",
        ),
    ];
    assert_eq!(rules_hit(&private), []);
    let elsewhere = [
        (
            "crates/io-map/src/wire.rs",
            "pub struct Record { pub text: String }",
        ),
        (
            "crates/index/src/lib.rs",
            "pub struct Record;\npub fn read() -> Record { todo() }",
        ),
    ];
    assert_eq!(rules_hit(&elsewhere), []);
}

#[test]
fn the_domain_does_no_io() {
    assert_eq!(strict("use std::fs;"), [Rule::DomainIo]);
    assert_eq!(strict("fn main() { println!(\"x\"); }"), [Rule::DomainIo]);
    assert_eq!(
        rules_hit(&[("crates/io-map/src/store.rs", "use std::fs;")]),
        []
    );
}

#[test]
fn suppressions_are_expectations_with_reasons_on_the_list() {
    assert_eq!(
        strict("#[allow(dead_code)]\nfn main() {}"),
        [Rule::Suppression]
    );
    assert_eq!(
        strict("#[expect(dead_code)]\nfn main() {}"),
        [Rule::Suppression]
    );
    assert_eq!(
        strict("#[expect(dead_code, reason = \"x\")]\nfn main() {}"),
        [Rule::Suppression]
    );
    let listed = [(
        "xtask/src/process.rs",
        "#[expect(clippy::disallowed_methods, reason = \"x\")]\nfn main() {}",
    )];
    assert_eq!(rules_hit(&listed), []);
    let commas = [(
        "xtask/src/process.rs",
        "#[expect(clippy::disallowed_methods, reason = \"one, two, three\")]\nfn main() {}",
    )];
    assert_eq!(rules_hit(&commas), []);
}

#[test]
fn scenario_tests_come_from_their_table() {
    let by_hand = [("crates/codemap/tests/cli.rs", "#[test]\nfn read() {}")];
    assert_eq!(rules_hit(&by_hand), [Rule::TestRegistry]);
    let generated = [(
        "crates/codemap/tests/cli.rs",
        "macro_rules! tests { ($($name:ident),*) => { $( #[test] fn $name() {} )* } }",
    )];
    assert_eq!(rules_hit(&generated), []);
    assert_eq!(
        rules_hit(&[("crates/codemap/tests/parity.rs", "#[test]\nfn old() {}")]),
        []
    );
}

#[test]
fn the_gui_builds_through_its_helpers_theme_ids_and_keys() {
    let gui = |path: &str, text: &str| rules_hit(&[(path, text)]);
    assert_eq!(
        gui("crates/gui/src/panels.rs", "fn main() { ui.open(item); }"),
        [Rule::Widget]
    );
    assert_eq!(
        gui("crates/gui/src/widgets.rs", "fn main() { ui.open(item); }"),
        []
    );
    assert_eq!(
        gui(
            "crates/gui/src/panels.rs",
            "fn main() { let x = Px::new(4); }"
        ),
        [Rule::Theme]
    );
    assert_eq!(
        gui("crates/gui/src/theme.rs", "const GAP: Px = Px::new(4);"),
        []
    );
    assert_eq!(
        gui(
            "crates/gui/src/panels.rs",
            "fn main() { let x = Px::new(width); }"
        ),
        []
    );
    assert_eq!(
        gui(
            "crates/gui/src/panels.rs",
            "fn main() { let id = Id::new(\"save\"); }"
        ),
        [Rule::ElementId]
    );
    assert_eq!(
        gui(
            "crates/gui/src/ids.rs",
            "fn main() { let id = Id::new(\"save\"); }"
        ),
        []
    );
    assert_eq!(
        gui(
            "crates/gui/src/panels.rs",
            "fn main() { let key = Key::Up; }"
        ),
        [Rule::KeyBinding]
    );
    assert_eq!(
        gui("crates/gui/src/keys.rs", "fn main() { let key = Key::Up; }"),
        []
    );
    assert_eq!(
        gui(
            "crates/ui/src/tree.rs",
            "fn main() { ui.open(item); let x = Px::new(4); }"
        ),
        []
    );
}

#[test]
fn aliases_are_newtypes() {
    assert_eq!(strict("type Line = u32;"), [Rule::Alias]);
    assert_eq!(strict("type Span = Line;"), [Rule::Alias]);
    assert_eq!(
        strict(
            "struct Line;\nimpl Add for Line { type Output = Self; fn add(self, other: Self) -> Self { self } }"
        ),
        []
    );
}
