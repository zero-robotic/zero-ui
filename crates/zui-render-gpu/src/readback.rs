use crate::backend;
use std::sync::mpsc;
use zui_paint::{PixelBuffer, RenderError, RenderLimits};
use zui_types::Extent;

pub(crate) fn copy(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    texture: &wgpu::Texture,
    extent: Extent,
    limits: RenderLimits,
) -> Result<PixelBuffer, RenderError> {
    let row_bytes = extent.width * 4;
    let padded_row = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ZUI pixel readback"),
        size: u64::from(padded_row) * u64::from(extent.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: extent.width,
            height: extent.height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(backend)?;
    receiver.recv().map_err(backend)?.map_err(backend)?;
    let mut output = PixelBuffer::new(extent, limits)?;
    {
        let mapped = buffer.slice(..).get_mapped_range().map_err(backend)?;
        for (source, target) in mapped
            .chunks_exact(padded_row as usize)
            .zip(output.pixels_mut().chunks_exact_mut(row_bytes as usize))
        {
            target.copy_from_slice(&source[..row_bytes as usize]);
        }
    }
    buffer.unmap();
    Ok(output)
}
