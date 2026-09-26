use super::*;

fn span(start: u32, end: u32) -> Span {
    Span::new(Line::new(start), Line::new(end)).unwrap()
}

#[test]
fn hashes_match_the_legacy_fnv1a() {
    assert_eq!(TextHash::of(b"").value(), 0xcbf2_9ce4_8422_2325);
    assert_eq!(TextHash::of(b"a").value(), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(TextHash::of(b"foobar").value(), 0x8594_4171_f739_67e8);
    assert_eq!(TextHash::new(0xab).to_string(), "00000000000000ab");
}

#[test]
fn a_slice_hash_ends_every_line_with_a_newline() {
    let text = FileText::from("fn a() {\n  1\n}\nfn b() {\n  2\n}");
    assert_eq!(
        text.hash(span(0, 2)).unwrap().value(),
        0x6832_67bb_9c6e_e5ea
    );
    assert_eq!(
        text.hash(span(4, 4)),
        Some(TextHash::of(b"  2\n")),
        "one line hashes with its newline"
    );
    assert_eq!(text.hash(span(5, 6)), None);
    assert_eq!(text.count(), LineCount::new(6));
    assert_eq!(text.span(), Some(span(0, 5)));
}

#[test]
fn tabs_expand_to_four_spaces_before_hashing() {
    let text = FileText::from("\tx\r\ny\n");
    assert_eq!(text.line(Line::new(0)).unwrap().as_str(), "    x");
    assert_eq!(text.line(Line::new(1)).unwrap().as_str(), "y");
    assert_eq!(text.whole_hash(), TextHash::of(b"    x\r\ny\n"));
    assert_eq!(FileText::default().count(), LineCount::new(0));
    assert_eq!(FileText::default().span(), None);
}

#[test]
fn spans_refuse_an_end_before_the_start() {
    assert!(Span::new(Line::new(3), Line::new(2)).is_none());
    let one = span(2, 4);
    assert!(one.contains(Line::new(4)));
    assert!(!one.contains(Line::new(5)));
    assert!(one.overlaps(span(4, 9)));
    assert!(!one.overlaps(span(5, 9)));
    assert!(one.encloses(span(3, 4)));
    assert_eq!(one.count(), LineCount::new(3));
    assert_eq!(one.lines().map(Line::value).collect::<Vec<_>>(), [2, 3, 4]);
}

#[test]
fn lines_shift_by_offsets() {
    let line = Line::new(5);
    assert_eq!(line.shifted(LineOffset::new(-5)), Some(Line::new(0)));
    assert_eq!(line.shifted(LineOffset::new(-6)), None);
    assert_eq!(Line::new(7).offset_from(line), Some(LineOffset::new(2)));
    assert_eq!(line.number(), 6);
}

#[test]
fn relative_paths_use_forward_slashes() {
    let path = RelativePath::new("src\\index\\mod.rs");
    assert_eq!(path.as_str(), "src/index/mod.rs");
    assert_eq!(path.stem(), "mod");
    assert_eq!(path.directory(), Some("index"));
    assert_eq!(path.extension(), Some("rs"));
    let top = RelativePath::new("lib.tar.gz");
    assert_eq!(top.stem(), "lib");
    assert_eq!(top.directory(), None);
    assert_eq!(top.extension(), Some("gz"));
    assert_eq!(RelativePath::new("Makefile").extension(), None);
}
