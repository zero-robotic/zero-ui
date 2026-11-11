use crate::{
    kurbo::{Affine, Rect, Shape},
    lower_path,
};
use std::sync::Arc;
use zui_paint::{Command, DamageConfig, DamageStats, Frame, RenderError, Scene};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRegion {
    pub fn full(frame: Frame) -> Self {
        Self {
            x: 0,
            y: 0,
            width: frame.extent.width,
            height: frame.extent.height,
        }
    }
    pub fn rect(self) -> Rect {
        Rect::new(
            self.x as f64,
            self.y as f64,
            (self.x + self.width) as f64,
            (self.y + self.height) as f64,
        )
    }
    pub fn pixels(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

struct Draw {
    index: usize,
    clips: Arc<[usize]>,
    bounds: Rect,
    raw_bounds: Rect,
}

struct Snapshot {
    scene: Scene,
    frame: Frame,
    draws: Vec<Draw>,
    bytes: usize,
}

pub struct DamagePlan {
    pub regions: Vec<PixelRegion>,
    snapshot: Option<Arc<Snapshot>>,
}

impl DamagePlan {
    pub fn full(frame: Frame) -> Self {
        Self {
            regions: vec![PixelRegion::full(frame)],
            snapshot: None,
        }
    }
    pub fn includes(&self, index: usize, region: PixelRegion) -> bool {
        self.snapshot.as_ref().is_none_or(|snapshot| {
            snapshot
                .draws
                .binary_search_by_key(&index, |draw| draw.index)
                .ok()
                .is_none_or(|position| {
                    intersect(snapshot.draws[position].bounds, region.rect()).area() > 0.0
                })
        })
    }
}

pub struct DamageTracker {
    previous: Option<Arc<Snapshot>>,
    config: DamageConfig,
    stats: DamageStats,
}

impl DamageTracker {
    pub fn new(config: DamageConfig) -> Self {
        Self {
            previous: None,
            config,
            stats: Default::default(),
        }
    }
    pub fn invalidate(&mut self) {
        self.previous = None;
        self.stats = Default::default();
    }
    pub fn stats(&self) -> DamageStats {
        self.stats
    }
    pub fn plan(&self, scene: &Scene, frame: Frame) -> Result<DamagePlan, RenderError> {
        let full = PixelRegion::full(frame);
        if let Some(previous) = &self.previous
            && previous.frame == frame
            && previous.scene.same_commands(scene)
        {
            return Ok(DamagePlan {
                regions: Vec::new(),
                snapshot: Some(previous.clone()),
            });
        }
        let snapshot = Snapshot::new(
            scene,
            frame,
            self.config.max_scene_bytes,
            self.previous.as_deref(),
        )?
        .map(Arc::new);
        let mut regions = Vec::new();
        if let (Some(old), Some(new)) = (&self.previous, &snapshot)
            && old.frame == frame
            && self.config.max_regions > 0
        {
            let equal = |before: &Draw, after: &Draw| {
                old.scene.commands()[before.index] == new.scene.commands()[after.index]
                    && before.clips.len() == after.clips.len()
                    && before
                        .clips
                        .iter()
                        .zip(after.clips.iter())
                        .all(|(left, right)| {
                            old.scene.commands()[*left] == new.scene.commands()[*right]
                        })
            };
            if old.draws.len() == new.draws.len() {
                for (before, after) in old.draws.iter().zip(&new.draws) {
                    if !equal(before, after) {
                        add(&mut regions, before.bounds, full, self.config.max_regions);
                        add(&mut regions, after.bounds, full, self.config.max_regions);
                    }
                }
            } else {
                let prefix = old
                    .draws
                    .iter()
                    .zip(&new.draws)
                    .take_while(|(a, b)| equal(a, b))
                    .count();
                let suffix = old.draws[prefix..]
                    .iter()
                    .rev()
                    .zip(new.draws[prefix..].iter().rev())
                    .take_while(|(a, b)| equal(a, b))
                    .count();
                for draw in old.draws[prefix..old.draws.len() - suffix]
                    .iter()
                    .chain(&new.draws[prefix..new.draws.len() - suffix])
                {
                    add(&mut regions, draw.bounds, full, self.config.max_regions);
                }
            }
            if regions.iter().map(|region| region.pixels()).sum::<u64>() * 5 > full.pixels() * 3 {
                regions = vec![full];
            }
        } else {
            regions.push(full);
        }
        Ok(DamagePlan { regions, snapshot })
    }
    pub fn commit(&mut self, plan: DamagePlan, frame: Frame, pixel_bytes: usize) {
        self.stats = DamageStats {
            regions: plan.regions.len(),
            rasterized_pixels: plan.regions.iter().map(|region| region.pixels()).sum(),
            total_pixels: PixelRegion::full(frame).pixels(),
            reused_frame: plan.regions.is_empty(),
            retained_bytes: plan
                .snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.bytes)
                .saturating_add(pixel_bytes),
        };
        self.previous = plan.snapshot;
    }
}

impl Snapshot {
    fn new(
        scene: &Scene,
        frame: Frame,
        budget: usize,
        previous: Option<&Self>,
    ) -> Result<Option<Self>, RenderError> {
        let mut bytes = scene.storage_bytes();
        if budget == 0 || bytes > budget {
            return Ok(None);
        }
        let full = PixelRegion::full(frame).rect();
        let scale = Affine::scale(frame.scale);
        let mut clip_bounds = vec![full];
        let mut clips = Vec::new();
        let mut shared_clips: Arc<[usize]> = Arc::from([]);
        let mut draws = Vec::new();
        for (index, command) in scene.commands().iter().enumerate() {
            match command {
                Command::PushClip { path, transform } => {
                    let bounds = (scale * Affine::new(transform.0))
                        .transform_rect_bbox(lower_path(path).bounding_box())
                        .inflate(2.0, 2.0);
                    clip_bounds.push(intersect(*clip_bounds.last().unwrap(), bounds));
                    clips.push(index);
                    shared_clips = clips.clone().into();
                    bytes = bytes.saturating_add(clips.len() * core::mem::size_of::<usize>());
                }
                Command::PopClip => {
                    clip_bounds.pop();
                    clips.pop();
                    shared_clips = clips.clone().into();
                    bytes = bytes.saturating_add(clips.len() * core::mem::size_of::<usize>());
                }
                _ => {
                    let reused = previous.filter(|old| old.frame == frame).and_then(|old| {
                        let draw = old.draws.get(draws.len())?;
                        same_geometry(&old.scene.commands()[draw.index], command)
                            .then_some(draw.raw_bounds)
                    });
                    let bounds = match reused {
                        Some(bounds) => bounds,
                        None => command_bounds(command, frame)?,
                    };
                    draws.push(Draw {
                        index,
                        clips: shared_clips.clone(),
                        bounds: intersect(bounds, *clip_bounds.last().unwrap()),
                        raw_bounds: bounds,
                    });
                    bytes = bytes.saturating_add(core::mem::size_of::<Draw>() * 2);
                }
            }
            if bytes > budget {
                return Ok(None);
            }
        }
        Ok(Some(Self {
            scene: scene.clone(),
            frame,
            draws,
            bytes,
        }))
    }
}

fn same_geometry(before: &Command, after: &Command) -> bool {
    match (before, after) {
        (
            Command::Glyphs {
                run: left,
                transform: left_transform,
                ..
            },
            Command::Glyphs {
                run: right,
                transform: right_transform,
                ..
            },
        ) => left == right && left_transform == right_transform,
        (
            Command::Fill {
                path: left,
                transform: left_transform,
                ..
            },
            Command::Fill {
                path: right,
                transform: right_transform,
                ..
            },
        ) => left == right && left_transform == right_transform,
        (
            Command::Stroke {
                path: left,
                transform: left_transform,
                width: left_width,
                ..
            },
            Command::Stroke {
                path: right,
                transform: right_transform,
                width: right_width,
                ..
            },
        ) => left == right && left_transform == right_transform && left_width == right_width,
        _ => before == after,
    }
}

pub fn command_bounds(command: &Command, frame: Frame) -> Result<Rect, RenderError> {
    let full = PixelRegion::full(frame).rect();
    let (bounds, transform, padding) = match command {
        Command::Fill {
            path, transform, ..
        } => (lower_path(path).bounding_box(), transform, 0.0),
        Command::Stroke {
            path,
            transform,
            width,
            ..
        } => (lower_path(path).bounding_box(), transform, width * 5.0),
        Command::Image {
            image, transform, ..
        } => (
            Rect::new(
                0.0,
                0.0,
                image.extent().width as f64,
                image.extent().height as f64,
            ),
            transform,
            0.0,
        ),
        Command::Glyphs { run, transform, .. } => {
            use skrifa::{
                GlyphId, MetadataProvider,
                instance::{LocationRef, Size},
            };
            let font = skrifa::FontRef::from_index(run.font.bytes(), run.font.index())
                .map_err(|_| RenderError::InvalidFont)?;
            let coords: Vec<_> = run
                .normalized_coords
                .iter()
                .map(|value| skrifa::instance::NormalizedCoord::from_bits(*value))
                .collect();
            let metrics = font.glyph_metrics(Size::new(run.font_size), LocationRef::new(&coords));
            let mut bounds: Option<Rect> = None;
            for glyph in &run.glyphs {
                let area = if let Some(area) = metrics.bounds(GlyphId::new(glyph.id)) {
                    area
                } else {
                    let outlines = font.outline_glyphs();
                    let Some(outline) = outlines.get(GlyphId::new(glyph.id)) else {
                        return Ok(full);
                    };
                    let mut pen = skrifa::outline::pen::ControlBoundsPen::default();
                    if outline
                        .draw(
                            skrifa::outline::DrawSettings::unhinted(
                                Size::new(run.font_size),
                                LocationRef::new(&coords),
                            ),
                            &mut pen,
                        )
                        .is_err()
                    {
                        return Ok(full);
                    }
                    let Some(area) = pen.bounding_box() else {
                        continue;
                    };
                    area
                };
                let area = Rect::new(
                    (glyph.x + area.x_min) as f64,
                    (glyph.y - area.y_max) as f64,
                    (glyph.x + area.x_max) as f64,
                    (glyph.y - area.y_min) as f64,
                );
                bounds = Some(bounds.map_or(area, |old| old.union(area)));
            }
            (bounds.unwrap_or(Rect::ZERO), transform, 2.0)
        }
        _ => return Ok(full),
    };
    let bounds = (Affine::scale(frame.scale) * Affine::new(transform.0))
        .transform_rect_bbox(bounds.inflate(padding, padding))
        .inflate(2.0, 2.0);
    Ok(if bounds.is_finite() {
        intersect(bounds, full)
    } else {
        full
    })
}

pub fn intersect(left: Rect, right: Rect) -> Rect {
    let result = left.intersect(right);
    if result.width() <= 0.0 || result.height() <= 0.0 {
        Rect::ZERO
    } else {
        result
    }
}

fn add(regions: &mut Vec<PixelRegion>, bounds: Rect, full: PixelRegion, limit: usize) {
    if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
        return;
    }
    let mut area = PixelRegion {
        x: bounds.x0.floor().max(0.0) as u32,
        y: bounds.y0.floor().max(0.0) as u32,
        width: 0,
        height: 0,
    };
    area.width = (bounds.x1.ceil() as u32)
        .min(full.width)
        .saturating_sub(area.x);
    area.height = (bounds.y1.ceil() as u32)
        .min(full.height)
        .saturating_sub(area.y);
    let right = (area.x + area.width).next_multiple_of(64).min(full.width);
    let bottom = (area.y + area.height).next_multiple_of(32).min(full.height);
    area.x = area.x / 64 * 64;
    area.y = area.y / 32 * 32;
    area.width = right - area.x;
    area.height = bottom - area.y;
    let mut index = 0;
    while index < regions.len() {
        let old = regions[index];
        if area.x <= old.x + old.width
            && old.x <= area.x + area.width
            && area.y <= old.y + old.height
            && old.y <= area.y + area.height
        {
            let right = (area.x + area.width).max(old.x + old.width);
            let bottom = (area.y + area.height).max(old.y + old.height);
            area.x = area.x.min(old.x);
            area.y = area.y.min(old.y);
            area.width = right - area.x;
            area.height = bottom - area.y;
            regions.swap_remove(index);
            index = 0;
        } else {
            index += 1;
        }
    }
    regions.push(area);
    if regions.len() > limit {
        *regions = vec![full];
    }
}
