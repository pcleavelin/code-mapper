use std::path::Path as FsPath;

use super::*;
use crate::id::{Key, Position};
use crate::index::{Backend, Depth, Imports, Root, SourceFile, Symbol, SymbolKind, SymbolName};
use crate::map::{Freshness, Map, StepId, TourKind};
use crate::text::{FileText, Line};

fn span(start: u32, end: u32) -> Span {
    Span::new(Line::new(start), Line::new(end)).unwrap()
}

fn symbol(text: &str, start: u32, end: u32) -> Symbol {
    Symbol::new(
        SymbolName::new(text),
        SymbolKind::new("fn"),
        span(start, end),
        Depth::new(0),
        None,
        Vec::new(),
    )
}

fn index_of(lines: &[&str], symbols: Vec<Symbol>) -> Index {
    let text = FileText::from(lines.join("\n").as_str());
    let highlights = vec![Vec::new(); lines.len()];
    let hash = text.whole_hash();
    let mut index = Index::new(Root::new(FsPath::new(".")));
    index.push(SourceFile::new(
        RelativePath::new("a.rs"),
        text,
        highlights,
        symbols,
        Imports::new(),
        hash,
        Backend::TreeSitter,
    ));
    index
}

fn one_file() -> Index {
    index_of(
        &["fn a() {", "  1", "}", "fn b() {", "  2", "}"],
        vec![symbol("a", 0, 2), symbol("b", 3, 5)],
    )
}

fn first_file() -> FileId {
    FileId::at(Position::new(0))
}

fn tour(name: &str) -> CommentTarget {
    CommentTarget::Tour(TourName::new(name).unwrap())
}

fn text(words: &str) -> CommentText {
    CommentText::new(words).unwrap()
}

fn reply(words: &str) -> Reply {
    Reply {
        text: ReplyText::new(words).unwrap(),
        author: Author::Agent,
    }
}

#[test]
fn a_reply_answers_an_open_comment() {
    let index = one_file();
    let mut comments = Comments::default();
    let id = comments.add(&index, tour("t"), Author::Human, text("why?"));
    assert!(comments.get(&id).unwrap().is_open());
    comments.reply(&id, reply("because")).unwrap();
    let answered = comments.get(&id).unwrap();
    assert!(!answered.is_open());
    assert_eq!(answered.reply(), Some(&reply("because")));
    assert_eq!(comments.open().count(), 0);
}

#[test]
fn a_second_reply_replaces_the_first() {
    let index = one_file();
    let mut comments = Comments::default();
    let id = comments.add(&index, tour("t"), Author::Human, text("why?"));
    comments.reply(&id, reply("first")).unwrap();
    comments.reply(&id, reply("second")).unwrap();
    assert_eq!(comments.get(&id).unwrap().reply(), Some(&reply("second")));
}

#[test]
fn dismiss_removes_the_comment() {
    let index = one_file();
    let mut comments = Comments::default();
    let kept = comments.add(&index, tour("t"), Author::Human, text("keep"));
    let gone = comments.add(&index, tour("t"), Author::Human, text("drop"));
    let removed = comments.dismiss(&gone).unwrap();
    assert_eq!(removed.text(), &text("drop"));
    assert!(comments.get(&gone).is_none());
    assert!(comments.get(&kept).is_some());
}

#[test]
fn an_unknown_id_is_refused() {
    let mut comments = Comments::default();
    let unknown = CommentId::new("zzzzzz").unwrap();
    assert_eq!(
        comments.reply(&unknown, reply("x")),
        Err(CommentError::NoSuchComment(unknown.clone()))
    );
    assert_eq!(
        comments.dismiss(&unknown),
        Err(CommentError::NoSuchComment(unknown))
    );
}

#[test]
fn equal_comments_on_one_place_get_two_ids() {
    let index = one_file();
    let mut comments = Comments::default();
    let one = comments.add(&index, tour("t"), Author::Human, text("same"));
    let other = comments.add(&index, tour("t"), Author::Human, text("same"));
    assert_ne!(one, other);
    assert_eq!(comments.iter().count(), 2);
}

#[test]
fn a_code_comment_follows_its_symbol_and_goes_stale_when_its_text_changes() {
    let index = one_file();
    let mut comments = Comments::default();
    let target = CommentTarget::code(&index, first_file(), span(4, 4)).unwrap();
    let id = comments.add(&index, target, Author::Human, text("odd"));
    let moved = index_of(
        &["// new", "fn a() {", "  1", "}", "fn b() {", "  2", "}"],
        vec![symbol("a", 1, 3), symbol("b", 4, 6)],
    );
    comments.resolve_all(&moved);
    let resolution = comments.get(&id).unwrap().resolution().unwrap();
    assert_eq!(resolution.span, span(5, 5));
    assert_eq!(resolution.freshness, Freshness::Current);
    let edited = index_of(
        &["fn a() {", "  1", "}", "fn b() {", "  3", "}"],
        vec![symbol("a", 0, 2), symbol("b", 3, 5)],
    );
    comments.resolve_all(&edited);
    let stale = comments.get(&id).unwrap().resolution().unwrap();
    assert_eq!(stale.freshness, Freshness::Stale);
}

#[test]
fn step_comments_are_found_by_their_step() {
    let index = one_file();
    let mut comments = Comments::default();
    let address = StepAddress {
        tour: TourName::new("t").unwrap(),
        step: StepId::new("abc123").unwrap(),
    };
    let id = comments.add(
        &index,
        CommentTarget::Step(address.clone()),
        Author::Human,
        text("here"),
    );
    let _other = comments.add(&index, tour("t"), Author::Human, text("there"));
    let found: Vec<&CommentId> = comments.on_step(&address).map(Comment::id).collect();
    assert_eq!(found, [&id]);
}

fn answer(comments: &mut Comments, id: &CommentId) {
    comments.reply(id, reply("done")).unwrap();
}

#[test]
fn the_walk_visits_answered_comments_in_id_order_and_wraps() {
    let index = one_file();
    let mut comments = Comments::default();
    let mut answered: Vec<CommentId> = ["a", "b", "c"]
        .into_iter()
        .map(|words| comments.add(&index, tour("t"), Author::Human, text(words)))
        .collect();
    let _open = comments.add(&index, tour("t"), Author::Human, text("still open"));
    for id in &answered {
        answer(&mut comments, id);
    }
    answered.sort();
    let walk =
        |after: Option<&CommentId>| comments.next_after(after).map(|found| found.id().clone());
    assert_eq!(walk(None).as_ref(), answered.first());
    assert_eq!(walk(answered.first()).as_ref(), answered.get(1));
    assert_eq!(walk(answered.get(1)).as_ref(), answered.get(2));
    assert_eq!(walk(answered.get(2)).as_ref(), answered.first());
}

#[test]
fn the_walk_visits_open_comments_when_none_is_answered() {
    let index = one_file();
    let mut comments = Comments::default();
    assert!(comments.next_after(None).is_none());
    let open = comments.add(&index, tour("t"), Author::Human, text("why?"));
    assert_eq!(comments.next_after(None).map(Comment::id), Some(&open));
    assert_eq!(
        comments.next_after(Some(&open)).map(Comment::id),
        Some(&open)
    );
}

#[test]
fn a_comment_on_a_removed_step_is_found_on_its_tour() {
    let index = one_file();
    let name = TourName::new("t").unwrap();
    let mut map = Map::new(Vec::new()).unwrap();
    let _added = map
        .add_tour(name.clone(), TourKind::Flow, Author::Human)
        .unwrap();
    let kept = map
        .add_step(&index, &name, first_file(), span(0, 2), Author::Human, None)
        .unwrap();
    let removed = map
        .add_step(&index, &name, first_file(), span(3, 5), Author::Human, None)
        .unwrap();
    map.remove_step(&name, &removed).unwrap();
    let mut comments = Comments::default();
    let on_step = |step: &StepId| {
        CommentTarget::Step(StepAddress {
            tour: name.clone(),
            step: step.clone(),
        })
    };
    let _on_kept = comments.add(&index, on_step(&kept), Author::Human, text("here"));
    let gone = comments.add(&index, on_step(&removed), Author::Human, text("gone"));
    let _elsewhere = comments.add(
        &index,
        CommentTarget::Step(StepAddress {
            tour: TourName::new("other").unwrap(),
            step: removed.clone(),
        }),
        Author::Human,
        text("other tour"),
    );
    let tour = map.tour(&name).unwrap();
    let found: Vec<&CommentId> = comments.on_gone_steps(tour).map(Comment::id).collect();
    assert_eq!(found, [&gone]);
}

#[test]
fn blank_text_makes_no_comment_and_no_reply() {
    assert!(CommentText::new(" \n\t").is_none());
    assert!(ReplyText::new("").is_none());
    assert!(CommentText::new(" why? ").is_some());
}

#[test]
fn a_comment_id_is_six_lowercase_base36_digits() {
    assert!(CommentId::new("a1b2c3").is_some());
    assert!(CommentId::new("A1B2C3").is_none());
    assert!(CommentId::new("a1b2c").is_none());
    assert!(CommentId::new("a1b2c3d").is_none());
    assert!(CommentId::new("a1b-c3").is_none());
}
