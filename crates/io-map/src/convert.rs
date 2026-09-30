use std::collections::BTreeSet;

use domain::{
    Anchor, GroupName, Line, LineOffset, Map, MapError, Note, RelativePath, Step, StepId,
    StepOrder, SymbolName, TextHash, Tour, TourName,
};

use crate::error::{Fault, FieldKey, FieldValue, Located};
use crate::wire::{CmapStep, CmapTour, StepKey, WireFault};

impl From<WireFault> for Located {
    fn from(fault: WireFault) -> Self {
        Self::at(Line::new(fault.line), fault.fault)
    }
}

fn name_fault(error: MapError) -> Fault {
    match error {
        MapError::InvalidName(name) => Fault::InvalidName(name),
        other => Fault::Map(other),
    }
}

fn step_from_wire(step: &CmapStep, tour: &TourName) -> Result<Step, Located> {
    let here = Line::new(step.line);
    let missing = |key: StepKey| {
        Located::at(
            here,
            Fault::MissingField(FieldKey::new(key.name().as_str())),
        )
    };
    let id = StepId::new(&step.id)
        .ok_or_else(|| Located::at(here, Fault::InvalidStepId(FieldValue::new(&step.id))))?;
    let order = step.order.ok_or_else(|| missing(StepKey::Order))?;
    let file = step.file.as_deref().ok_or_else(|| missing(StepKey::File))?;
    let (start, end) = step.lines.ok_or_else(|| missing(StepKey::Lines))?;
    let hash = step.hash.ok_or_else(|| missing(StepKey::Hash))?;
    let parent = match &step.parent {
        None => None,
        Some((parent, line)) => Some(StepId::new(parent).ok_or_else(|| {
            Located::at(
                Line::new(*line),
                Fault::UnknownParent {
                    tour: tour.clone(),
                    step: id.clone(),
                    parent: FieldValue::new(parent),
                },
            )
        })?),
    };
    let link = match &step.link {
        Some((target, line)) if !target.is_empty() => Some(
            TourName::new(target)
                .map_err(|error| Located::at(Line::new(*line), name_fault(error)))?,
        ),
        _ => None,
    };
    let symbol = (!step.symbol.is_empty()).then(|| SymbolName::new(&step.symbol));
    let anchor = Anchor::new(
        RelativePath::new(file),
        symbol,
        LineOffset::new(start),
        LineOffset::new(end),
        TextHash::new(hash),
    );
    Ok(Step::new(
        id,
        StepOrder::new(order),
        parent,
        step.author,
        anchor,
        Note::new(&step.note),
        link,
    ))
}

pub(crate) fn tour_from_wire(cmap: &CmapTour) -> Result<Tour, Located> {
    let Some((raw_name, name_line)) = &cmap.name else {
        return Err(Located::anywhere(Fault::NoTourLine));
    };
    let name = TourName::new(raw_name)
        .map_err(|error| Located::at(Line::new(*name_line), name_fault(error)))?;
    let steps = cmap
        .steps
        .iter()
        .map(|step| step_from_wire(step, &name))
        .collect::<Result<Vec<Step>, Located>>()?;
    let ids: BTreeSet<&StepId> = steps.iter().map(Step::id).collect();
    let mut in_order: Vec<(&Step, &CmapStep)> = steps.iter().zip(&cmap.steps).collect();
    in_order
        .sort_by(|one, other| (one.0.order(), one.0.id()).cmp(&(other.0.order(), other.0.id())));
    for (step, raw) in in_order {
        if let (Some(parent), Some((raw_parent, line))) = (step.parent(), &raw.parent)
            && !ids.contains(parent)
        {
            return Err(Located::at(
                Line::new(*line),
                Fault::UnknownParent {
                    tour: name.clone(),
                    step: step.id().clone(),
                    parent: FieldValue::new(raw_parent),
                },
            ));
        }
    }
    Tour::new(
        name,
        cmap.kind,
        cmap.author,
        GroupName::new(&cmap.group),
        Note::new(&cmap.note),
        steps,
    )
    .map_err(|error| Located::anywhere(Fault::Map(error)))
}

pub(crate) fn map_from_tours(tours: Vec<Tour>) -> Result<Map, Located> {
    Map::new(tours).map_err(|error| Located::anywhere(Fault::Map(error)))
}

fn step_to_wire(step: &Step) -> CmapStep {
    let anchor = step.anchor();
    CmapStep {
        id: step.id().as_str().to_owned(),
        line: 0,
        order: Some(step.order().value()),
        parent: step.parent().map(|parent| (parent.as_str().to_owned(), 0)),
        author: step.author(),
        file: Some(anchor.file().as_str().to_owned()),
        symbol: anchor
            .symbol()
            .map_or_else(String::new, |symbol| symbol.as_str().to_owned()),
        lines: Some((anchor.start().value(), anchor.end().value())),
        hash: Some(anchor.hash().value()),
        link: step.link().map(|link| (link.as_str().to_owned(), 0)),
        note: step
            .note()
            .map_or_else(String::new, |note| note.as_str().to_owned()),
    }
}

pub(crate) fn tour_to_wire(tour: &Tour) -> CmapTour {
    CmapTour {
        name: Some((tour.name().as_str().to_owned(), 0)),
        kind: tour.kind(),
        author: tour.author(),
        group: tour
            .group()
            .map_or_else(String::new, |group| group.as_str().to_owned()),
        note: tour
            .note()
            .map_or_else(String::new, |note| note.as_str().to_owned()),
        steps: tour.steps().iter().map(step_to_wire).collect(),
    }
}
