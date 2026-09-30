use std::collections::BTreeSet;

use clap::CommandFactory;
use domain::{
    Depth, FileText, Imports, Line, MapError, PathName, RelativePath, SourceFile, Span,
    StepAddress, StepId, SymbolName, TextFragment,
};
use features::{Feature, Surface};
use io_map::{Fault, FieldKey, FieldValue, Origin, ParseError};

use crate::convert::{Count, Levels, LineNumber, StepIndex, Under, line_range};
use crate::failure::{Candidate, Failure, StepPlace};
use crate::output::Output;
use crate::wire::{self, Command};
use crate::{Argument, Channel, CommandLine, Invocation};

fn arguments(words: &[&str]) -> Vec<Argument> {
    words.iter().map(|word| Argument::new(word)).collect()
}

fn file(lines: usize) -> SourceFile {
    let text = (0..lines)
        .map(|line| format!("line {line}\n"))
        .collect::<Vec<String>>()
        .concat();
    let text = FileText::from(text.as_str());
    let hash = text.whole_hash();
    SourceFile::new(
        RelativePath::new("src/a.rs"),
        text,
        Vec::new(),
        Vec::new(),
        Imports::new(),
        hash,
        domain::Backend::TreeSitter,
    )
}

fn span(start: u32, end: u32) -> Option<Span> {
    Span::new(Line::new(start), Line::new(end))
}

#[test]
fn every_command_feature_is_a_subcommand_and_every_subcommand_a_command_feature() {
    let subcommands = wire::subcommands();
    let names: BTreeSet<String> = subcommands.iter().map(|pair| pair.0.clone()).collect();
    let features: BTreeSet<String> = Feature::ALL
        .into_iter()
        .filter(|feature| feature.spec().surface() == Surface::Command)
        .map(|feature| feature.spec().name().as_str().to_owned())
        .collect();
    assert_eq!(names, features);
    for (name, about) in &subcommands {
        let feature = Feature::ALL
            .into_iter()
            .find(|feature| feature.spec().name().as_str() == name)
            .unwrap();
        if feature != Feature::Help {
            assert_eq!(about, feature.spec().summary().as_str(), "{name}");
        }
    }
}

#[test]
fn help_subcommand_line_carries_its_feature_summary() {
    let rendered = wire::help();
    let summary = Feature::Help.spec().summary();
    let line = rendered
        .as_str()
        .lines()
        .find(|line| line.trim_start().starts_with("help "))
        .unwrap();
    assert!(line.contains(summary.as_str()), "{line}");
}

#[test]
fn every_subcommand_parses_to_its_own_feature() {
    let command = Command::command();
    let mut seen = BTreeSet::new();
    for sub in command.get_subcommands() {
        let mut words = vec![sub.get_name().to_owned()];
        for argument in sub
            .get_positionals()
            .filter(|argument| argument.is_required_set())
        {
            let value = if argument.get_id() == "kind" {
                "flow"
            } else {
                "1"
            };
            words.push(value.to_owned());
        }
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let invocation = Invocation::parse(&arguments(&words)).unwrap();
        assert_eq!(
            invocation.feature().spec().name().as_str(),
            sub.get_name(),
            "{words:?}"
        );
        seen.insert(invocation.feature());
    }
    assert_eq!(seen.len(), command.get_subcommands().count());
}

#[test]
fn help_is_the_usage_and_one_line_per_command() {
    let help = crate::help();
    let text = help.as_str();
    assert!(text.starts_with("Usage: codemap <root>"));
    assert!(text.contains(
        "  path-group    <name> <group>                   put a path in a group; / nests groups (flows/http), \"\" = top level\n"
    ));
    assert!(
        text.ends_with(
            "  help          Print this message or the help of the given subcommand(s)\n"
        )
    );
}

#[test]
fn a_parse_failure_goes_to_stderr_and_help_to_stdout() {
    let failure = Invocation::parse(&arguments(&["bogus"])).unwrap_err();
    assert_eq!(failure.channel(), Channel::Stderr);
    assert!(
        failure
            .text()
            .as_str()
            .starts_with("error: unrecognized subcommand 'bogus'")
    );
    let invalid = Invocation::parse(&arguments(&["path-new", "x", "kinda"])).unwrap_err();
    assert!(invalid.text().as_str().starts_with(
        "error: invalid value 'kinda' for '<KIND>': expected one of flow, layer, type"
    ));
    let help = Invocation::parse(&arguments(&["help"])).unwrap_err();
    assert_eq!(help.channel(), Channel::Stdout);
}

#[test]
fn negative_numbers_reach_the_commands_that_take_them() {
    for words in [
        &["note-edit", "p", "-1", "a", "b"][..],
        &["path-add", "p", "sym", "-1"],
        &["path-move", "p", "0", "-1"],
    ] {
        Invocation::parse(&arguments(words)).unwrap();
    }
}

#[test]
fn a_command_line_splits_on_spaces_outside_quotes() {
    let line = CommandLine::new("path-note  startup \"the first path\" x");
    let words: Vec<String> = line
        .arguments()
        .iter()
        .map(|argument| argument.as_str().to_owned())
        .collect();
    assert_eq!(words, ["path-note", "startup", "the first path", "x"]);
}

#[test]
fn failures_read_as_the_legacy_messages() {
    let name = PathName::new("startup").unwrap();
    let cases: Vec<(Failure, &str)> = vec![
        (
            Failure::Map(MapError::NameTaken(name.clone())),
            "a path named 'startup' already exists",
        ),
        (
            Failure::Map(MapError::InvalidName(domain::InvalidName::new(".x"))),
            "'.x' cannot name a path: use letters, digits, '.', '_' and '-', not starting with '.'",
        ),
        (
            Failure::Map(MapError::CaseClash {
                name: PathName::new("Startup").unwrap(),
                other: name.clone(),
            }),
            "'Startup' differs from the path 'startup' only in letter case",
        ),
        (
            Failure::Map(MapError::NoSuchStep(StepAddress {
                path: name.clone(),
                step: StepId::new("abc123").unwrap(),
            })),
            "no such step",
        ),
        (
            Failure::Map(MapError::UnderItself),
            "a step cannot go under itself or its own descendants",
        ),
        (
            Failure::Map(MapError::NoteLacks(TextFragment::new("zzz"))),
            "the note does not contain 'zzz'",
        ),
        (
            Failure::LinkedFrom {
                path: name.clone(),
                steps: vec![StepPlace {
                    path: PathName::new("scratch").unwrap(),
                    index: StepIndex::new(1),
                }],
            },
            "'startup' is linked from scratch[1]; unlink those steps first",
        ),
        (
            Failure::NoPlaceUnder {
                under: Under::new(9),
                steps: Count::new(4),
            },
            "no step [9] to go under: the path has 4 steps (-1 = root)",
        ),
        (Failure::NoLink(StepIndex::new(2)), "step [2] has no link"),
        (
            Failure::NoSuchFile(RelativePath::new("nope.rs")),
            "no such file: nope.rs",
        ),
        (
            Failure::Stale {
                steps: Count::new(1),
                links: Count::new(0),
            },
            "1 stale steps",
        ),
        (
            Failure::Stale {
                steps: Count::new(0),
                links: Count::new(2),
            },
            "2 broken links",
        ),
        (
            Failure::Stale {
                steps: Count::new(1),
                links: Count::new(2),
            },
            "1 stale steps, 2 broken links",
        ),
        (Failure::NoRepository, "not in a jj or git repo"),
        (
            Failure::SymbolClash {
                query: SymbolName::new("len"),
                candidates: vec![Candidate {
                    name: Output::of("store:len".to_owned()),
                    label: Output::of("len src/store.rs:5-7".to_owned()),
                }],
            },
            "ambiguous: len; use one of\n  store:len                                len src/store.rs:5-7",
        ),
    ];
    for (failure, message) in cases {
        assert_eq!(failure.to_string(), message);
    }
}

#[test]
fn a_parse_error_names_its_origin_and_line() {
    let at_file = ParseError {
        origin: Origin::File("root\\.codemap\\a.cmap".into()),
        line: Some(Line::new(21)),
        fault: Fault::Conflict,
    };
    assert_eq!(
        Failure::Parse(at_file).to_string(),
        "root/.codemap/a.cmap:22: an unresolved merge conflict"
    );
    let at_revision = ParseError {
        origin: Origin::Revision(domain::Revision::new("@-")),
        line: None,
        fault: Fault::NoPathLine,
    };
    assert_eq!(
        Failure::Parse(at_revision).to_string(),
        ".codemap at @-: a path with no 'path' line"
    );
    let version = ParseError {
        origin: Origin::Revision(domain::Revision::new("HEAD")),
        line: Some(Line::new(0)),
        fault: Fault::Version(FieldValue::new("codemap 7")),
    };
    assert_eq!(
        Failure::Parse(version).to_string(),
        ".codemap at HEAD:1: 'codemap 7' is not 'codemap 8'; regenerate this map"
    );
    let field = ParseError {
        origin: Origin::Revision(domain::Revision::new("HEAD")),
        line: Some(Line::new(2)),
        fault: Fault::UnknownStepField(FieldKey::new("colour")),
    };
    assert_eq!(
        Failure::Parse(field).to_string(),
        ".codemap at HEAD:3: unknown field 'colour' of a step"
    );
}

#[test]
fn under_admits_minus_one_and_every_existing_step() {
    let steps = Count::new(4);
    assert!(Under::new(-1).fits(steps));
    assert!(Under::new(3).fits(steps));
    assert!(!Under::new(4).fits(steps));
    assert!(!Under::new(-2).fits(steps));
    assert_eq!(Under::new(-1).step(), None);
    assert_eq!(Under::new(2).step(), Some(StepIndex::new(2)));
    assert_eq!(Count::new(0).last(), Under::new(-1));
    assert_eq!(Count::new(4).last(), Under::new(3));
    assert_eq!(Under::of(None).to_string(), "-1");
}

#[test]
fn one_based_line_ranges_convert_to_spans() {
    let source = file(10);
    let range =
        |start: i64, end: i64| line_range(&source, LineNumber::new(start), LineNumber::new(end));
    assert_eq!(range(1, 10), span(0, 9));
    assert_eq!(range(3, 3), span(2, 2));
    assert_eq!(range(0, 3), None);
    assert_eq!(range(4, 3), None);
    assert_eq!(range(1, 11), None);
}

#[test]
fn shown_lines_clamp_to_the_file() {
    let source = file(10);
    let number = |value: i64| Some(LineNumber::new(value));
    assert_eq!(LineNumber::visible(&source, None, None), span(0, 9));
    assert_eq!(
        LineNumber::visible(&source, number(0), number(99)),
        span(0, 9)
    );
    assert_eq!(LineNumber::visible(&source, number(7), number(3)), None);
    assert_eq!(LineNumber::visible(&source, number(23), None), None);
    assert_eq!(
        LineNumber::visible(&source, number(1), number(0)),
        span(0, 0)
    );
    assert_eq!(LineNumber::visible(&file(0), None, None), span(0, 0));
}

#[test]
fn levels_beyond_the_deepest_depth_saturate() {
    assert_eq!(Levels::new(4).depth(), Depth::new(4));
    assert_eq!(Levels::new(usize::MAX).depth(), Depth::new(u32::MAX));
}

#[test]
fn numbered_lines_are_right_aligned_and_one_based() {
    let source = file(3);
    let mut output = Output::new();
    wire::numbered_lines(&mut output, &source, span(1, 5).unwrap());
    assert_eq!(output.as_str(), "    2 line 1\n    3 line 2\n");
}

#[test]
fn coverage_ends_with_the_total() {
    let mut output = Output::new();
    wire::all_coverage(&mut output, Count::new(20), Count::new(33));
    assert_eq!(output.as_str(), "total: 20/33\n");
}
