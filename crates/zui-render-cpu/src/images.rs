//! CPU-owned conversion cache. Image identities never escape this adapter.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use vello_cpu::Pixmap;
use zui_paint::{Command, ImageId, ImageResource, RenderError, Scene};
#[derive(Default)]
pub(super) struct Images(BTreeMap<ImageId, Arc<Pixmap>>);
impl Images {
    pub(super) fn prepare(&mut self, scene: &Scene) {
        let used: BTreeSet<_> = scene
            .commands()
            .iter()
            .filter_map(|command| {
                if let Command::Image { image, .. } = command {
                    Some(image.id())
                } else {
                    None
                }
            })
            .collect();
        self.0.retain(|id, _| used.contains(id));
    }
    pub(super) fn get(&mut self, image: &ImageResource) -> Result<Arc<Pixmap>, RenderError> {
        if let Some(pixmap) = self.0.get(&image.id()) {
            return Ok(pixmap.clone());
        }
        let extent = image.extent();
        let width = u16::try_from(extent.width).map_err(|_| RenderError::TargetTooLarge)?;
        let height = u16::try_from(extent.height).map_err(|_| RenderError::TargetTooLarge)?;
        let mut pixmap = Pixmap::new(width, height);
        pixmap.data_as_u8_slice_mut().copy_from_slice(image.bytes());
        let pixmap = Arc::new(pixmap);
        self.0.insert(image.id(), pixmap.clone());
        Ok(pixmap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_paint::{ImageLimits, ImageSampling};
    use zui_types::Extent;
    #[test]
    fn converted_pixels_are_shared_then_released_when_absent_from_the_scene() {
        let image =
            ImageResource::from_rgba8(Extent::new(1, 1), vec![255; 4], ImageLimits::default())
                .unwrap();
        let mut cache = Images::default();
        let first = cache.get(&image).unwrap();
        assert!(Arc::ptr_eq(&first, &cache.get(&image.clone()).unwrap()));
        let weak = Arc::downgrade(&first);
        drop(first);
        let mut scene = Scene::new();
        scene.painter().image(image.clone(), ImageSampling::Nearest);
        cache.prepare(&scene);
        assert!(weak.upgrade().is_some());
        scene.clear();
        cache.prepare(&scene);
        assert!(weak.upgrade().is_none());
        assert_eq!(cache.get(&image).unwrap().data_as_u8_slice(), image.bytes());
    }
}
