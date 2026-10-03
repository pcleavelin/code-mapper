use domain::FontFamily;

use crate::wire::WireFace;
use crate::{FaceIndex, Style};

pub(crate) fn family(wire: &WireFace) -> Option<FontFamily> {
    (wire.monospaced && !wire.family.starts_with('.'))
        .then(|| FontFamily::new(&wire.family))
        .flatten()
}

pub(crate) const fn style(wire: &WireFace) -> Style {
    if wire.regular {
        Style::Regular
    } else {
        Style::Other
    }
}

pub(crate) const fn index(wire: &WireFace) -> FaceIndex {
    FaceIndex::new(wire.index)
}
