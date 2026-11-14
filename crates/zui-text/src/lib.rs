//! Parley implementation of ZUI's portable text service.
mod font_file;
mod geometry;
#[cfg(feature = "system-fonts")]
mod system_fonts;
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontFamilyName, Layout, LayoutContext,
    LineHeight, PositionedLayoutItem, StyleProperty,
};
use std::{collections::BTreeMap, sync::Arc};
use zui_api::text::{
    InteractiveTextLayout, TextAlign, TextError, TextLayout, TextLine, TextRequest, TextService,
    TextWidth,
};
use zui_paint::{FontResource, Glyph, GlyphRun};
use zui_types::Size;

/// Behavior after the registered family fallback chain cannot cover a glyph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MissingGlyphPolicy {
    /// Keep the original text and render the selected font's glyph zero (.notdef).
    /// Its visible shape depends on the font; no replacement text is inserted.
    #[default]
    FontPlaceholder,
    /// Fail layout, useful for font coverage validation and strict exports.
    Reject,
}

#[derive(Clone, Copy, Debug)]
pub struct TextLimits {
    pub max_font_bytes: usize,
    pub max_font_faces: usize,
    pub max_text_bytes: usize,
    pub max_glyphs: usize,
    pub max_font_size: f32,
}
impl Default for TextLimits {
    fn default() -> Self {
        Self {
            max_font_bytes: 64 * 1024 * 1024,
            max_font_faces: 64,
            max_text_bytes: 1024 * 1024,
            max_glyphs: 100_000,
            max_font_size: 512.0,
        }
    }
}

pub struct ParleyText {
    fonts: FontContext,
    context: LayoutContext<()>,
    resources: BTreeMap<(u64, u32), FontResource>,
    system_fonts: bool,
    font_bytes: usize,
    epoch: u64,
    limits: TextLimits,
    missing_glyph_policy: MissingGlyphPolicy,
}
impl Default for ParleyText {
    fn default() -> Self {
        Self::new(TextLimits::default())
    }
}
impl ParleyText {
    pub fn new(limits: TextLimits) -> Self {
        Self::new_with_policy(limits, MissingGlyphPolicy::default())
    }
    /// Policy is fixed for the service lifetime, so cached layouts remain valid.
    pub fn new_with_policy(limits: TextLimits, missing_glyph_policy: MissingGlyphPolicy) -> Self {
        Self::with_font_source(limits, missing_glyph_policy, false)
    }
    fn with_font_source(
        limits: TextLimits,
        missing_glyph_policy: MissingGlyphPolicy,
        system_fonts: bool,
    ) -> Self {
        let fonts = FontContext {
            collection: parley::fontique::Collection::new(parley::fontique::CollectionOptions {
                shared: false,
                system_fonts,
            }),
            source_cache: Default::default(),
        };
        Self {
            fonts,
            context: LayoutContext::new(),
            resources: BTreeMap::new(),
            system_fonts,
            font_bytes: 0,
            epoch: u64::from(system_fonts),
            limits,
            missing_glyph_policy,
        }
    }
    /// Registers caller-owned bytes, never discovers or reads operating-system fonts.
    /// Registration is staged in a cloned collection so invalid data/budgets do not
    /// partially change the active font set or its epoch.
    pub fn register_font(&mut self, bytes: Arc<[u8]>) -> Result<Vec<String>, TextError> {
        let total = self
            .font_bytes
            .checked_add(bytes.len())
            .ok_or(TextError::BudgetExceeded)?;
        if total > self.limits.max_font_bytes {
            return Err(TextError::BudgetExceeded);
        }
        let epoch = self.epoch.checked_add(1).ok_or(TextError::BudgetExceeded)?;
        let blob = parley::fontique::Blob::new(Arc::new(bytes.clone()));
        let blob_id = blob.id();
        let mut collection = self.fonts.collection.clone();
        let registered = collection.register_fonts(blob, None);
        let faces: usize = registered.iter().map(|(_, fonts)| fonts.len()).sum();
        if faces == 0 {
            return Err(TextError::InvalidFont);
        }
        if faces
            > self
                .limits
                .max_font_faces
                .saturating_sub(self.resources.len())
        {
            return Err(TextError::BudgetExceeded);
        }
        let mut resources = Vec::new();
        let mut names = Vec::new();
        for (family, fonts) in registered {
            let name = collection
                .family_name(family)
                .ok_or(TextError::InvalidFont)?
                .to_owned();
            if !names.contains(&name) {
                names.push(name);
            }
            for font in fonts {
                let resource = FontResource::new(bytes.clone(), font.index())
                    .map_err(|_| TextError::InvalidFont)?;
                resources.push(((blob_id, font.index()), resource));
            }
        }
        self.fonts.collection = collection;
        self.resources.extend(resources);
        self.font_bytes = total;
        self.epoch = epoch;
        Ok(names)
    }
}

impl ParleyText {
    fn shape(&mut self, request: &TextRequest<'_>) -> Result<Layout<()>, TextError> {
        let style = request.style;
        if !style.font_size.is_finite()
            || style.font_size <= 0.0
            || style.font_size > self.limits.max_font_size
            || !style.line_height.is_finite()
            || style.line_height <= 0.0
            || style.families.is_empty()
        {
            return Err(TextError::InvalidStyle);
        }
        if request.text.len() > self.limits.max_text_bytes {
            return Err(TextError::BudgetExceeded);
        }
        if let TextWidth::AtMost(width) = request.width
            && (!width.is_finite() || width < 0.0)
        {
            return Err(TextError::InvalidWidth);
        }
        if !self.system_fonts && self.resources.is_empty() {
            return Err(TextError::NoFonts);
        }
        for family in &style.families {
            if self.fonts.collection.family_id(family).is_none() {
                return Err(TextError::UnknownFamily(family.clone()));
            }
        }
        let families: Vec<_> = style
            .families
            .iter()
            .map(|name| FontFamilyName::Named(name.as_str().into()))
            .collect();
        let mut builder = self
            .context
            .ranged_builder(&mut self.fonts, request.text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::List(
            families.as_slice().into(),
        )));
        builder.push_default(StyleProperty::FontSize(style.font_size));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            style.line_height,
        )));
        let mut layout: Layout<()> = builder.build(request.text);
        let width = match request.width {
            TextWidth::Unbounded => None,
            TextWidth::AtMost(width) => Some(width),
            TextWidth::MinContent => Some(layout.calculate_content_widths().min),
        };
        layout.break_all_lines(width);
        layout.align(
            match style.align {
                TextAlign::Start => Alignment::Start,
                TextAlign::Center => Alignment::Center,
                TextAlign::End => Alignment::End,
            },
            AlignmentOptions::default(),
        );
        let size = Size::new(layout.full_width() as f64, layout.height() as f64);
        if !size.is_valid() {
            return Err(TextError::InvalidStyle);
        }
        Ok(layout)
    }
    fn export(&mut self, layout: &Layout<()>) -> Result<TextLayout, TextError> {
        let size = Size::new(layout.full_width() as f64, layout.height() as f64);
        let mut runs = Vec::new();
        let mut lines = Vec::new();
        let mut glyph_count = 0_usize;
        // Stage newly used system faces. A failed export must not consume our
        // retained-resource budget or publish partially validated resources.
        let mut pending_resources = BTreeMap::new();
        let mut font_bytes = self.font_bytes;
        for line in layout.lines() {
            lines.push(TextLine {
                text_range: line.text_range(),
                baseline: line.metrics().baseline,
                height: line.metrics().line_height,
            });
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(positioned) = item else {
                    continue;
                };
                let run = positioned.run();
                if run.synthesis().embolden() || run.synthesis().skew().is_some() {
                    return Err(TextError::UnsupportedFontSynthesis);
                }
                let font = run.font();
                let key = (font.data.id(), font.index);
                let resource = if let Some(resource) = self
                    .resources
                    .get(&key)
                    .or_else(|| pending_resources.get(&key))
                {
                    resource.clone()
                } else {
                    if !self.system_fonts {
                        return Err(TextError::InvalidFont);
                    }
                    if self.resources.len() + pending_resources.len() >= self.limits.max_font_faces
                    {
                        return Err(TextError::BudgetExceeded);
                    }
                    let bytes = font_file::standalone_face(
                        font.data.data(),
                        font.index,
                        self.limits.max_font_bytes.saturating_sub(font_bytes),
                    )?;
                    font_bytes += bytes.len();
                    let resource =
                        FontResource::new(bytes, 0).map_err(|_| TextError::InvalidFont)?;
                    pending_resources.insert(key, resource.clone());
                    resource
                };
                let mut glyphs = Vec::new();
                for glyph in positioned.positioned_glyphs() {
                    if glyph.id == 0 && self.missing_glyph_policy == MissingGlyphPolicy::Reject {
                        return Err(TextError::MissingGlyph);
                    }
                    if !glyph.x.is_finite() || !glyph.y.is_finite() {
                        return Err(TextError::InvalidStyle);
                    }
                    glyph_count += 1;
                    if glyph_count > self.limits.max_glyphs {
                        return Err(TextError::BudgetExceeded);
                    }
                    glyphs.push(Glyph {
                        id: glyph.id,
                        x: glyph.x,
                        y: glyph.y,
                    });
                }
                runs.push(GlyphRun {
                    font: resource,
                    font_size: run.font_size(),
                    glyphs,
                    normalized_coords: run
                        .normalized_coords()
                        .iter()
                        .map(|coord| coord.to_bits())
                        .collect(),
                });
            }
        }
        self.resources.extend(pending_resources);
        self.font_bytes = font_bytes;
        // Lazy materialization does not change font selection or the epoch.
        Ok(TextLayout {
            size,
            first_baseline: layout.first_baseline(),
            lines,
            runs,
            font_epoch: self.epoch,
        })
    }
}

impl TextService for ParleyText {
    fn font_epoch(&self) -> u64 {
        self.epoch
    }
    fn layout(&mut self, request: TextRequest<'_>) -> Result<TextLayout, TextError> {
        let shaped = self.shape(&request)?;
        self.export(&shaped)
    }
    fn layout_interactive(
        &mut self,
        request: TextRequest<'_>,
    ) -> Result<InteractiveTextLayout, TextError> {
        let shaped = self.shape(&request)?;
        let layout = self.export(&shaped)?;
        let geometry = Arc::new(geometry::ParleyGeometry::new(shaped, request.text));
        Ok(InteractiveTextLayout { layout, geometry })
    }
}
