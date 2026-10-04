use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::atomic::{AtomicU32, Ordering};

use domain::{
    Anchor, Author, Comment, CommentId, CommentState, CommentTarget, CommentText, Line, LineOffset,
    RelativePath, Reply, ReplyText, Root, StepAddress, StepId, SymbolName, TextHash, TourName,
};

use crate::{CommentFault, CommentLoadError, CommentStore, FieldKey, FieldValue};

static SCRATCH_COUNT: AtomicU32 = AtomicU32::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let count = SCRATCH_COUNT.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!("io-comments-{label}-{}-{count}", process::id()));
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn store(&self) -> CommentStore {
        CommentStore::new(&Root::new(&self.0))
    }

    fn put(&self, name: &str, text: &str) {
        let directory = self.store().directory().to_path_buf();
        fs::create_dir_all(&directory).unwrap();
        io_store::write(&directory.join(name), text).unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        drop(fs::remove_dir_all(&self.0));
    }
}

fn id(text: &str) -> CommentId {
    CommentId::new(text).unwrap()
}

fn comment(name: &str, target: CommentTarget, state: CommentState) -> Comment {
    Comment::new(
        id(name),
        target,
        Author::Human,
        CommentText::new("why this?\nand a backslash \\ here").unwrap(),
        state,
    )
}

fn code() -> CommentTarget {
    CommentTarget::Code(Anchor::new(
        RelativePath::new("src/a.rs"),
        Some(SymbolName::new("pin_span")),
        LineOffset::new(2),
        LineOffset::new(-1),
        TextHash::new(0x0123_4567_89ab_cdef),
    ))
}

fn step() -> CommentTarget {
    CommentTarget::Step(StepAddress {
        tour: TourName::new("anchor").unwrap(),
        step: StepId::new("abc123").unwrap(),
    })
}

fn tour() -> CommentTarget {
    CommentTarget::Tour(TourName::new("anchor").unwrap())
}

#[test]
fn outside_a_repository_comments_live_under_the_root() {
    let scratch = Scratch::new("local");
    assert_eq!(
        scratch.store().directory(),
        scratch.0.join(".codemap-comments")
    );
}

#[test]
fn every_kind_of_comment_reads_back_as_written() {
    let scratch = Scratch::new("round-trip");
    let store = scratch.store();
    let answered = CommentState::Answered(Reply {
        text: ReplyText::new("done; see tour anchor").unwrap(),
        author: Author::Agent,
    });
    let written = vec![
        comment("aaaaaa", code(), CommentState::Open),
        comment("bbbbbb", step(), answered),
        comment("cccccc", tour(), CommentState::Open),
    ];
    for one in &written {
        store.write(one).unwrap();
    }
    let read = store.load().unwrap();
    let read: Vec<&Comment> = read.iter().collect();
    assert_eq!(read, written.iter().collect::<Vec<_>>());
}

#[test]
fn remove_deletes_only_that_comment() {
    let scratch = Scratch::new("remove");
    let store = scratch.store();
    store
        .write(&comment("aaaaaa", tour(), CommentState::Open))
        .unwrap();
    store
        .write(&comment("bbbbbb", tour(), CommentState::Open))
        .unwrap();
    store.remove(&id("aaaaaa")).unwrap();
    store.remove(&id("aaaaaa")).unwrap();
    let left: Vec<CommentId> = store
        .load()
        .unwrap()
        .iter()
        .map(|one| one.id().clone())
        .collect();
    assert_eq!(left, [id("bbbbbb")]);
}

#[test]
fn another_version_is_refused() {
    let scratch = Scratch::new("version");
    scratch.put(
        "aaaaaa.comment",
        "codemap-comment 2\ntarget tour\ntour t\nauthor human\ntext x\n",
    );
    match scratch.store().load() {
        Err(CommentLoadError::Parse(error)) => {
            assert_eq!(error.line, Some(Line::new(0)));
            assert_eq!(
                error.fault,
                CommentFault::Version(FieldValue::new("codemap-comment 2"))
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_reply_without_its_author_is_refused() {
    let scratch = Scratch::new("reply");
    scratch.put(
        "aaaaaa.comment",
        "codemap-comment 1\ntarget tour\ntour t\nauthor human\ntext x\nreply done\n",
    );
    match scratch.store().load() {
        Err(CommentLoadError::Parse(error)) => assert_eq!(
            error.fault,
            CommentFault::MissingField(FieldKey::new("reply-author"))
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_file_not_named_by_an_id_is_refused() {
    let scratch = Scratch::new("name");
    scratch.put(
        "Not-An-Id.comment",
        "codemap-comment 1\ntarget tour\ntour t\nauthor human\ntext x\n",
    );
    assert!(matches!(
        scratch.store().load(),
        Err(CommentLoadError::InvalidId(_))
    ));
}

#[test]
fn the_stamp_moves_when_a_comment_is_written_or_removed() {
    let scratch = Scratch::new("stamp");
    let store = scratch.store();
    assert_eq!(store.stamp(), None);
    store
        .write(&comment("aaaaaa", tour(), CommentState::Open))
        .unwrap();
    let one = store.stamp().unwrap();
    store
        .write(&comment("bbbbbb", tour(), CommentState::Open))
        .unwrap();
    let two = store.stamp().unwrap();
    assert_ne!(one, two);
    store.remove(&id("bbbbbb")).unwrap();
    assert_ne!(store.stamp().unwrap(), two);
}
