use std::collections::HashMap;

use fontdue::FontSettings;
use ui::{Coordinate, Extent, FontSize, Glyph, Point, Px};

use crate::atlas::{Bitmap, Texel};
use crate::error::{Reason, StartError};

#[derive(Clone, Copy, Debug)]
struct FontFile(&'static [u8]);

const BUNDLED: FontFile = FontFile(include_bytes!("../../../assets/Hack-Regular.ttf"));

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Metrics {
    pub(crate) cell: Extent,
    pub(crate) ascent: Px,
}

pub(crate) struct Raster {
    pub(crate) width: Texel,
    pub(crate) height: Texel,
    pub(crate) offset: Point,
    pub(crate) bitmap: Bitmap,
}

pub(crate) struct Font {
    face: fontdue::Font,
    metrics: HashMap<FontSize, Metrics>,
}

impl Font {
    pub(crate) fn load() -> Result<Self, StartError> {
        let face = fontdue::Font::from_bytes(BUNDLED.0, FontSettings::default())
            .map_err(|reason| StartError::Font(Reason::new(reason)))?;
        Ok(Self {
            face,
            metrics: HashMap::new(),
        })
    }

    pub(crate) fn metrics(&mut self, size: FontSize) -> Metrics {
        if let Some(known) = self.metrics.get(&size) {
            return *known;
        }
        let scale = size.float();
        let line = self.face.horizontal_line_metrics(scale);
        let cell_width = self.face.metrics('M', scale).advance_width.round().max(1.0);
        let ascent = line.map_or(0.0, |line| line.ascent.round());
        let row_height = line.map_or(1.0, |line| {
            (line.ascent - line.descent + line.line_gap)
                .round()
                .max(1.0)
        });
        let metrics = Metrics {
            cell: Extent::new(
                Coordinate::new(cell_width).truncate(),
                Coordinate::new(row_height).truncate(),
            ),
            ascent: Coordinate::new(ascent).truncate(),
        };
        self.metrics.insert(size, metrics);
        metrics
    }

    pub(crate) fn rasterize(&self, size: FontSize, glyph: Glyph) -> Raster {
        let (metrics, bitmap) = self.face.rasterize(glyph.get(), size.float());
        let height = Px::of_count(metrics.height);
        Raster {
            width: Texel::of_count(metrics.width),
            height: Texel::of_count(metrics.height),
            offset: Point::new(Px::new(metrics.xmin), -(Px::new(metrics.ymin) + height)),
            bitmap: Bitmap::new(bitmap),
        }
    }
}
