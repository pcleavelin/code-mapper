use std::f32::consts::TAU;
use std::path::PathBuf;
use std::sync::Arc;

use io_fonts::FontFile;
use ui::{
    Color, Command, Coordinate, Count, DrawList, Extent, FontSize, Glyph, Grid, Label, Measure,
    Point, Px, Rect, Repaint, Scale, Vector,
};
use wgpu::{
    BindGroup, Buffer, BufferDescriptor, BufferUsages, Device, FilterMode, Instance, PresentMode,
    Queue, RequestAdapterOptions, Sampler, SamplerDescriptor, Surface, SurfaceConfiguration,
    Texture, TextureFormat, TextureUsages,
};
use winit::window::Window;

use crate::atlas::{Atlas, Entry, Sprite, SpriteKey, Texel, TextureArea};
use crate::error::{Reason, StartError};
use crate::font::{Font, Metrics};
use crate::gpu::{GpuBuffer, Pipeline, Vertex, atlas_texture, bind_group, block_on};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub(crate) struct IndexSlot(u32);

impl IndexSlot {
    fn of_count(count: usize) -> Self {
        Self(u32::try_from(count).unwrap_or(0))
    }

    pub(crate) const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Indices(Vec<u32>);

impl Indices {
    pub(crate) fn as_slice(&self) -> &[u32] {
        &self.0
    }

    fn count(&self) -> IndexSlot {
        IndexSlot::of_count(self.0.len())
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    fn quad(&mut self, base: IndexSlot) {
        let first = base.0;
        self.0
            .extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
    }

    fn fan(&mut self, base: IndexSlot, points: u32) {
        let center = base.0;
        for point in 0..points {
            self.0.extend_from_slice(&[
                center,
                center + 1 + point,
                center + 1 + (point + 1) % points,
            ]);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Batch {
    pub(crate) clip: Rect,
    pub(crate) start: IndexSlot,
    pub(crate) end: IndexSlot,
}

pub struct Renderer {
    pub(crate) window: Arc<Window>,
    pub(crate) surface: Surface<'static>,
    pub(crate) device: Device,
    pub(crate) queue: Queue,
    pub(crate) config: SurfaceConfiguration,
    pub(crate) pipeline: Pipeline,
    pub(crate) bind_group: BindGroup,
    sampler: Sampler,
    pub(crate) uniforms: Buffer,
    pub(crate) texture: Texture,
    pub(crate) vertex_buffer: Option<GpuBuffer>,
    pub(crate) index_buffer: Option<GpuBuffer>,
    font: Font,
    pub(crate) atlas: Atlas,
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Indices,
    pub(crate) batches: Vec<Batch>,
    clip: Rect,
    pub(crate) size: Extent,
    pub(crate) scale: Scale,
    pub(crate) shot: Option<PathBuf>,
    pub(crate) repaint: Repaint,
}

impl Renderer {
    pub(crate) fn new(window: Arc<Window>) -> Result<Self, StartError> {
        let instance = Instance::default();
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(StartError::Surface)?;
        let adapter = block_on(instance.request_adapter(&RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..RequestAdapterOptions::default()
        }))
        .map_err(StartError::Adapter)?;
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(StartError::Device)?;
        let inner = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, inner.width.max(1), inner.height.max(1))
            .ok_or(StartError::SurfaceConfiguration)?;
        let capabilities = surface.get_capabilities(&adapter);
        if let Some(format) = capabilities.formats.iter().find(|format| {
            matches!(
                format,
                TextureFormat::Bgra8Unorm | TextureFormat::Rgba8Unorm
            )
        }) {
            config.format = *format;
        }
        config.present_mode = PresentMode::AutoVsync;
        config.usage |= TextureUsages::COPY_SRC;
        surface.configure(&device, &config);
        let pipeline = Pipeline::new(&device, config.format);
        let sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            ..SamplerDescriptor::default()
        });
        let uniforms = device.create_buffer(&BufferDescriptor {
            label: Some("uniforms"),
            size: 16,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let atlas = Atlas::new(Atlas::FIRST_SIDE);
        let texture = atlas_texture(&device, atlas.side().square());
        let group = bind_group(
            &device,
            &pipeline.bind_layout,
            &uniforms,
            &texture,
            &sampler,
        );
        let font = Font::load()?;
        let scale = Scale::new(window.scale_factor());
        let size = Extent::new(
            Px::new(i32::try_from(inner.width).unwrap_or(0)),
            Px::new(i32::try_from(inner.height).unwrap_or(0)),
        );
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group: group,
            sampler,
            uniforms,
            texture,
            vertex_buffer: None,
            index_buffer: None,
            font,
            atlas,
            vertices: Vec::new(),
            indices: Indices::default(),
            batches: Vec::new(),
            clip: Rect::at(Point::default(), size),
            size,
            scale,
            shot: None,
            repaint: Repaint::NONE,
        })
    }

    pub const fn scale(&self) -> Scale {
        self.scale
    }

    pub const fn size(&self) -> Extent {
        self.size
    }

    pub fn shoot(&mut self, path: PathBuf) {
        self.shot = Some(path);
    }

    pub fn use_font_file(&mut self, file: Option<&FontFile>) -> Result<(), Reason> {
        self.font = match file {
            Some(file) => Font::of_file(file)?,
            None => Font::bundled()?,
        };
        self.replace_atlas(Atlas::FIRST_SIDE);
        Ok(())
    }

    pub const fn use_repaint(&mut self, repaint: Repaint) {
        self.repaint = repaint;
    }

    pub(crate) fn resize(&mut self, size: Extent) {
        if size.width.unsigned() == 0 || size.height.unsigned() == 0 {
            return;
        }
        self.config.width = size.width.unsigned();
        self.config.height = size.height.unsigned();
        self.surface.configure(&self.device, &self.config);
        self.size = size;
    }

    fn metrics(&mut self, size: FontSize) -> Metrics {
        self.font.metrics(size)
    }

    fn sprite(&mut self, size: FontSize, glyph: Glyph) -> Option<Sprite> {
        let key = SpriteKey { size, glyph };
        match self.atlas.cached(key) {
            Some(Entry::Blank) => return None,
            Some(Entry::Placed(sprite)) => return Some(sprite),
            None => {}
        }
        loop {
            let raster = self.font.rasterize(size, glyph);
            if raster.width == Texel::ZERO || raster.height == Texel::ZERO {
                self.atlas.remember(key, Entry::Blank);
                return None;
            }
            if let Some(corner) = self.atlas.allocate(raster.width, raster.height) {
                self.atlas.blit(corner, raster.width, &raster.bitmap);
                let sprite = Sprite {
                    corner,
                    width: raster.width,
                    height: raster.height,
                    offset: raster.offset,
                };
                self.atlas.remember(key, Entry::Placed(sprite));
                return Some(sprite);
            }
            self.grow_atlas();
        }
    }

    fn grow_atlas(&mut self) {
        self.replace_atlas(self.atlas.grown_side());
    }

    fn replace_atlas(&mut self, side: Texel) {
        self.atlas = Atlas::new(side);
        self.texture = atlas_texture(&self.device, self.atlas.side().square());
        self.bind_group = bind_group(
            &self.device,
            &self.pipeline.bind_layout,
            &self.uniforms,
            &self.texture,
            &self.sampler,
        );
    }

    fn begin(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.batches.clear();
        self.clip = Rect::at(Point::default(), self.size);
    }

    fn cut(&mut self) {
        let count = self.indices.count();
        let clip = self.clip;
        match self.batches.last_mut() {
            Some(batch) if batch.start == batch.end => batch.clip = clip,
            _ => self.batches.push(Batch {
                clip,
                start: count,
                end: count,
            }),
        }
    }

    fn extend_batch(&mut self) {
        if self.batches.is_empty() {
            self.cut();
        }
        let count = self.indices.count();
        if let Some(batch) = self.batches.last_mut() {
            batch.end = count;
        }
    }

    fn quad(&mut self, corners: [Vector; 4], area: TextureArea, color: Color) {
        let base = IndexSlot::of_count(self.vertices.len());
        let [first, second, third, fourth] = corners;
        let start = area.start;
        let end = area.end;
        for (position, texture) in [
            (first, start),
            (second, Vector::new(end.horizontal, start.vertical)),
            (third, end),
            (fourth, Vector::new(start.horizontal, end.vertical)),
        ] {
            self.vertices.push(Vertex {
                position,
                texture,
                color: self.repaint.paint(color),
            });
        }
        self.indices.quad(base);
        self.extend_batch();
    }

    const fn corners(start: Vector, end: Vector) -> [Vector; 4] {
        [
            start,
            Vector::new(end.horizontal, start.vertical),
            end,
            Vector::new(start.horizontal, end.vertical),
        ]
    }

    fn fill(&mut self, rect: Rect, color: Color) {
        if rect.is_empty() || color.is_invisible() {
            return;
        }
        let area = self.atlas.white();
        let corners = Self::corners(
            Vector::of_point(rect.origin()),
            Vector::of_point(Point::new(rect.right(), rect.bottom())),
        );
        self.quad(corners, area, color);
    }

    fn circle(&mut self, center: Vector, radius: Coordinate, color: Color) {
        let points: u8 = 12;
        let area = self.atlas.white();
        let base = IndexSlot::of_count(self.vertices.len());
        let texture = area.start;
        let color = self.repaint.paint(color);
        self.vertices.push(Vertex {
            position: center,
            texture,
            color,
        });
        let radius = radius.get();
        for point in 0..points {
            let angle = f32::from(point) / f32::from(points) * TAU;
            self.vertices.push(Vertex {
                position: Vector::new(
                    Coordinate::new(center.horizontal.get() + radius * angle.cos()),
                    Coordinate::new(center.vertical.get() + radius * angle.sin()),
                ),
                texture,
                color,
            });
        }
        self.indices.fan(base, u32::from(points));
        self.extend_batch();
    }

    fn put(&mut self, size: FontSize, glyph: Glyph, pen: Px, baseline: Px, color: Color) {
        if let Some(sprite) = self.sprite(size, glyph) {
            let start = Vector::of_point(Point::new(
                pen + sprite.offset.horizontal,
                baseline + sprite.offset.vertical,
            ));
            let end = Vector::new(
                Coordinate::new(start.horizontal.get() + sprite.width.float()),
                Coordinate::new(start.vertical.get() + sprite.height.float()),
            );
            let area = self.atlas.area(sprite);
            self.quad(Self::corners(start, end), area, color);
        }
    }

    fn text(&mut self, at: Point, size: FontSize, text: &Label, color: Color) {
        let metrics = self.metrics(size);
        let clip = self.clip;
        let mut pen = at.horizontal;
        let baseline = at.vertical + metrics.ascent;
        for character in text.as_str().chars() {
            if pen >= clip.right() {
                break;
            }
            let glyph = Glyph::new(character);
            if pen + metrics.cell.width > clip.left && glyph != Glyph::SPACE {
                self.put(size, glyph, pen, baseline, color);
            }
            pen += metrics.cell.width * Count::new(glyph.columns());
        }
    }

    fn grid(&mut self, at: Point, size: FontSize, grid: &Grid) {
        let metrics = self.metrics(size);
        let clip = self.clip;
        let mut row_top = at.vertical;
        for row in 0..grid.rows().get() {
            let top = row_top;
            row_top += metrics.cell.height;
            if top >= clip.bottom() {
                break;
            }
            if top + metrics.cell.height <= clip.top {
                continue;
            }
            let baseline = top + metrics.ascent;
            let mut pen = at.horizontal;
            for cell in grid.row(Count::new(row)) {
                let Some(cell) = cell else { break };
                if pen >= clip.right() {
                    break;
                }
                if cell.glyph != Glyph::SPACE && pen + metrics.cell.width > clip.left {
                    self.put(size, cell.glyph, pen, baseline, cell.color);
                }
                pen += metrics.cell.width;
            }
        }
    }

    pub(crate) fn record(&mut self, drawing: &DrawList) {
        self.begin();
        for command in drawing.commands() {
            match command {
                Command::Clip(clip) => {
                    self.clip = *clip;
                    self.cut();
                }
                Command::Fill { rect, color } => self.fill(*rect, *color),
                Command::Quad { corners, color } => {
                    let area = self.atlas.white();
                    self.quad(*corners, area, *color);
                }
                Command::Circle {
                    center,
                    radius,
                    color,
                } => self.circle(*center, *radius, *color),
                Command::Text {
                    at,
                    size,
                    text,
                    color,
                } => self.text(*at, *size, text, *color),
                Command::Grid { at, size, grid } => self.grid(*at, *size, grid),
            }
        }
    }
}

impl Measure for Renderer {
    fn cell(&mut self, size: FontSize) -> Extent {
        self.metrics(size).cell
    }
}
