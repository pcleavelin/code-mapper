use std::collections::BTreeSet;

use crate::model::{Model, StepKey, TourSlot};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Unit {
    #[default]
    Crate,
    File,
}

impl Unit {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Crate => "crate",
            Self::File => "file",
        }
    }

    pub(crate) fn of(self, file: &str) -> String {
        let parts: Vec<&str> = file.split('/').collect();
        let owner = match parts.as_slice() {
            ["crates" | "packages" | "libs" | "apps", name, _, ..] => Some(*name),
            [dir, _, ..] => Some(*dir),
            _ => None,
        };
        match self {
            Self::Crate => owner.unwrap_or(file).to_owned(),
            Self::File => {
                let leaf = parts.last().copied().unwrap_or(file);
                match owner {
                    Some(owner) if owner != leaf => format!("{owner}/{leaf}"),
                    _ => leaf.to_owned(),
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Lifelines {
    #[default]
    Auto,
    Chosen(Unit),
}

impl Lifelines {
    pub(crate) const fn turned(self) -> Self {
        match self {
            Self::Auto => Self::Chosen(Unit::Crate),
            Self::Chosen(Unit::Crate) => Self::Chosen(Unit::File),
            Self::Chosen(Unit::File) => Self::Auto,
        }
    }
}

const AUTO_CRATES: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Calls {
    #[default]
    All,
    Crossing,
}

impl Calls {
    pub(crate) const fn turned(self) -> Self {
        match self {
            Self::All => Self::Crossing,
            Self::Crossing => Self::All,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SequenceState {
    pub(crate) lifelines: Lifelines,
    pub(crate) expanded: BTreeSet<StepKey>,
    pub(crate) calls: Calls,
    pub(crate) followed: Option<StepKey>,
    pub(crate) root: Option<TourSlot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SequenceAction {
    TurnUnit,
    TurnCalls,
    Followed(StepKey),
    ToggleLink(StepKey),
    UnfoldAll(Vec<StepKey>),
    FoldAll,
    Root(TourSlot),
}

impl SequenceState {
    pub(crate) fn apply(&mut self, action: SequenceAction) {
        match action {
            SequenceAction::TurnUnit => self.lifelines = self.lifelines.turned(),
            SequenceAction::TurnCalls => self.calls = self.calls.turned(),
            SequenceAction::Followed(key) => self.followed = Some(key),
            SequenceAction::ToggleLink(key) => {
                if !self.expanded.remove(&key) {
                    self.expanded.insert(key);
                }
            }
            SequenceAction::UnfoldAll(keys) => self.expanded.extend(keys),
            SequenceAction::FoldAll => self.expanded.clear(),
            SequenceAction::Root(path) => self.root = Some(path),
        }
    }
}

pub(crate) const ACTOR: usize = 0;

#[derive(Clone, Debug)]
pub(crate) struct Message {
    pub(crate) key: StepKey,
    pub(crate) depth: usize,
    pub(crate) number: String,
    pub(crate) symbol: String,
    pub(crate) place: String,
    pub(crate) note: String,
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) link: Option<String>,
    pub(crate) unfolded: Option<bool>,
    pub(crate) via: Option<String>,
    pub(crate) stale: bool,
    pub(crate) end: usize,
    pub(crate) level: usize,
    pub(crate) from_level: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Participant {
    pub(crate) name: String,
    pub(crate) steps: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Diagram {
    pub(crate) unit: Unit,
    pub(crate) participants: Vec<Participant>,
    pub(crate) messages: Vec<Message>,
}

struct Walk<'model> {
    model: &'model Model,
    unit: Unit,
    expanded: &'model BTreeSet<StepKey>,
    everything: bool,
    diagram: Diagram,
    last_at_depth: Vec<usize>,
    open: BTreeSet<TourSlot>,
}

impl Walk<'_> {
    fn participant(&mut self, name: String) -> usize {
        if let Some(at) = self
            .diagram
            .participants
            .iter()
            .position(|participant| participant.name == name)
        {
            return at;
        }
        self.diagram
            .participants
            .push(Participant { name, steps: 0 });
        self.diagram.participants.len() - 1
    }

    fn path(
        &mut self,
        path: TourSlot,
        offset: usize,
        prefix: &str,
        via: Option<&str>,
        nesting: usize,
    ) {
        if !self.open.insert(path) {
            return;
        }
        for numbered in self.model.numbered(path) {
            let key = StepKey {
                tour: path,
                step: numbered.step,
            };
            let Some(step) = self.model.step(key) else {
                continue;
            };
            let depth = offset + usize::try_from(numbered.depth.value()).unwrap_or(0);
            let to = self.participant(self.unit.of(step.file().as_str()));
            if let Some(participant) = self.diagram.participants.get_mut(to) {
                participant.steps += 1;
            }
            let from = if depth == 0 {
                ACTOR
            } else {
                self.last_at_depth
                    .get(depth - 1)
                    .and_then(|row| self.diagram.messages.get(*row))
                    .map_or(ACTOR, |parent| parent.to)
            };
            let row = self.diagram.messages.len();
            self.last_at_depth.truncate(depth);
            self.last_at_depth.push(row);
            let span = step.span();
            let number = format!("{prefix}{}", numbered.number.as_str());
            let link = step.link().map(ToString::to_string);
            let unfolded = step
                .link()
                .and_then(|name| self.model.find_tour(name))
                .map(|linked| {
                    !self.open.contains(&linked)
                        && (self.everything || self.expanded.contains(&key))
                });
            self.diagram.messages.push(Message {
                key,
                depth,
                number: number.clone(),
                symbol: step
                    .symbol()
                    .map_or_else(|| "(lines)".to_owned(), ToString::to_string),
                place: format!(
                    "{}:{}-{}",
                    step.file().as_str(),
                    span.start().number(),
                    span.end().number()
                ),
                note: step
                    .note()
                    .map_or_else(String::new, |note| note.as_str().to_owned()),
                from,
                to,
                link: link.clone(),
                unfolded,
                via: via.map(str::to_owned),
                stale: step.is_stale(),
                end: row,
                level: 0,
                from_level: 0,
            });
            if unfolded == Some(true)
                && let Some(name) = step.link()
                && let Some(linked) = self.model.find_tour(name)
            {
                let prefix = format!("{number} \u{203a} ");
                self.path(linked, depth + 1, &prefix, link.as_deref(), nesting + 1);
            }
        }
        self.open.remove(&path);
    }
}

fn walked(
    model: &Model,
    path: TourSlot,
    unit: Unit,
    expanded: &BTreeSet<StepKey>,
    everything: bool,
) -> Diagram {
    let actor = model
        .tour(path)
        .map_or_else(String::new, |found| found.name().to_string());
    let mut walk = Walk {
        model,
        unit,
        expanded,
        everything,
        diagram: Diagram {
            unit,
            participants: vec![Participant {
                name: actor,
                steps: 0,
            }],
            messages: Vec::new(),
        },
        last_at_depth: Vec::new(),
        open: BTreeSet::new(),
    };
    walk.path(path, 0, "", None, 0);
    walk.diagram
}

fn nested(diagram: &mut Diagram) {
    let depths: Vec<usize> = diagram
        .messages
        .iter()
        .map(|message| message.depth)
        .collect();
    for (row, message) in diagram.messages.iter_mut().enumerate() {
        let mut end = row;
        while depths
            .get(end + 1)
            .is_some_and(|depth| *depth > message.depth)
        {
            end += 1;
        }
        message.end = end;
    }
    let spans: Vec<(usize, usize, usize)> = diagram
        .messages
        .iter()
        .enumerate()
        .map(|(row, message)| (row, message.end, message.to))
        .collect();
    let level_at = |row: usize, participant: usize| {
        spans
            .iter()
            .filter(|(start, end, to)| *to == participant && *start < row && *end >= row)
            .count()
    };
    let levels: Vec<(usize, usize)> = diagram
        .messages
        .iter()
        .enumerate()
        .map(|(row, message)| {
            let from_level = if message.from == ACTOR {
                0
            } else {
                level_at(row, message.from).saturating_sub(1)
            };
            (level_at(row, message.to), from_level)
        })
        .collect();
    for (message, (level, from_level)) in diagram.messages.iter_mut().zip(levels) {
        message.level = level;
        message.from_level = from_level;
    }
}

pub(crate) fn link_keys(model: &Model, path: TourSlot) -> Vec<StepKey> {
    walked(model, path, Unit::Crate, &BTreeSet::new(), true)
        .messages
        .iter()
        .filter(|message| message.link.is_some())
        .map(|message| message.key)
        .collect()
}

pub(crate) fn diagram(model: &Model, path: TourSlot, state: &SequenceState) -> Diagram {
    let mut diagram = match state.lifelines {
        Lifelines::Chosen(unit) => walked(model, path, unit, &state.expanded, false),
        Lifelines::Auto => {
            let crates = walked(model, path, Unit::Crate, &state.expanded, false);
            if crates.participants.len() > AUTO_CRATES {
                crates
            } else {
                walked(model, path, Unit::File, &state.expanded, false)
            }
        }
    };
    if state.calls == Calls::Crossing {
        diagram
            .messages
            .retain(|message| message.from != message.to || message.from == ACTOR);
    }
    nested(&mut diagram);
    diagram
}
