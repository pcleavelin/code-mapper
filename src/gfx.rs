//! The window and the GPU. One wgpu pipeline draws everything as textured, coloured quads from
//! one atlas: every glyph at every size in use, rasterised on demand, plus a white pixel for
//! solid fills. Positions are whole physical pixels, so text is never scaled or sampled between
//! pixels. Clipping is a scissor rect per draw command.

use crate::ui::{Input, Key, Measure, Mods};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WKey, NamedKey};
use winit::window::{Window, WindowId};

pub type Color = [u8; 4];

/// A block of text as it will be drawn: a grid of characters with a colour each, one cell
/// per character, row-major, '\0' past the end of a line. Built when the text or its
/// colours change and drawn every frame without allocating.
pub struct Glyphs {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<(char, Color)>,
}

impl Glyphs {
    pub fn new(w: usize, h: usize) -> Glyphs {
        Glyphs { w, h, cells: vec![('\0', [0; 4]); w * h] }
    }

    pub fn set(&mut self, x: usize, y: usize, c: char, color: Color) {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x] = (c, color);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        Rect { x, y, w: (r - x).max(0), h: (b - y).max(0) }
    }
    pub fn shrink(&self, n: i32) -> Rect {
        Rect { x: self.x + n, y: self.y + n, w: (self.w - 2 * n).max(0), h: (self.h - 2 * n).max(0) }
    }
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }
}

const FONT: &[u8] = include_bytes!("../assets/Hack-Regular.ttf");

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: Color,
}

/// A glyph in the atlas: where its bitmap sits and how it hangs off the pen.
#[derive(Clone, Copy)]
struct Glyph {
    u: u32,
    v: u32,
    w: u32,
    h: u32,
    dx: i32, // bitmap left relative to the pen x
    dy: i32, // bitmap top relative to the baseline
}

/// Cell of a monospace font at one pixel size.
#[derive(Clone, Copy)]
pub struct FontMetrics {
    pub cell_w: i32,
    pub row_h: i32,
    pub ascent: i32,
}

struct Atlas {
    size: u32,
    pixels: Vec<u8>, // one coverage byte per texel
    row_x: u32,
    row_y: u32,
    row_h: u32,
    dirty: bool,
    glyphs: HashMap<(u32, char), Option<Glyph>>,
}

impl Atlas {
    fn new(size: u32) -> Atlas {
        let mut a = Atlas { size, pixels: vec![0; (size * size) as usize], row_x: 0, row_y: 0, row_h: 0, dirty: true, glyphs: HashMap::new() };
        // the white pixel for solid quads, with a margin so filtering never bleeds a neighbour
        for y in 0..4 {
            for x in 0..4 {
                a.pixels[(y * size + x) as usize] = 255;
            }
        }
        a.row_x = 5;
        a.row_h = 5;
        a
    }

    /// A slot of w x h texels, or None when the atlas is full.
    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let (w, h) = (w + 1, h + 1); // one texel of margin
        if self.row_x + w > self.size {
            self.row_y += self.row_h;
            self.row_x = 0;
            self.row_h = 0;
        }
        if self.row_y + h > self.size {
            return None;
        }
        let at = (self.row_x, self.row_y);
        self.row_x += w;
        self.row_h = self.row_h.max(h);
        Some(at)
    }
}

/// Everything drawn this frame: quads in one batch, split into commands where the clip changes.
struct Cmd {
    clip: Rect,
    start: u32, // index range of this command
    end: u32,
}

pub struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    texture: wgpu::Texture,
    vbuf: Option<(wgpu::Buffer, usize)>,
    ibuf: Option<(wgpu::Buffer, usize)>,

    font: fontdue::Font,
    metrics: HashMap<u32, FontMetrics>,
    atlas: Atlas,

    verts: Vec<Vertex>,
    idx: Vec<u32>,
    cmds: Vec<Cmd>,
    clips: Vec<Rect>,
    pub size: (i32, i32),
    pub scale: f32,
    /// Set by the app: the next frame is also written to this file as a PNG.
    pub shot: Option<std::path::PathBuf>,
}

const SHADER: &str = r#"
struct Uniforms { screen: vec4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsIn { @location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32> };
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> };

@vertex fn vs(in: VsIn) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(in.pos.x / u.screen.x * 2.0 - 1.0, 1.0 - in.pos.y / u.screen.y * 2.0, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}

@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let a = textureSample(tex, samp, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * a);
}
"#;

fn bytes_of<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

impl Gfx {
    fn new(window: Arc<Window>) -> Gfx {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })).expect("no GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
        let size = window.inner_size();
        let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).expect("surface config");
        let caps = surface.get_capabilities(&adapter);
        // colours are given in sRGB already; a non-sRGB target writes them through untouched
        if let Some(f) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *f;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.usage |= wgpu::TextureUsages::COPY_SRC; // screenshots read the frame back
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("ui"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[&bind_layout], push_constant_ranges: &[] });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8, shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Unorm8x4, offset: 16, shader_location: 2 },
                    ],
                }],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState { format: config.format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Nearest, min_filter: wgpu::FilterMode::Nearest, ..Default::default() });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor { label: Some("uniforms"), size: 16, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let atlas = Atlas::new(1024);
        let texture = Self::make_texture(&device, atlas.size);
        let bind_group = Self::make_bind_group(&device, &bind_layout, &uniforms, &texture, &sampler);
        let font = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default()).expect("font");
        let scale = window.scale_factor() as f32;
        Gfx {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_layout,
            bind_group,
            sampler,
            uniforms,
            texture,
            vbuf: None,
            ibuf: None,
            font,
            metrics: HashMap::new(),
            atlas,
            verts: Vec::new(),
            idx: Vec::new(),
            cmds: Vec::new(),
            clips: Vec::new(),
            size: (size.width as i32, size.height as i32),
            scale,
            shot: None,
        }
    }

    fn make_texture(device: &wgpu::Device, size: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn make_bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, uniforms: &wgpu::Buffer, texture: &wgpu::Texture, sampler: &wgpu::Sampler) -> wgpu::BindGroup {
        let view = texture.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    }

    fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.size = (w as i32, h as i32);
    }

    // ---- fonts ----

    /// Cell and row of the font at `px` pixels. Whole pixels, so rows and columns tile exactly.
    pub fn font(&mut self, px: u32) -> FontMetrics {
        if let Some(m) = self.metrics.get(&px) {
            return *m;
        }
        let lm = self.font.horizontal_line_metrics(px as f32).expect("line metrics");
        let cell_w = self.font.metrics('M', px as f32).advance_width.round().max(1.0) as i32;
        let ascent = lm.ascent.round() as i32;
        let row_h = (lm.ascent - lm.descent + lm.line_gap).round().max(1.0) as i32;
        let m = FontMetrics { cell_w, row_h, ascent };
        self.metrics.insert(px, m);
        m
    }

    fn glyph(&mut self, px: u32, c: char) -> Option<Glyph> {
        if let Some(g) = self.atlas.glyphs.get(&(px, c)) {
            return *g;
        }
        let (m, bitmap) = self.font.rasterize(c, px as f32);
        if m.width == 0 || m.height == 0 {
            self.atlas.glyphs.insert((px, c), None);
            return None;
        }
        let (w, h) = (m.width as u32, m.height as u32);
        let at = match self.atlas.alloc(w, h) {
            Some(at) => at,
            None => {
                self.grow_atlas();
                return self.glyph(px, c);
            }
        };
        let size = self.atlas.size;
        for y in 0..h {
            let row = &bitmap[(y * w) as usize..((y + 1) * w) as usize];
            let off = ((at.1 + y) * size + at.0) as usize;
            self.atlas.pixels[off..off + w as usize].copy_from_slice(row);
        }
        self.atlas.dirty = true;
        let g = Glyph { u: at.0, v: at.1, w, h, dx: m.xmin, dy: -(m.ymin + m.height as i32) };
        self.atlas.glyphs.insert((px, c), Some(g));
        Some(g)
    }

    /// Twice the atlas, every glyph rasterised again on demand.
    fn grow_atlas(&mut self) {
        let size = (self.atlas.size * 2).min(8192);
        if size == self.atlas.size {
            self.atlas = Atlas::new(size); // full at the cap: start over rather than stop drawing text
        } else {
            self.atlas = Atlas::new(size);
        }
        self.texture = Self::make_texture(&self.device, size);
        self.bind_group = Self::make_bind_group(&self.device, &self.bind_layout, &self.uniforms, &self.texture, &self.sampler);
    }

    // ---- drawing ----

    fn begin(&mut self) {
        self.verts.clear();
        self.idx.clear();
        self.cmds.clear();
        self.clips.clear();
        self.clips.push(Rect::new(0, 0, self.size.0, self.size.1));
    }

    pub fn push_clip(&mut self, r: Rect) {
        let r = self.clips.last().unwrap().intersect(&r);
        self.clips.push(r);
        self.cut();
    }

    pub fn pop_clip(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
            self.cut();
        }
    }

    pub fn clip(&self) -> Rect {
        *self.clips.last().unwrap()
    }

    /// Start a new command at the current clip.
    fn cut(&mut self) {
        let n = self.idx.len() as u32;
        let clip = self.clip();
        match self.cmds.last_mut() {
            Some(c) if c.start == c.end => c.clip = clip, // nothing drawn under the old clip yet
            _ => self.cmds.push(Cmd { clip, start: n, end: n }),
        }
    }

    fn quad(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, uv: [f32; 4], color: Color) {
        let n = self.verts.len() as u32;
        self.verts.push(Vertex { pos: [x0, y0], uv: [uv[0], uv[1]], color });
        self.verts.push(Vertex { pos: [x1, y0], uv: [uv[2], uv[1]], color });
        self.verts.push(Vertex { pos: [x1, y1], uv: [uv[2], uv[3]], color });
        self.verts.push(Vertex { pos: [x0, y1], uv: [uv[0], uv[3]], color });
        self.idx.extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
        if self.cmds.is_empty() {
            self.cut();
        }
        self.cmds.last_mut().unwrap().end = self.idx.len() as u32;
    }

    fn white(&self) -> [f32; 4] {
        let s = self.atlas.size as f32;
        [1.0 / s, 1.0 / s, 3.0 / s, 3.0 / s]
    }

    pub fn rect(&mut self, r: Rect, color: Color) {
        if r.is_empty() || color[3] == 0 {
            return;
        }
        let uv = self.white();
        self.quad(r.x as f32, r.y as f32, r.right() as f32, r.bottom() as f32, uv, color);
    }

    pub fn rect_outline(&mut self, r: Rect, width: i32, color: Color) {
        self.rect(Rect::new(r.x, r.y, r.w, width), color);
        self.rect(Rect::new(r.x, r.bottom() - width, r.w, width), color);
        self.rect(Rect::new(r.x, r.y, width, r.h), color);
        self.rect(Rect::new(r.right() - width, r.y, width, r.h), color);
    }

    /// A straight segment of the given thickness.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: Color) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.01 {
            return;
        }
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        let uv = self.white();
        let n = self.verts.len() as u32;
        self.verts.push(Vertex { pos: [x0 + nx, y0 + ny], uv: [uv[0], uv[1]], color });
        self.verts.push(Vertex { pos: [x1 + nx, y1 + ny], uv: [uv[2], uv[1]], color });
        self.verts.push(Vertex { pos: [x1 - nx, y1 - ny], uv: [uv[2], uv[3]], color });
        self.verts.push(Vertex { pos: [x0 - nx, y0 - ny], uv: [uv[0], uv[3]], color });
        self.idx.extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
        if self.cmds.is_empty() {
            self.cut();
        }
        self.cmds.last_mut().unwrap().end = self.idx.len() as u32;
    }

    /// A cubic bezier as a polyline.
    pub fn curve(&mut self, p: [(f32, f32); 4], width: f32, color: Color) {
        let n = 24;
        let mut prev = p[0];
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let (a, b, c, d) = ((1.0 - t).powi(3), 3.0 * t * (1.0 - t).powi(2), 3.0 * t * t * (1.0 - t), t.powi(3));
            let q = (a * p[0].0 + b * p[1].0 + c * p[2].0 + d * p[3].0, a * p[0].1 + b * p[1].1 + c * p[2].1 + d * p[3].1);
            self.line(prev.0, prev.1, q.0, q.1, width, color);
            prev = q;
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, color: Color) {
        let n = 12;
        let uv = self.white();
        let base = self.verts.len() as u32;
        self.verts.push(Vertex { pos: [cx, cy], uv: [uv[0], uv[1]], color });
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            self.verts.push(Vertex { pos: [cx + r * a.cos(), cy + r * a.sin()], uv: [uv[0], uv[1]], color });
        }
        for i in 0..n {
            self.idx.extend_from_slice(&[base, base + 1 + i, base + 1 + (i + 1) % n]);
        }
        if self.cmds.is_empty() {
            self.cut();
        }
        self.cmds.last_mut().unwrap().end = self.idx.len() as u32;
    }

    /// One character from the atlas at a pen position on a baseline.
    fn put(&mut self, px: u32, c: char, pen: i32, baseline: i32, color: Color) {
        if let Some(g) = self.glyph(px, c) {
            let s = self.atlas.size as f32;
            let (gx, gy) = ((pen + g.dx) as f32, (baseline + g.dy) as f32);
            let uv = [g.u as f32 / s, g.v as f32 / s, (g.u + g.w) as f32 / s, (g.v + g.h) as f32 / s];
            self.quad(gx, gy, gx + g.w as f32, gy + g.h as f32, uv, color);
        }
    }

    /// Text at `px` with its top-left at (x, y), one cell per character. Returns the pen x.
    pub fn text(&mut self, x: i32, y: i32, px: u32, text: &str, color: Color) -> i32 {
        let m = self.font(px);
        let clip = self.clip();
        let mut pen = x;
        let baseline = y + m.ascent;
        for c in text.chars() {
            if pen >= clip.right() {
                break;
            }
            if pen + m.cell_w > clip.x && c != ' ' {
                self.put(px, c, pen, baseline, color);
            }
            pen += m.cell_w;
        }
        pen
    }

    /// A grid at `px` with its top-left at (x, y): rows `row_h` apart, columns `cell_w`.
    /// Rows and cells outside the clip are skipped, nothing is allocated.
    pub fn glyphs(&mut self, x: i32, y: i32, px: u32, g: &Glyphs) {
        let m = self.font(px);
        let clip = self.clip();
        for row in 0..g.h {
            let ty = y + row as i32 * m.row_h;
            if ty >= clip.bottom() {
                break;
            }
            if ty + m.row_h <= clip.y {
                continue;
            }
            let baseline = ty + m.ascent;
            let mut pen = x;
            for i in row * g.w..(row + 1) * g.w {
                let (c, color) = g.cells[i];
                if c == '\0' || pen >= clip.right() {
                    break;
                }
                if c != ' ' && pen + m.cell_w > clip.x {
                    self.put(px, c, pen, baseline, color);
                }
                pen += m.cell_w;
            }
        }
    }

    fn render(&mut self, clear: Color) {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(_) => return,
        };
        if self.atlas.dirty {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &self.atlas.pixels,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.atlas.size), rows_per_image: Some(self.atlas.size) },
                wgpu::Extent3d { width: self.atlas.size, height: self.atlas.size, depth_or_array_layers: 1 },
            );
            self.atlas.dirty = false;
        }
        let screen = [self.size.0 as f32, self.size.1 as f32, 0.0, 0.0];
        self.queue.write_buffer(&self.uniforms, 0, bytes_of(&screen));
        // vertex and index buffers grow to fit and are reused
        let vbytes = bytes_of(&self.verts);
        if self.vbuf.as_ref().is_none_or(|(_, cap)| *cap < vbytes.len()) {
            let cap = vbytes.len().max(1 << 16).next_power_of_two();
            let b = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("verts"), size: cap as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.vbuf = Some((b, cap));
        }
        let ibytes = bytes_of(&self.idx);
        if self.ibuf.as_ref().is_none_or(|(_, cap)| *cap < ibytes.len()) {
            let cap = ibytes.len().max(1 << 16).next_power_of_two();
            let b = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("idx"), size: cap as u64, usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.ibuf = Some((b, cap));
        }
        if !vbytes.is_empty() {
            self.queue.write_buffer(&self.vbuf.as_ref().unwrap().0, 0, vbytes);
            self.queue.write_buffer(&self.ibuf.as_ref().unwrap().0, 0, ibytes);
        }

        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let c = |v: u8| v as f64 / 255.0;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: c(clear[0]), g: c(clear[1]), b: c(clear[2]), a: 1.0 }), store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !self.idx.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.vbuf.as_ref().unwrap().0.slice(..));
                pass.set_index_buffer(self.ibuf.as_ref().unwrap().0.slice(..), wgpu::IndexFormat::Uint32);
                let screen = Rect::new(0, 0, self.size.0, self.size.1);
                for cmd in &self.cmds {
                    let r = cmd.clip.intersect(&screen);
                    if cmd.end > cmd.start && !r.is_empty() {
                        pass.set_scissor_rect(r.x as u32, r.y as u32, r.w as u32, r.h as u32);
                        pass.draw_indexed(cmd.start..cmd.end, 0, 0..1);
                    }
                }
            }
        }
        if let Some(path) = self.shot.take() {
            self.read_back(&mut encoder, &frame.texture, &path);
        } else {
            self.queue.submit([encoder.finish()]);
        }
        self.window.pre_present_notify();
        frame.present();
    }

    /// Copy the rendered frame to a buffer, wait for it, and write it as a PNG.
    fn read_back(&mut self, encoder: &mut wgpu::CommandEncoder, tex: &wgpu::Texture, path: &std::path::Path) {
        let (w, h) = (self.config.width, self.config.height);
        let row = (w * 4).div_ceil(256) * 256; // rows are padded to 256 bytes for the copy
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("shot"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let done = std::mem::replace(encoder, self.device.create_command_encoder(&Default::default())).finish();
        self.queue.submit([done]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        let data = slice.get_mapped_range();
        let bgra = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let r = &data[(y * row) as usize..(y * row + w * 4) as usize];
            for p in r.chunks(4) {
                if bgra { px.extend_from_slice(&[p[2], p[1], p[0], 255]) } else { px.extend_from_slice(&[p[0], p[1], p[2], 255]) }
            }
        }
        drop(data);
        buf.unmap();
        match image::RgbaImage::from_raw(w, h, px).map(|i| i.save(path)) {
            Some(Ok(())) => eprintln!("screenshot: {}", path.display()),
            other => eprintln!("screenshot failed: {other:?}"),
        }
    }

    pub fn set_title(&self, t: &str) {
        self.window.set_title(t);
    }
}

impl Measure for Gfx {
    fn cell(&mut self, px: u32) -> (i32, i32) {
        let m = self.font(px);
        (m.cell_w, m.row_h)
    }
}

// ---- the event loop ----

/// What a frame asks of the loop.
pub struct Frame {
    pub redraw_after: Option<Duration>,
    pub quit: bool,
    pub clear: Color,
}

pub trait App {
    fn frame(&mut self, gfx: &mut Gfx, input: &mut Input) -> Frame;
    /// A line of a test script the loop did not understand: the app's own commands. False
    /// means the line is not done yet and is run again next frame.
    fn script(&mut self, _line: &str) -> bool {
        true
    }
    /// The centre of the element a script names, from last frame's rectangles.
    fn locate(&mut self, _name: &str) -> Option<(i32, i32)> {
        None
    }
}

/// A test script from `CODEMAP_SCRIPT=<file>`: one command per line, fed to the app as if the
/// mouse and keyboard had done it. `wait <n>` lets n frames pass; `mouse <x> <y>`, `down`, `up`,
/// `click <x> <y>`, `dblclick <x> <y>`, `drag <x0> <y0> <x1> <y1>`, `wheel <dy> [ctrl|shift]`,
/// `down [ctrl|shift|alt] [twice]` (`twice` makes the press a double-click),
/// `key <name> [ctrl] [alt]`, `text <chars>`, `quit`; anything else goes to the app. Every
/// input command is its own frame, so the app sees it exactly as a real event.
struct Script {
    lines: Vec<String>,
    pc: usize,
    wait: u32,
}

impl Script {
    fn load() -> Option<Script> {
        let path = std::env::var_os("CODEMAP_SCRIPT")?;
        let text = std::fs::read_to_string(&path).ok()?;
        Some(Script { lines: text.lines().map(|l| l.trim().to_owned()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect(), pc: 0, wait: 0 })
    }
}

struct Runner<A: App> {
    app: A,
    title: String,
    gfx: Option<Gfx>,
    input: Input,
    mods: Mods,
    last_click: Option<(Instant, u8, (i32, i32))>,
    pending: bool, // input arrived since the last frame
    next_redraw: Option<Instant>,
    start: Instant,
    script: Option<Script>,
}

impl<A: App> Runner<A> {
    /// Run the script up to and including the next input command or wait. Returns whether to
    /// quit.
    fn step_script(&mut self) -> bool {
        let Some(sc) = self.script.as_mut() else { return false };
        if sc.wait > 0 {
            sc.wait -= 1;
            return false;
        }
        self.input.mods = self.mods;
        loop {
            let Some(line) = sc.lines.get(sc.pc).cloned() else { return false };
            sc.pc += 1;
            let w: Vec<&str> = line.split_whitespace().collect();
            let num = |i: usize| w.get(i).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
            let mods = |from: usize| Mods { ctrl: w[from.min(w.len())..].contains(&"ctrl"), shift: w[from.min(w.len())..].contains(&"shift"), alt: w[from.min(w.len())..].contains(&"alt") };
            match w[0] {
                "wait" => {
                    sc.wait = (num(1).max(1) - 1) as u32;
                    return false;
                }
                "mouse" => self.input.mouse = (num(1), num(2)),
                "down" => {
                    self.input.down[0] = true;
                    self.input.pressed[0] = true;
                    self.input.clicks[0] = if w.contains(&"twice") { 2 } else { 1 };
                    self.input.mods = mods(1);
                    return false;
                }
                "up" => {
                    self.input.down[0] = false;
                    self.input.released[0] = true;
                    return false;
                }
                "click" | "dblclick" => {
                    // expands into its own frames; the press carries the click count
                    let mut rest: String = w[3.min(w.len())..].join(" ");
                    if w[0] == "dblclick" {
                        rest.push_str(" twice");
                    }
                    let at = sc.pc;
                    sc.lines.splice(at..at, [format!("mouse {} {}", w[1], w[2]), format!("down {rest}"), "wait 1".into(), "up".into(), "wait 1".into()]);
                }
                "click-id" | "hover-id" | "dblclick-id" => {
                    // the element by name, then the plain form of the same gesture
                    let Some(name) = w.get(1) else { continue };
                    match self.app.locate(name) {
                        Some((x, y)) => {
                            let verb = match &w[0][..w[0].len() - 3] {
                                "hover" => "mouse",
                                v => v,
                            };
                            let line = format!("{verb} {x} {y} {}", w[2.min(w.len())..].join(" "));
                            let at = sc.pc;
                            sc.lines.insert(at, line);
                        }
                        None => eprintln!("script: no element '{name}' last frame"),
                    }
                }
                "drag" => {
                    let (x0, y0, x1, y1) = (num(1), num(2), num(3), num(4));
                    let n = 8;
                    let mut ins = vec![format!("mouse {x0} {y0}"), "down".into(), "wait 1".into()];
                    for i in 1..=n {
                        ins.push(format!("mouse {} {}", x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n));
                        ins.push("wait 1".into());
                    }
                    ins.push("up".into());
                    ins.push("wait 1".into());
                    let at = sc.pc;
                    sc.lines.splice(at..at, ins);
                }
                "wheel" => {
                    self.input.wheel.1 += num(1) as f32;
                    self.input.mods = mods(2);
                    return false;
                }
                "key" => {
                    let k = match w.get(1).copied().unwrap_or("") {
                        "enter" => Key::Enter,
                        "escape" => Key::Escape,
                        "backspace" => Key::Backspace,
                        "left" => Key::Left,
                        "right" => Key::Right,
                        "up" => Key::Up,
                        "down" => Key::Down,
                        other => Key::Char(other.chars().next().unwrap_or(' ')),
                    };
                    self.input.keys.push((k, mods(2)));
                    return false;
                }
                "text" => {
                    self.input.text.push_str(&w[1..].join(" "));
                    return false;
                }
                "quit" => return true,
                _ => {
                    if !self.app.script(&line) {
                        sc.pc -= 1;
                        return false;
                    }
                    if line.starts_with("shot") {
                        return false; // the screenshot is of the frame that follows, before the next command
                    }
                }
            }
        }
    }
}

impl<A: App> ApplicationHandler for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title(&self.title).with_inner_size(winit::dpi::LogicalSize::new(1600.0, 1000.0)).with_maximized(true);
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let gfx = Gfx::new(window);
        self.input.size = gfx.size;
        self.input.scale = gfx.scale;
        self.gfx = Some(gfx);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(s) => {
                gfx.resize(s.width, s.height);
                self.input.size = gfx.size;
                self.pending = true;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                gfx.scale = scale_factor as f32;
                self.input.scale = gfx.scale;
                self.pending = true;
            }
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.mods = Mods { ctrl: s.control_key(), shift: s.shift_key(), alt: s.alt_key() };
                self.input.mods = self.mods;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.mouse = (position.x as i32, position.y as i32);
                self.pending = true;
            }
            WindowEvent::CursorLeft { .. } => {
                self.input.mouse = (-1, -1);
                self.pending = true;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    MouseButton::Back => {
                        if state == ElementState::Pressed {
                            self.input.back = true;
                        }
                        self.pending = true;
                        return;
                    }
                    MouseButton::Forward => {
                        if state == ElementState::Pressed {
                            self.input.forward = true;
                        }
                        self.pending = true;
                        return;
                    }
                    _ => return,
                };
                match state {
                    ElementState::Pressed => {
                        self.input.down[b] = true;
                        self.input.pressed[b] = true;
                        let now = Instant::now();
                        let near = |a: (i32, i32), c: (i32, i32)| (a.0 - c.0).abs() < 4 && (a.1 - c.1).abs() < 4;
                        let double = matches!(self.last_click, Some((t, lb, p)) if lb == b as u8 && now.duration_since(t) < Duration::from_millis(350) && near(p, self.input.mouse));
                        self.input.clicks[b] = if double { 2 } else { 1 };
                        self.last_click = if double { None } else { Some((now, b as u8, self.input.mouse)) };
                    }
                    ElementState::Released => {
                        self.input.down[b] = false;
                        self.input.released[b] = true;
                    }
                }
                self.pending = true;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x * 40.0, y * 40.0),
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                };
                self.input.wheel.0 += x;
                self.input.wheel.1 += y;
                self.pending = true;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    let key = match &event.logical_key {
                        WKey::Named(NamedKey::Enter) => Some(Key::Enter),
                        WKey::Named(NamedKey::Escape) => Some(Key::Escape),
                        WKey::Named(NamedKey::Backspace) => Some(Key::Backspace),
                        WKey::Named(NamedKey::Delete) => Some(Key::Delete),
                        WKey::Named(NamedKey::Tab) => Some(Key::Tab),
                        WKey::Named(NamedKey::ArrowLeft) => Some(Key::Left),
                        WKey::Named(NamedKey::ArrowRight) => Some(Key::Right),
                        WKey::Named(NamedKey::ArrowUp) => Some(Key::Up),
                        WKey::Named(NamedKey::ArrowDown) => Some(Key::Down),
                        WKey::Named(NamedKey::Home) => Some(Key::Home),
                        WKey::Named(NamedKey::End) => Some(Key::End),
                        WKey::Named(NamedKey::PageUp) => Some(Key::PageUp),
                        WKey::Named(NamedKey::PageDown) => Some(Key::PageDown),
                        WKey::Character(s) => s.chars().next().map(Key::Char),
                        _ => None,
                    };
                    if let Some(k) = key {
                        self.input.keys.push((k, self.mods));
                    }
                    if let Some(t) = &event.text {
                        if !self.mods.ctrl && !self.mods.alt {
                            self.input.text.extend(t.chars().filter(|c| !c.is_control()));
                        }
                    }
                }
                self.pending = true;
            }
            WindowEvent::RedrawRequested => {
                self.input.time = self.start.elapsed().as_secs_f64();
                let scripted_quit = self.step_script();
                let gfx = self.gfx.as_mut().unwrap();
                gfx.begin();
                let out = self.app.frame(gfx, &mut self.input);
                gfx.render(out.clear);
                self.input.end_frame();
                self.pending = false;
                self.next_redraw = if self.script.is_some() { Some(Instant::now() + Duration::from_millis(8)) } else { out.redraw_after.map(|d| Instant::now() + d) };
                if out.quit || scripted_quit {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(gfx) = self.gfx.as_ref() else { return };
        let due = self.next_redraw.is_some_and(|t| Instant::now() >= t);
        if self.pending || due {
            gfx.window.request_redraw();
        }
        event_loop.set_control_flow(match self.next_redraw {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        });
    }
}

pub fn run(title: &str, app: impl App + 'static) {
    let event_loop = EventLoop::new().expect("event loop");
    let mut runner = Runner { app, title: title.to_owned(), gfx: None, input: Input::default(), mods: Mods::default(), last_click: None, pending: true, next_redraw: None, start: Instant::now(), script: Script::load() };
    event_loop.run_app(&mut runner).expect("run");
}
