use crate::{GlyphRun, ImageResource, Path, RenderError, RenderLimits};
use alloc::{collections::BTreeSet, sync::Arc, vec::Vec};
use zui_types::{Affine, Color, Rect};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageSampling {
    Nearest,
    #[default]
    Bilinear,
}

/// Non-zero winding fills and source-over composition are the initial baseline.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Native image pixels occupy [0, width] × [0, height] before transform.
    Image {
        image: ImageResource,
        sampling: ImageSampling,
        transform: Affine,
    },
    Glyphs {
        run: GlyphRun,
        color: Color,
        transform: Affine,
    },
    Fill {
        path: Path,
        color: Color,
        transform: Affine,
    },
    Stroke {
        path: Path,
        color: Color,
        width: f64,
        transform: Affine,
    },
    PushClip {
        path: Path,
        transform: Affine,
    },
    PopClip,
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    commands: Arc<Vec<Command>>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }
    pub fn same_commands(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.commands, &other.commands) || self.commands == other.commands
    }
    pub fn clear(&mut self) {
        Arc::make_mut(&mut self.commands).clear();
    }
    pub fn storage_bytes(&self) -> usize {
        self.commands.iter().fold(
            self.commands
                .capacity()
                .saturating_mul(core::mem::size_of::<Command>()),
            |bytes, command| {
                bytes.saturating_add(match command {
                    Command::Fill { path, .. }
                    | Command::Stroke { path, .. }
                    | Command::PushClip { path, .. } => path.storage_bytes(),
                    Command::Glyphs { run, .. } => run
                        .glyphs
                        .capacity()
                        .saturating_mul(core::mem::size_of::<crate::Glyph>())
                        .saturating_add(run.normalized_coords.capacity().saturating_mul(2)),
                    _ => 0,
                })
            },
        )
    }
    pub fn painter(&mut self) -> Painter<'_> {
        Painter {
            scene: self,
            transform: Affine::IDENTITY,
        }
    }

    /// Validate before backend submission, including aggregate complexity limits.
    pub fn validate(&self, limits: RenderLimits) -> Result<(), RenderError> {
        if self.commands.len() > limits.max_commands {
            return Err(RenderError::SceneTooComplex);
        }
        let mut depth = 0_usize;
        let mut elements = 0_usize;
        let mut glyphs = 0_usize;
        let mut font_bytes = 0_usize;
        let mut fonts = BTreeSet::new();
        let mut images = BTreeSet::new();
        let mut image_bytes = 0usize;
        for command in self.commands.iter() {
            let (path, transform) = match command {
                Command::Image {
                    image, transform, ..
                } => {
                    if !transform.is_finite() {
                        return Err(RenderError::InvalidGeometry);
                    }
                    if images.insert(image.id()) {
                        image_bytes = image_bytes
                            .checked_add(image.bytes().len())
                            .ok_or(RenderError::SceneTooComplex)?;
                    }
                    if images.len() > limits.max_images || image_bytes > limits.max_image_bytes {
                        return Err(RenderError::SceneTooComplex);
                    }
                    continue;
                }
                Command::Glyphs { run, transform, .. } => {
                    glyphs = glyphs
                        .checked_add(run.glyphs.len())
                        .ok_or(RenderError::SceneTooComplex)?;
                    if fonts.insert(run.font.id()) {
                        font_bytes = font_bytes
                            .checked_add(run.font.bytes().len())
                            .ok_or(RenderError::SceneTooComplex)?;
                    }
                    if glyphs > limits.max_glyphs || font_bytes > limits.max_font_bytes {
                        return Err(RenderError::SceneTooComplex);
                    }
                    if !transform.is_finite()
                        || !run.font_size.is_finite()
                        || run.font_size <= 0.0
                        || run
                            .glyphs
                            .iter()
                            .any(|glyph| !glyph.x.is_finite() || !glyph.y.is_finite())
                        || run
                            .normalized_coords
                            .iter()
                            .any(|coord| !(-16384..=16384).contains(coord))
                    {
                        return Err(RenderError::InvalidGeometry);
                    }
                    continue;
                }
                Command::Fill {
                    path, transform, ..
                } => (path, transform),
                Command::Stroke {
                    path,
                    transform,
                    width,
                    ..
                } => {
                    if !width.is_finite() || *width <= 0.0 {
                        return Err(RenderError::InvalidGeometry);
                    }
                    (path, transform)
                }
                Command::PushClip { path, transform } => {
                    depth += 1;
                    if depth > limits.max_clip_depth {
                        return Err(RenderError::SceneTooComplex);
                    }
                    (path, transform)
                }
                Command::PopClip => {
                    depth = depth.checked_sub(1).ok_or(RenderError::UnbalancedClip)?;
                    continue;
                }
            };
            elements = elements
                .checked_add(path.elements().len())
                .ok_or(RenderError::SceneTooComplex)?;
            if elements > limits.max_path_elements {
                return Err(RenderError::SceneTooComplex);
            }
            if !path.is_valid() || !transform.is_finite() {
                return Err(RenderError::InvalidGeometry);
            }
        }
        if depth != 0 {
            return Err(RenderError::UnbalancedClip);
        }
        Ok(())
    }
}

/// Scoped transforms and clips avoid leaking painter state between controls.
pub struct Painter<'a> {
    scene: &'a mut Scene,
    transform: Affine,
}

impl Painter<'_> {
    pub fn append(&mut self, scene: &Scene) {
        let transform = self.transform;
        Arc::make_mut(&mut self.scene.commands).extend(scene.commands().iter().cloned().map(
            |mut command| {
                match &mut command {
                    Command::Image {
                        transform: local, ..
                    }
                    | Command::Glyphs {
                        transform: local, ..
                    }
                    | Command::Fill {
                        transform: local, ..
                    }
                    | Command::Stroke {
                        transform: local, ..
                    }
                    | Command::PushClip {
                        transform: local, ..
                    } => *local = transform.compose(*local),
                    Command::PopClip => {}
                }
                command
            },
        ));
    }
    pub fn image(&mut self, image: ImageResource, sampling: ImageSampling) {
        Arc::make_mut(&mut self.scene.commands).push(Command::Image {
            image,
            sampling,
            transform: self.transform,
        });
    }
    pub fn glyphs(&mut self, run: GlyphRun, color: Color) {
        Arc::make_mut(&mut self.scene.commands).push(Command::Glyphs {
            run,
            color,
            transform: self.transform,
        });
    }
    pub fn fill(&mut self, path: Path, color: Color) {
        Arc::make_mut(&mut self.scene.commands).push(Command::Fill {
            path,
            color,
            transform: self.transform,
        });
    }
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.fill(Path::rectangle(rect), color);
    }
    pub fn stroke(&mut self, path: Path, color: Color, width: f64) {
        Arc::make_mut(&mut self.scene.commands).push(Command::Stroke {
            path,
            color,
            width,
            transform: self.transform,
        });
    }
    pub fn with_transform(&mut self, local: Affine, draw: impl FnOnce(&mut Painter<'_>)) {
        let mut nested = Painter {
            scene: self.scene,
            transform: self.transform.compose(local),
        };
        draw(&mut nested);
    }
    pub fn with_clip(&mut self, path: Path, draw: impl FnOnce(&mut Painter<'_>)) {
        Arc::make_mut(&mut self.scene.commands).push(Command::PushClip {
            path,
            transform: self.transform,
        });
        draw(self);
        Arc::make_mut(&mut self.scene.commands).push(Command::PopClip);
    }
}
