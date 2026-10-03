use std::collections::HashMap;

use fontdue::FontSettings;
use io_fonts::FontFile;
use ui::{Coordinate, Extent, FontSize, Glyph, Point, Px};

use crate::atlas::{Bitmap, Texel};
use crate::error::{Reason, StartError};

#[derive(Clone, Copy, Debug)]
struct Face<'bytes>(&'bytes [u8]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BundledFamily(&'static str);

impl BundledFamily {
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

const BUNDLED: Face<'static> = Face(include_bytes!("../../../assets/Hack-Regular.ttf"));
pub const BUNDLED_FAMILY: BundledFamily = BundledFamily("Hack");
const ICONS: Face<'static> = Face(include_bytes!("../../../assets/codicon.ttf"));

#[derive(Clone, Copy, Debug)]
struct Fill(f32);

const ICON_FILL: Fill = Fill(0.9);

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
    icons: fontdue::Font,
    metrics: HashMap<FontSize, Metrics>,
}

impl Font {
    pub(crate) fn load() -> Result<Self, StartError> {
        Self::bundled().map_err(StartError::Font)
    }

    pub(crate) fn bundled() -> Result<Self, Reason> {
        Self::of_face(BUNDLED, FontSettings::default())
    }

    pub(crate) fn of_file(file: &FontFile) -> Result<Self, Reason> {
        Self::of_face(
            Face(file.bytes().as_slice()),
            FontSettings {
                collection_index: file.index().get(),
                ..FontSettings::default()
            },
        )
    }

    fn of_face(face: Face<'_>, settings: FontSettings) -> Result<Self, Reason> {
        let parse = |embedded: Face<'_>, chosen: FontSettings| {
            fontdue::Font::from_bytes(embedded.0, chosen).map_err(Reason::new)
        };
        Ok(Self {
            face: parse(face, settings)?,
            icons: parse(ICONS, FontSettings::default())?,
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

    pub(crate) fn rasterize(&mut self, size: FontSize, glyph: Glyph) -> Raster {
        if glyph.is_icon() {
            return self.icon(size, glyph);
        }
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

impl Font {
    fn icon(&mut self, size: FontSize, glyph: Glyph) -> Raster {
        let font = self.metrics(size);
        let columns = Px::of_count(glyph.columns());
        let across = Px::new(font.cell.width.get() * columns.get());
        let square = across.min(font.cell.height);
        let (metrics, bitmap) = self
            .icons
            .rasterize(glyph.get(), square.float() * ICON_FILL.0);
        let width = Px::of_count(metrics.width);
        let height = Px::of_count(metrics.height);
        Raster {
            width: Texel::of_count(metrics.width),
            height: Texel::of_count(metrics.height),
            offset: Point::new(
                (across - width) / 2,
                (font.cell.height - height) / 2 - font.ascent,
            ),
            bitmap: Bitmap::new(bitmap),
        }
    }
}
