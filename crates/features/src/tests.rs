use std::collections::BTreeSet;

use strum::VariantArray;

use crate::{Feature, Surface, Trigger};

#[test]
fn every_feature_has_a_unique_name_and_a_trigger() {
    let mut names = BTreeSet::new();
    for feature in Feature::VARIANTS {
        let spec = feature.spec();
        assert!(names.insert(spec.name()), "{feature:?} repeats a name");
        assert!(!spec.triggers().is_empty(), "{feature:?} has no trigger");
        assert!(
            !spec.summary().as_str().is_empty(),
            "{feature:?} has no summary"
        );
    }
}

#[test]
fn a_command_feature_is_triggered_by_its_own_name() {
    for feature in Feature::VARIANTS {
        let spec = feature.spec();
        if spec.surface() == Surface::Command {
            assert_eq!(spec.triggers(), [Trigger::Command(spec.name())]);
        }
    }
}

#[test]
fn every_trigger_belongs_to_one_feature() {
    let mut seen = BTreeSet::new();
    for feature in Feature::VARIANTS {
        for trigger in feature.spec().triggers() {
            assert!(seen.insert(*trigger), "{trigger:?} triggers two features");
        }
    }
}

#[test]
fn a_palette_entry_shows_only_a_chord_its_feature_is_bound_to() {
    for feature in Feature::VARIANTS {
        let triggers = feature.spec().triggers();
        for trigger in triggers {
            if let Trigger::Palette(label, Some(chord)) = trigger {
                assert!(
                    triggers.contains(&Trigger::Key(*chord)),
                    "{feature:?} shows {chord:?} beside '{}' but no key triggers it",
                    label.as_str()
                );
            }
        }
    }
}
