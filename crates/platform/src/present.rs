use std::path::Path;

use png::{BitDepth, ColorType, Encoder, EncodingError};
use ui::{Color, DrawList, Point, Rect};
use wgpu::{
    BufferDescriptor, BufferUsages, Color as ClearColor, CommandEncoder, CommandEncoderDescriptor,
    Extent3d, IndexFormat, LoadOp, MapMode, Operations, Origin3d, PollType,
    RenderPassColorAttachment, RenderPassDescriptor, StoreOp, SurfaceError, TexelCopyBufferInfo,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureFormat,
    TextureViewDescriptor,
};

use crate::atlas::Upload;
use crate::gpu::{BufferName, ByteCount, GpuBuffer, bytes_of};
use crate::renderer::Renderer;
use crate::report::report;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Pixels(Vec<u8>);

impl Renderer {
    fn upload_atlas(&mut self) {
        if self.atlas.upload == Upload::Done {
            return;
        }
        let side = self.atlas.side();
        self.queue.write_texture(
            TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            self.atlas.pixels().as_slice(),
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side.get()),
                rows_per_image: Some(side.get()),
            },
            side.square(),
        );
        self.atlas.upload = Upload::Done;
    }

    fn upload_geometry(&mut self) {
        let screen = [
            self.size.width.float(),
            self.size.height.float(),
            0.0_f32,
            0.0,
        ];
        self.queue
            .write_buffer(&self.uniforms, 0, bytes_of(&screen).as_slice());
        let vertices = bytes_of(&self.vertices);
        GpuBuffer::fit(
            &self.device,
            &mut self.vertex_buffer,
            ByteCount::of(vertices),
            BufferName::VERTICES,
            BufferUsages::VERTEX,
        );
        let indices = bytes_of(self.indices.as_slice());
        GpuBuffer::fit(
            &self.device,
            &mut self.index_buffer,
            ByteCount::of(indices),
            BufferName::INDICES,
            BufferUsages::INDEX,
        );
        if !vertices.is_empty()
            && let (Some(vertex_buffer), Some(index_buffer)) =
                (&self.vertex_buffer, &self.index_buffer)
        {
            self.queue
                .write_buffer(&vertex_buffer.buffer, 0, vertices.as_slice());
            self.queue
                .write_buffer(&index_buffer.buffer, 0, indices.as_slice());
        }
    }

    fn encode_pass(&self, encoder: &mut CommandEncoder, target: &Texture, clear: Color) {
        let view = target.create_view(&TextureViewDescriptor::default());
        let channel = |value: u8| f64::from(value) / 255.0;
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(ClearColor {
                        r: channel(clear.red()),
                        g: channel(clear.green()),
                        b: channel(clear.blue()),
                        a: 1.0,
                    }),
                    store: StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        let (Some(vertex_buffer), Some(index_buffer)) = (&self.vertex_buffer, &self.index_buffer)
        else {
            return;
        };
        if self.indices.as_slice().is_empty() {
            return;
        }
        pass.set_pipeline(&self.pipeline.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.buffer.slice(..));
        pass.set_index_buffer(index_buffer.buffer.slice(..), IndexFormat::Uint32);
        let screen = Rect::at(Point::default(), self.size);
        for batch in &self.batches {
            let visible = batch.clip.intersect(screen);
            if batch.end > batch.start && !visible.is_empty() {
                pass.set_scissor_rect(
                    visible.left.unsigned(),
                    visible.top.unsigned(),
                    visible.width.unsigned(),
                    visible.height.unsigned(),
                );
                pass.draw_indexed(batch.start.get()..batch.end.get(), 0, 0..1);
            }
        }
    }

    pub(crate) fn render(&mut self, drawing: &DrawList, clear: Color) {
        self.record(drawing);
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(SurfaceError::Lost | SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(_) => return,
        };
        self.upload_atlas();
        self.upload_geometry();
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor::default());
        self.encode_pass(&mut encoder, &frame.texture, clear);
        if let Some(path) = self.shot.take() {
            self.read_back(encoder, &frame.texture, &path);
        } else {
            self.queue.submit([encoder.finish()]);
        }
        self.window.pre_present_notify();
        frame.present();
    }

    fn read_back(&self, mut encoder: CommandEncoder, texture: &Texture, path: &Path) {
        let width = self.config.width;
        let height = self.config.height;
        let row = (width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&BufferDescriptor {
            label: Some("shot"),
            size: u64::from(row * height),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(MapMode::Read, |_| {});
        if let Err(error) = self.device.poll(PollType::Wait {
            submission_index: None,
            timeout: None,
        }) {
            report(format_args!("screenshot failed: {error}"));
            return;
        }
        let mapped = slice.get_mapped_range();
        let pixels = Pixels::from_rows(
            &mapped,
            Pixels::row_length(row),
            Pixels::row_length(width * 4),
            Pixels::row_length(height),
            self.config.format,
        );
        drop(mapped);
        buffer.unmap();
        match pixels
            .encode(width, height)
            .and_then(|bytes| io_store::write(path, bytes).map_err(EncodingError::IoError))
        {
            Ok(()) => report(format_args!("screenshot: {}", path.display())),
            Err(error) => report(format_args!("screenshot failed: {error}")),
        }
    }
}

impl Pixels {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    fn row_length(value: u32) -> usize {
        usize::try_from(value).unwrap_or(0)
    }

    pub(crate) fn from_rows(
        bytes: &[u8],
        stride: usize,
        used: usize,
        rows: usize,
        format: TextureFormat,
    ) -> Self {
        let blue_first = matches!(
            format,
            TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb
        );
        let mut out = Vec::with_capacity(used * rows);
        for line in bytes.chunks(stride.max(1)).take(rows) {
            for pixel in line.get(..used).unwrap_or_default().chunks(4) {
                if let [first, second, third, _] = *pixel {
                    if blue_first {
                        out.extend_from_slice(&[third, second, first, 255]);
                    } else {
                        out.extend_from_slice(&[first, second, third, 255]);
                    }
                }
            }
        }
        Self(out)
    }

    fn encode(&self, width: u32, height: u32) -> Result<Vec<u8>, EncodingError> {
        let mut bytes = Vec::new();
        {
            let mut encoder = Encoder::new(&mut bytes, width, height);
            encoder.set_color(ColorType::Rgba);
            encoder.set_depth(BitDepth::Eight);
            let mut writer = encoder.write_header()?;
            writer.write_image_data(self.as_bytes())?;
            writer.finish()?;
        }
        Ok(bytes)
    }
}
