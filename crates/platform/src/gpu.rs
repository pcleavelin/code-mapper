use std::borrow::Cow;
use std::mem;
use std::pin;
use std::slice;
use std::task::{Context, Poll, Waker};

use ui::{Color, Vector};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, Device, Extent3d, FragmentState,
    MultisampleState, PipelineCompilationOptions, PipelineLayoutDescriptor, PrimitiveState,
    RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType, ShaderModuleDescriptor,
    ShaderSource, ShaderStages, Texture, TextureDescriptor, TextureDimension, TextureFormat,
    TextureSampleType, TextureUsages, TextureViewDimension, VertexAttribute, VertexBufferLayout,
    VertexFormat, VertexState, VertexStepMode,
};

const SHADER: Shader = Shader(
    r"
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
",
);

#[derive(Clone, Copy, Debug)]
struct Shader(&'static str);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct Vertex {
    pub(crate) position: Vector,
    pub(crate) texture: Vector,
    pub(crate) color: Color,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Bytes<'data>(&'data [u8]);

impl<'data> Bytes<'data> {
    pub(crate) const fn as_slice(self) -> &'data [u8] {
        self.0
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    pub(crate) const fn length(self) -> usize {
        self.0.len()
    }
}

#[expect(
    unsafe_code,
    reason = "vertices, indices and uniforms are plain Copy data, read as bytes for the GPU upload"
)]
pub(crate) fn bytes_of<Value: Copy>(values: &[Value]) -> Bytes<'_> {
    Bytes(unsafe { slice::from_raw_parts(values.as_ptr().cast::<u8>(), mem::size_of_val(values)) })
}

pub(crate) fn block_on<Work: Future>(work: Work) -> Work::Output {
    let mut work = pin::pin!(work);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = work.as_mut().poll(&mut context) {
            return value;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BufferName(&'static str);

impl BufferName {
    pub(crate) const VERTICES: Self = Self("verts");
    pub(crate) const INDICES: Self = Self("idx");
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) struct ByteCount(usize);

impl ByteCount {
    const SMALLEST: Self = Self(1 << 16);

    pub(crate) const fn of(bytes: Bytes<'_>) -> Self {
        Self(bytes.length())
    }

    fn wide(self) -> u64 {
        u64::try_from(self.0).unwrap_or(0)
    }
}

pub(crate) struct GpuBuffer {
    pub(crate) buffer: Buffer,
    capacity: ByteCount,
}

impl GpuBuffer {
    pub(crate) fn fit(
        device: &Device,
        slot: &mut Option<Self>,
        needed: ByteCount,
        name: BufferName,
        usage: BufferUsages,
    ) {
        if slot
            .as_ref()
            .is_none_or(|current| current.capacity < needed)
        {
            let capacity = ByteCount(needed.max(ByteCount::SMALLEST).0.next_power_of_two());
            let buffer = device.create_buffer(&BufferDescriptor {
                label: Some(name.0),
                size: capacity.wide(),
                usage: usage | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            *slot = Some(Self { buffer, capacity });
        }
    }
}

pub(crate) struct Pipeline {
    pub(crate) bind_layout: BindGroupLayout,
    pub(crate) pipeline: RenderPipeline,
}

fn bind_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

impl Pipeline {
    pub(crate) fn new(device: &Device, format: TextureFormat) -> Self {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ui"),
            source: ShaderSource::Wgsl(Cow::Borrowed(SHADER.0)),
        });
        let bind_layout = bind_layout(device);
        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        let stride = u64::try_from(mem::size_of::<Vertex>()).unwrap_or(0);
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[VertexBufferLayout {
                    array_stride: stride,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        VertexAttribute {
                            format: VertexFormat::Unorm8x4,
                            offset: 16,
                            shader_location: 2,
                        },
                    ],
                }],
                compilation_options: PipelineCompilationOptions::default(),
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });
        Self {
            bind_layout,
            pipeline,
        }
    }
}

pub(crate) fn atlas_texture(device: &Device, side: Extent3d) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("atlas"),
        size: side,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::R8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

pub(crate) fn bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    uniforms: &Buffer,
    texture: &Texture,
    sampler: &Sampler,
) -> BindGroup {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&view),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    })
}
