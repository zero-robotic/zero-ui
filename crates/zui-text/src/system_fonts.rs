//! Optional OS font discovery stays inside the concrete text adapter.
use crate::{MissingGlyphPolicy, ParleyText, TextLimits};
use zui_api::text::TextError;

impl ParleyText {
    /// Discover system fonts through Fontique's platform adapter. Font data is
    /// loaded on demand; no font file is embedded. Default/new remain isolated.
    pub fn with_system_fonts(limits: TextLimits) -> Self {
        Self::with_system_fonts_and_policy(limits, MissingGlyphPolicy::default())
    }

    /// Like `with_system_fonts`, with an immutable missing-glyph policy.
    pub fn with_system_fonts_and_policy(limits: TextLimits, policy: MissingGlyphPolicy) -> Self {
        Self::with_font_source(limits, policy, true)
    }

    /// Available registered and system family names, sorted and deduplicated.
    /// Enumerates metadata, rather than loading every font into paint resources.
    pub fn font_family_names(&mut self) -> Vec<String> {
        let mut names: Vec<_> = self
            .fonts
            .collection
            .family_names()
            .map(str::to_owned)
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Resolve installed preferred names in order, skipping unavailable names.
    /// If none are available, use the platform's generic sans-serif families.
    /// Script-specific glyph fallback is performed by the shaping engine.
    /// Use the returned names in TextStyle; arbitrary unknown names still error.
    pub fn resolve_families(&mut self, preferred: &[&str]) -> Result<Vec<String>, TextError> {
        let mut ids = Vec::new();
        for name in preferred {
            if let Some(id) = self.fonts.collection.family_id(name)
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            ids.extend(
                self.fonts
                    .collection
                    .generic_families(parley::fontique::GenericFamily::SansSerif),
            );
        }
        let names: Vec<_> = ids
            .into_iter()
            .filter_map(|id| self.fonts.collection.family_name(id).map(str::to_owned))
            .collect();
        if names.is_empty() {
            Err(TextError::NoFonts)
        } else {
            Ok(names)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zui_api::text::{TextAlign, TextRequest, TextService, TextStyle, TextWidth};

    #[test]
    #[ignore = "requires installed system fonts"]
    fn failed_export_does_not_retain_partially_loaded_fonts() {
        let mut text = ParleyText::with_system_fonts(TextLimits {
            max_glyphs: 0,
            ..Default::default()
        });
        let style = TextStyle {
            families: text.resolve_families(&[]).unwrap(),
            font_size: 20.0,
            line_height: 1.2,
            align: TextAlign::Start,
        };
        let request = || TextRequest {
            text: "A",
            style: &style,
            width: TextWidth::Unbounded,
        };
        assert!(matches!(
            text.layout(request()),
            Err(TextError::BudgetExceeded)
        ));
        assert!(text.resources.is_empty());
        assert_eq!(text.font_bytes, 0);
        text.limits.max_glyphs = 1;
        let layout = text.layout(request()).unwrap();
        assert_eq!(text.resources.len(), 1);
        assert_eq!(text.font_bytes, layout.runs[0].font.bytes().len());
    }
}
