//! Renderer-owned external textures, keyed only by portable image identity.
use std::collections::{BTreeMap, BTreeSet};
use zui_paint::{Command, ImageId, ImageResource, RenderError, Scene};
use zui_vello::{TextureId, geometry::RectU16, paint::ImageSource};
struct Entry {
    id: TextureId,
    view: wgpu::TextureView,
}
#[derive(Default)]
pub(super) struct Images {
    entries: BTreeMap<ImageId, Entry>,
    next: u64,
    pub(super) bindings: vello_gpu::TextureBindings,
}
impl Images {
    pub(super) fn prepare(
        &mut self,
        scene: &Scene,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), RenderError> {
        let mut used = BTreeSet::new();
        // Validate all sizes before any uploads or changes to retained resources.
        let maximum = device
            .limits()
            .max_texture_dimension_2d
            .min(u16::MAX as u32);
        for command in scene.commands() {
            if let Command::Image { image, .. } = command {
                let e = image.extent();
                if e.width > maximum || e.height > maximum {
                    return Err(RenderError::TargetTooLarge);
                }
                used.insert(image.id());
            }
        }
        self.entries.retain(|id, _| used.contains(id));
        self.bindings = vello_gpu::TextureBindings::new();
        for command in scene.commands() {
            let Command::Image { image, .. } = command else {
                continue;
            };
            if !self.entries.contains_key(&image.id()) {
                let id = TextureId(self.next);
                self.next = self.next.checked_add(1).ok_or_else(|| {
                    RenderError::Backend("image texture identities exhausted".into())
                })?;
                let e = image.extent();
                let size = wgpu::Extent3d {
                    width: e.width,
                    height: e.height,
                    depth_or_array_layers: 1,
                };
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("ZUI image"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    image.bytes(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(e.width * 4),
                        rows_per_image: Some(e.height),
                    },
                    size,
                );
                self.entries.insert(
                    image.id(),
                    Entry {
                        id,
                        view: texture.create_view(&Default::default()),
                    },
                );
            }
        }
        for entry in self.entries.values() {
            self.bindings.insert(entry.id, entry.view.clone());
        }
        Ok(())
    }
    pub(super) fn source(&self, image: &ImageResource) -> Result<ImageSource, RenderError> {
        let entry = self
            .entries
            .get(&image.id())
            .ok_or_else(|| RenderError::Backend("image was not prepared".into()))?;
        let e = image.extent();
        Ok(ImageSource::external_texture(
            entry.id,
            RectU16::new(0, 0, e.width as u16, e.height as u16),
            true,
        ))
    }
}
