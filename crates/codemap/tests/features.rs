use std::fs;
use std::path::{Path, PathBuf};

use domain::{Root, TourKind, TourName};
use features::{Feature, Gesture, Key, Modifiers, Surface, Trigger};
use io_map::MapStore;
use strum::VariantArray;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)).unwrap_or_default()
}

fn group_of(surface: Surface) -> &'static str {
    match surface {
        Surface::Command => "features/cli",
        Surface::Window => "features/gui",
    }
}

fn key_word(key: Key) -> String {
    match key {
        Key::Up => "up".to_owned(),
        Key::Down => "down".to_owned(),
        Key::Left => "left".to_owned(),
        Key::Right => "right".to_owned(),
        Key::Letter(letter) => letter.as_char().to_string(),
    }
}

fn script_lines(scripts: &str) -> Vec<&str> {
    scripts.lines().map(str::trim).collect()
}

fn aims_at(line: &str, element: &str) -> bool {
    line.split_whitespace().nth(1).is_some_and(|target| {
        target == element
            || target.starts_with(&format!("{element}/"))
            || target.starts_with(&format!("{element}@"))
    })
}

fn exercised(trigger: Trigger, commands: &str, scripts: &str) -> bool {
    let lines = script_lines(scripts);
    match trigger {
        Trigger::Command(name) => {
            let opening = format!("[\"{}\"", name.as_str());
            commands.match_indices(&opening).any(|(at, _)| {
                commands
                    .get(at + opening.len()..)
                    .is_some_and(|rest| rest.starts_with([',', ']']))
            })
        }
        Trigger::Click(element) | Trigger::Type(element) => {
            let name = element.as_str();
            lines.iter().any(|line| {
                (line.starts_with("click-id ") || line.starts_with("dblclick-id "))
                    && aims_at(line, name)
            }) || graph_button(name).is_some_and(|label| {
                lines
                    .iter()
                    .any(|line| line.starts_with("click <<DUMP ") && line.contains(label))
            })
        }
        Trigger::Key(chord) => {
            let modifier = match chord.modifiers() {
                Modifiers::Plain => None,
                Modifiers::Control => Some("ctrl"),
                Modifiers::Alt => Some("alt"),
            };
            lines.iter().any(|line| {
                let words: Vec<&str> = line.split_whitespace().collect();
                words.first() == Some(&"key")
                    && words.get(1) == Some(&key_word(chord.key()).as_str())
                    && modifier.is_none_or(|word| words.contains(&word))
            })
        }
        Trigger::Gesture(gesture, _) => lines.iter().any(|line| match gesture {
            Gesture::Hover => line.starts_with("hover-id ") || line.starts_with("mouse "),
            Gesture::Drag => line.starts_with("drag ") || line == &"down",
            Gesture::Wheel => {
                line.starts_with("wheel ") && !line.contains("ctrl") && !line.contains("shift")
            }
            Gesture::ShiftWheel => line.starts_with("wheel ") && line.contains("shift"),
            Gesture::ControlWheel => line.starts_with("wheel ") && line.contains("ctrl"),
            Gesture::Pinch => line.starts_with("pinch "),
            Gesture::DoubleClick => line.starts_with("dblclick"),
            Gesture::ControlClick => line.starts_with("click") && line.ends_with(" ctrl"),
            Gesture::AltClick => line.starts_with("click") && line.ends_with(" alt"),
            Gesture::BackButton | Gesture::ForwardButton => false,
        }),
        Trigger::Palette(label, _) => lines.iter().any(|line| {
            line.strip_prefix("text ").is_some_and(|typed| {
                let typed = typed.trim_start_matches('>').trim();
                !typed.is_empty()
                    && label
                        .as_str()
                        .to_lowercase()
                        .starts_with(&typed.to_lowercase())
            })
        }),
    }
}

fn graph_button(element: &str) -> Option<&'static str> {
    match element {
        "node-button" => Some("'callees"),
        "node-context" => Some("'[more-below]"),
        "node-source" => Some("'source"),
        "node-preview" => Some("'less"),
        "node-add" => Some("'+ step"),
        "box-parent" => Some("DUMP parent "),
        _ => None,
    }
}

#[test]
fn every_feature_has_its_flow_tour_in_the_map() {
    let root = Root::new(&workspace());
    let loaded = MapStore::new(&root).load();
    assert!(loaded.is_ok(), "the map loads");
    let Ok(map) = loaded else { return };
    let mut missing = Vec::new();
    for feature in Feature::VARIANTS {
        let spec = feature.spec();
        let wanted = format!("feature-{}", spec.name().as_str());
        let Ok(name) = TourName::new(&wanted) else {
            missing.push(format!("{wanted}: not a tour name"));
            continue;
        };
        match map.tour(&name) {
            None => missing.push(format!("{wanted}: no such tour")),
            Some(tour) => {
                if tour.kind() != TourKind::Flow {
                    missing.push(format!("{wanted}: not a flow"));
                }
                if tour.group().map(domain::GroupName::as_str) != Some(group_of(spec.surface())) {
                    missing.push(format!("{wanted}: not in {}", group_of(spec.surface())));
                }
                let roots = tour
                    .steps()
                    .iter()
                    .filter(|step| step.parent().is_none())
                    .count();
                if roots != 1 {
                    missing.push(format!(
                        "{wanted}: {roots} roots, the handler is the one root"
                    ));
                }
            }
        }
    }
    assert_eq!(missing.len(), 0, "feature tours:\n{}", missing.join("\n"));
}

#[test]
fn every_feature_is_exercised_by_a_scenario() {
    let scripts = read("tests/common/gui.rs");
    let commands = read("tests/common/cli.rs") + &scripts;
    let unexercised: Vec<&str> = Feature::VARIANTS
        .iter()
        .copied()
        .filter(|feature| {
            !feature
                .spec()
                .triggers()
                .iter()
                .any(|trigger| exercised(*trigger, &commands, &scripts))
        })
        .map(|feature| feature.spec().name().as_str())
        .collect();
    assert_eq!(
        unexercised.len(),
        0,
        "features no scenario triggers: {unexercised:?}"
    );
}
