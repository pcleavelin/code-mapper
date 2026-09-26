use crate::ui::Measure;
use std::collections::HashMap;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use winit::window::Window;

pub type Color = [u8; 4];

pub struct Glyphs {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<(char, Color)>,
}

impl Glyphs {
    pub fn new(w: usize, h: usize) -> Glyphs {
        Glyphs {
            w,
            h,
            cells: vec![('\0', [0; 4]); w * h],
        }
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
        Rect {
            x,
            y,
            w: (r - x).max(0),
            h: (b - y).max(0),
        }
    }
    pub fn shrink(&self, n: i32) -> Rect {
        Rect {
            x: self.x + n,
            y: self.y + n,
            w: (self.w - 2 * n).max(0),
            h: (self.h - 2 * n).max(0),
        }
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

#[derive(Clone, Copy)]
struct Glyph {
    u: u32,
    v: u32,
    w: u32,
    h: u32,
    dx: i32,
    dy: i32,
}

#[derive(Clone, Copy)]
pub struct FontMetrics {
    pub cell_w: i32,
    pub row_h: i32,
    pub ascent: i32,
}

struct Atlas {
    size: u32,
    pixels: Vec<u8>,
    row_x: u32,
    row_y: u32,
    row_h: u32,
    dirty: bool,
    glyphs: HashMap<(u32, char), Option<Glyph>>,
}

impl Atlas {
    fn new(size: u32) -> Atlas {
        let mut a = Atlas {
            size,
            pixels: vec![0; (size * size) as usize],
            row_x: 0,
            row_y: 0,
            row_h: 0,
            dirty: true,
            glyphs: HashMap::new(),
        };
        for y in 0..4 {
            for x in 0..4 {
                a.pixels[(y * size + x) as usize] = 255;
            }
        }
        a.row_x = 5;
        a.row_h = 5;
        a
    }

    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let (w, h) = (w + 1, h + 1);
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

struct Cmd {
    clip: Rect,
    start: u32,
    end: u32,
}

pub struct Gfx {
    pub(crate) window: Arc<Window>,
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

#[expect(
    unsafe_code,
    reason = "vertices and uniforms are plain Copy data, read as bytes for the GPU upload"
)]
fn bytes_of<T: Copy>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

fn block_on<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
}

fn fit_buffer(
    device: &wgpu::Device,
    buf: &mut Option<(wgpu::Buffer, usize)>,
    len: usize,
    label: &str,
    usage: wgpu::BufferUsages,
) {
    if buf.as_ref().is_none_or(|(_, cap)| *cap < len) {
        let cap = len.max(1 << 16).next_power_of_two();
        let b = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: cap as u64,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *buf = Some((b, cap));
    }
}

impl Gfx {
    pub(crate) fn new(window: Arc<Window>) -> Gfx {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("no GPU adapter");
        let (device, queue) =
            block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface config");
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().find(|f| {
            matches!(
                f,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
            )
        }) {
            config.format = *f;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.usage |= wgpu::TextureUsages::COPY_SRC;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
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
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Unorm8x4,
                            offset: 16,
                            shader_location: 2,
                        },
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
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let atlas = Atlas::new(1024);
        let texture = Self::make_texture(&device, atlas.size);
        let bind_group =
            Self::make_bind_group(&device, &bind_layout, &uniforms, &texture, &sampler);
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
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn make_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniforms: &wgpu::Buffer,
        texture: &wgpu::Texture,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        let view = texture.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub(crate) fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.size = (w as i32, h as i32);
    }

    pub fn font(&mut self, px: u32) -> FontMetrics {
        if let Some(m) = self.metrics.get(&px) {
            return *m;
        }
        let lm = self
            .font
            .horizontal_line_metrics(px as f32)
            .expect("line metrics");
        let cell_w = self
            .font
            .metrics('M', px as f32)
            .advance_width
            .round()
            .max(1.0) as i32;
        let ascent = lm.ascent.round() as i32;
        let row_h = (lm.ascent - lm.descent + lm.line_gap).round().max(1.0) as i32;
        let m = FontMetrics {
            cell_w,
            row_h,
            ascent,
        };
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
        let g = Glyph {
            u: at.0,
            v: at.1,
            w,
            h,
            dx: m.xmin,
            dy: -(m.ymin + m.height as i32),
        };
        self.atlas.glyphs.insert((px, c), Some(g));
        Some(g)
    }

    fn grow_atlas(&mut self) {
        let size = (self.atlas.size * 2).min(8192);
        self.atlas = Atlas::new(size);
        self.texture = Self::make_texture(&self.device, size);
        self.bind_group = Self::make_bind_group(
            &self.device,
            &self.bind_layout,
            &self.uniforms,
            &self.texture,
            &self.sampler,
        );
    }

    pub(crate) fn begin(&mut self) {
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

    fn cut(&mut self) {
        let n = self.idx.len() as u32;
        let clip = self.clip();
        match self.cmds.last_mut() {
            Some(c) if c.start == c.end => c.clip = clip,
            _ => self.cmds.push(Cmd {
                clip,
                start: n,
                end: n,
            }),
        }
    }

    fn extend_cmd(&mut self) {
        if self.cmds.is_empty() {
            self.cut();
        }
        self.cmds.last_mut().unwrap().end = self.idx.len() as u32;
    }

    fn quad(&mut self, p: [[f32; 2]; 4], uv: [f32; 4], color: Color) {
        let n = self.verts.len() as u32;
        self.verts.push(Vertex {
            pos: p[0],
            uv: [uv[0], uv[1]],
            color,
        });
        self.verts.push(Vertex {
            pos: p[1],
            uv: [uv[2], uv[1]],
            color,
        });
        self.verts.push(Vertex {
            pos: p[2],
            uv: [uv[2], uv[3]],
            color,
        });
        self.verts.push(Vertex {
            pos: p[3],
            uv: [uv[0], uv[3]],
            color,
        });
        self.idx
            .extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
        self.extend_cmd();
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
        let (x0, y0, x1, y1) = (r.x as f32, r.y as f32, r.right() as f32, r.bottom() as f32);
        self.quad([[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv, color);
    }

    pub fn rect_outline(&mut self, r: Rect, width: i32, color: Color) {
        self.rect(Rect::new(r.x, r.y, r.w, width), color);
        self.rect(Rect::new(r.x, r.bottom() - width, r.w, width), color);
        self.rect(Rect::new(r.x, r.y, width, r.h), color);
        self.rect(Rect::new(r.right() - width, r.y, width, r.h), color);
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, width: f32, color: Color) {
        let (dx, dy) = (x1 - x0, y1 - y0);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.01 {
            return;
        }
        let (nx, ny) = (-dy / len * width / 2.0, dx / len * width / 2.0);
        let uv = self.white();
        self.quad(
            [
                [x0 + nx, y0 + ny],
                [x1 + nx, y1 + ny],
                [x1 - nx, y1 - ny],
                [x0 - nx, y0 - ny],
            ],
            uv,
            color,
        );
    }

    pub fn curve(&mut self, p: [(f32, f32); 4], width: f32, color: Color) {
        let n = 24;
        let mut prev = p[0];
        for i in 1..=n {
            let t = i as f32 / n as f32;
            let (a, b, c, d) = (
                (1.0 - t).powi(3),
                3.0 * t * (1.0 - t).powi(2),
                3.0 * t * t * (1.0 - t),
                t.powi(3),
            );
            let q = (
                a * p[0].0 + b * p[1].0 + c * p[2].0 + d * p[3].0,
                a * p[0].1 + b * p[1].1 + c * p[2].1 + d * p[3].1,
            );
            self.line(prev.0, prev.1, q.0, q.1, width, color);
            prev = q;
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, color: Color) {
        let n = 12;
        let uv = self.white();
        let base = self.verts.len() as u32;
        self.verts.push(Vertex {
            pos: [cx, cy],
            uv: [uv[0], uv[1]],
            color,
        });
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            self.verts.push(Vertex {
                pos: [cx + r * a.cos(), cy + r * a.sin()],
                uv: [uv[0], uv[1]],
                color,
            });
        }
        for i in 0..n {
            self.idx
                .extend_from_slice(&[base, base + 1 + i, base + 1 + (i + 1) % n]);
        }
        self.extend_cmd();
    }

    fn put(&mut self, px: u32, c: char, pen: i32, baseline: i32, color: Color) {
        if let Some(g) = self.glyph(px, c) {
            let s = self.atlas.size as f32;
            let (x0, y0) = ((pen + g.dx) as f32, (baseline + g.dy) as f32);
            let (x1, y1) = (x0 + g.w as f32, y0 + g.h as f32);
            let uv = [
                g.u as f32 / s,
                g.v as f32 / s,
                (g.u + g.w) as f32 / s,
                (g.v + g.h) as f32 / s,
            ];
            self.quad([[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv, color);
        }
    }

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

    pub(crate) fn render(&mut self, clear: Color) {
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
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &self.atlas.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.atlas.size),
                    rows_per_image: Some(self.atlas.size),
                },
                wgpu::Extent3d {
                    width: self.atlas.size,
                    height: self.atlas.size,
                    depth_or_array_layers: 1,
                },
            );
            self.atlas.dirty = false;
        }
        let screen = [self.size.0 as f32, self.size.1 as f32, 0.0, 0.0];
        self.queue
            .write_buffer(&self.uniforms, 0, bytes_of(&screen));
        let vbytes = bytes_of(&self.verts);
        fit_buffer(
            &self.device,
            &mut self.vbuf,
            vbytes.len(),
            "verts",
            wgpu::BufferUsages::VERTEX,
        );
        let ibytes = bytes_of(&self.idx);
        fit_buffer(
            &self.device,
            &mut self.ibuf,
            ibytes.len(),
            "idx",
            wgpu::BufferUsages::INDEX,
        );
        if !vbytes.is_empty() {
            self.queue
                .write_buffer(&self.vbuf.as_ref().unwrap().0, 0, vbytes);
            self.queue
                .write_buffer(&self.ibuf.as_ref().unwrap().0, 0, ibytes);
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
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: c(clear[0]),
                            g: c(clear[1]),
                            b: c(clear[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
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
                pass.set_index_buffer(
                    self.ibuf.as_ref().unwrap().0.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
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

    fn read_back(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        tex: &wgpu::Texture,
        path: &std::path::Path,
    ) {
        let (w, h) = (self.config.width, self.config.height);
        let row = (w * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shot"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        let done = std::mem::replace(
            encoder,
            self.device.create_command_encoder(&Default::default()),
        )
        .finish();
        self.queue.submit([done]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        let data = slice.get_mapped_range();
        let bgra = matches!(
            self.config.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let r = &data[(y * row) as usize..(y * row + w * 4) as usize];
            for p in r.chunks(4) {
                if bgra {
                    px.extend_from_slice(&[p[2], p[1], p[0], 255])
                } else {
                    px.extend_from_slice(&[p[0], p[1], p[2], 255])
                }
            }
        }
        drop(data);
        buf.unmap();
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            let mut enc =
                png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(path)?), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut out = enc.write_header()?;
            out.write_image_data(&px)?;
            out.finish()?;
            Ok(())
        };
        match write() {
            Ok(()) => eprintln!("screenshot: {}", path.display()),
            Err(e) => eprintln!("screenshot failed: {e}"),
        }
    }
}

impl Measure for Gfx {
    fn cell(&mut self, px: u32) -> (i32, i32) {
        let m = self.font(px);
        (m.cell_w, m.row_h)
    }
}
