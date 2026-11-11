//! Internal lowering shared by the two Vello adapters, never by UI consumers.
//!
//! Keeping command interpretation here ensures identical transforms, fills,
//! clipping and background semantics without coupling one renderer to the other.
use vello_common::{
    kurbo::{Affine, BezPath, Stroke},
    peniko::Color,
};
use zui_paint::{Command, Frame, Path, PathElement, RenderError, RenderLimits, Scene};
pub mod damage;

pub use glifo::{Glyph, NormalizedCoord};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub use vello_common::{TextureId, geometry, kurbo, paint, peniko};

/// Retains identities across frames, while dropping fonts absent from the current scene.
#[derive(Default)]
pub struct FontCache(BTreeMap<zui_paint::FontId, peniko::FontData>);
impl FontCache {
    fn prepare(&mut self, scene: &Scene) {
        let used: BTreeSet<_> = scene
            .commands()
            .iter()
            .filter_map(|command| match command {
                Command::Glyphs { run, .. } => Some(run.font.id()),
                _ => None,
            })
            .collect();
        self.0.retain(|id, _| used.contains(id));
    }
    fn get(&mut self, font: &zui_paint::FontResource) -> Result<peniko::FontData, RenderError> {
        if let Some(data) = self.0.get(&font.id()) {
            return Ok(data.clone());
        }
        // Glifo assumes a valid sfnt/collection face and unwraps this parse.
        // Keep that precondition inside the adapter, not in callers/widgets.
        skrifa::FontRef::from_index(font.bytes(), font.index())
            .map_err(|_| RenderError::InvalidFont)?;
        let data = peniko::FontData::new(peniko::Blob::new(Arc::new(font.clone())), font.index());
        self.0.insert(font.id(), data.clone());
        Ok(data)
    }
}

pub fn glyphs(run: &zui_paint::GlyphRun) -> impl Iterator<Item = Glyph> + Clone + '_ {
    run.glyphs.iter().map(|glyph| Glyph {
        id: glyph.id,
        x: glyph.x,
        y: glyph.y,
    })
}
pub fn coords(run: &zui_paint::GlyphRun) -> &[NormalizedCoord] {
    &run.normalized_coords
}

pub trait Canvas {
    fn image(
        &mut self,
        image: &zui_paint::ImageResource,
        sampling: zui_paint::ImageSampling,
    ) -> Result<(), RenderError>;
    fn glyphs(
        &mut self,
        run: &zui_paint::GlyphRun,
        font: &peniko::FontData,
        color: Color,
    ) -> Result<(), RenderError>;
    fn transform(&mut self, transform: Affine);
    fn fill(&mut self, path: &BezPath, color: Color);
    fn stroke(&mut self, path: &BezPath, color: Color, stroke: Stroke);
    fn push_clip(&mut self, path: &BezPath);
    fn pop_clip(&mut self);
}

pub fn dimensions(frame: Frame, limits: RenderLimits) -> Result<(u16, u16), RenderError> {
    frame.validate(limits)?;
    let width = u16::try_from(frame.extent.width).map_err(|_| RenderError::TargetTooLarge)?;
    let height = u16::try_from(frame.extent.height).map_err(|_| RenderError::TargetTooLarge)?;
    Ok((width, height))
}

pub fn replay(
    scene: &Scene,
    frame: Frame,
    fonts: &mut FontCache,
    canvas: &mut impl Canvas,
) -> Result<(), RenderError> {
    replay_region(
        scene,
        frame,
        damage::PixelRegion::full(frame),
        fonts,
        canvas,
    )
}

pub fn replay_region(
    scene: &Scene,
    frame: Frame,
    region: damage::PixelRegion,
    fonts: &mut FontCache,
    canvas: &mut impl Canvas,
) -> Result<(), RenderError> {
    replay_filtered(scene, frame, region, fonts, canvas, |_| true)
}

pub fn replay_filtered(
    scene: &Scene,
    frame: Frame,
    region: damage::PixelRegion,
    fonts: &mut FontCache,
    canvas: &mut impl Canvas,
    includes: impl Fn(usize) -> bool,
) -> Result<(), RenderError> {
    fonts.prepare(scene);
    canvas.transform(Affine::IDENTITY);
    canvas.fill(
        &kurbo::Rect::new(0.0, 0.0, region.width as f64, region.height as f64).to_path(0.1),
        color(frame.background),
    );
    let scale =
        Affine::translate((-(region.x as f64), -(region.y as f64))) * Affine::scale(frame.scale);
    for (index, command) in scene.commands().iter().enumerate() {
        if !matches!(command, Command::PushClip { .. } | Command::PopClip) && !includes(index) {
            continue;
        }
        match command {
            Command::Image {
                image,
                sampling,
                transform,
            } => {
                canvas.transform(scale * Affine::new(transform.0));
                canvas.image(image, *sampling)?;
            }
            Command::Glyphs {
                run,
                color: paint,
                transform,
            } => {
                canvas.transform(scale * Affine::new(transform.0));
                canvas.glyphs(run, &fonts.get(&run.font)?, color(*paint))?;
            }
            Command::Fill {
                path,
                color: paint,
                transform,
            } => {
                canvas.transform(scale * Affine::new(transform.0));
                canvas.fill(&lower_path(path), color(*paint));
            }
            Command::Stroke {
                path,
                color: paint,
                width,
                transform,
            } => {
                canvas.transform(scale * Affine::new(transform.0));
                canvas.stroke(&lower_path(path), color(*paint), Stroke::new(*width));
            }
            Command::PushClip { path, transform } => {
                canvas.transform(scale * Affine::new(transform.0));
                canvas.push_clip(&lower_path(path));
            }
            Command::PopClip => canvas.pop_clip(),
        }
    }
    Ok(())
}

fn color(value: zui_types::Color) -> Color {
    Color::from_rgba8(value.r, value.g, value.b, value.a)
}

fn lower_path(path: &Path) -> BezPath {
    let mut result = BezPath::new();
    for element in path.elements() {
        match *element {
            PathElement::MoveTo(p) => result.move_to((p.x, p.y)),
            PathElement::LineTo(p) => result.line_to((p.x, p.y)),
            PathElement::QuadTo(c, p) => result.quad_to((c.x, c.y), (p.x, p.y)),
            PathElement::CubicTo(a, b, p) => result.curve_to((a.x, a.y), (b.x, b.y), (p.x, p.y)),
            PathElement::Close => result.close_path(),
        }
    }
    result
}

use kurbo::Shape;

/// Sampling policy shared by CPU pixmaps and GPU external textures.
pub fn image_sampler(sampling: zui_paint::ImageSampling) -> peniko::ImageSampler {
    peniko::ImageSampler {
        quality: match sampling {
            zui_paint::ImageSampling::Nearest => peniko::ImageQuality::Low,
            zui_paint::ImageSampling::Bilinear => peniko::ImageQuality::Medium,
        },
        ..Default::default()
    }
}
