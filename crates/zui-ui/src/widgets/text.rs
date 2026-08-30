use crate::{
    event::{EventContext, EventResult, UiEvent},
    layout::Constraints,
    theme::Theme,
    widget::{build_render_node_with_commands, Widget, WidgetId},
};
use zui_core::{Color, Dip, Point, Rect, Size};
use zui_render::ClipShape;

/// Requested weight for a [`Text`] run.
///
/// The current software renderer selects its system fallback glyphs by
/// character. The value is retained by `Text` now so the public API remains
/// stable when font-face selection is added to the renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontWeight {
    Light,
    #[default]
    Normal,
    Medium,
    Semibold,
    Bold,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextVerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextWrap {
    /// Preserve each explicit newline but never insert a line break.
    NoWrap,
    /// Prefer wrapping at whitespace, falling back to character wrapping for
    /// an individual word wider than the available line.
    #[default]
    Word,
    /// Wrap at every Unicode scalar value.
    Character,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Clone, Debug)]
struct Line {
    text: String,
    width: Dip,
    metrics: zui_render::TextRunMetrics,
}

/// A styled text widget with multiline layout and overflow handling.
pub struct Text {
    id: WidgetId,
    text: String,
    font_family: Option<String>,
    font_size: Option<u32>,
    font_weight: FontWeight,
    color: Option<Color>,
    line_height: Option<Dip>,
    wrap: TextWrap,
    overflow: TextOverflow,
    max_lines: Option<usize>,
    align: TextAlign,
    vertical_align: TextVerticalAlign,
    lines: Vec<Line>,
    bounds: Rect,
    theme: Theme,
}

impl Text {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            id: WidgetId::new(),
            lines: vec![Line {
                width: Dip::ZERO,
                text: text.clone(),
                metrics: zui_render::text_run_metrics(&text, Theme::default().text.font_size),
            }],
            text,
            font_family: None,
            font_size: None,
            font_weight: FontWeight::Normal,
            color: None,
            line_height: None,
            wrap: TextWrap::Word,
            overflow: TextOverflow::Clip,
            max_lines: None,
            align: TextAlign::Start,
            vertical_align: TextVerticalAlign::Top,
            bounds: Rect::default(),
            theme: Theme::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.font_family = Some(family.into());
        self
    }
    pub fn font_size(mut self, size: u32) -> Self {
        self.font_size = Some(size.max(1));
        self
    }
    pub fn font_weight(mut self, weight: FontWeight) -> Self {
        self.font_weight = weight;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// Sets the baseline-to-baseline distance in DIPs.
    pub fn line_height(mut self, height: f32) -> Self {
        self.line_height = Some(Dip(height.max(1.0)));
        self
    }
    pub fn wrap(mut self, wrap: TextWrap) -> Self {
        self.wrap = wrap;
        self
    }
    pub fn overflow(mut self, overflow: TextOverflow) -> Self {
        self.overflow = overflow;
        self
    }
    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = Some(max_lines);
        self
    }
    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }
    pub fn vertical_align(mut self, align: TextVerticalAlign) -> Self {
        self.vertical_align = align;
        self
    }
    pub fn ellipsis(mut self) -> Self {
        self.overflow = TextOverflow::Ellipsis;
        self
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// Updates an explicit foreground color for composite controls.
    pub fn set_color(&mut self, color: Color) {
        self.color = Some(color);
    }

    /// Updates an explicit scale for composite controls.
    pub fn set_font_size(&mut self, size: u32) {
        self.font_size = Some(size.max(1));
    }

    pub fn requested_font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }
    pub fn requested_font_weight(&self) -> FontWeight {
        self.font_weight
    }

    fn font_size_value(&self) -> u32 {
        self.font_size.unwrap_or(self.theme.text.font_size).max(1)
    }
    fn line_height_value(&self) -> Dip {
        self.line_height
            .unwrap_or(zui_render::text_metrics(self.font_size_value()).line_height)
    }
    fn color_value(&self) -> Color {
        self.color.unwrap_or(self.theme.text.color)
    }

    fn layout_lines(&self, max_width: Dip) -> Vec<Line> {
        let width_is_bounded = max_width.0.is_finite();
        let max_width = Dip(max_width.0.max(0.0));
        let mut lines = Vec::new();
        for paragraph in self.text.split('\n') {
            match self.wrap {
                TextWrap::NoWrap | _ if !width_is_bounded => lines.push(self.line(paragraph)),
                TextWrap::Character => lines.extend(self.wrap_characters(paragraph, max_width)),
                TextWrap::Word => lines.extend(self.wrap_words(paragraph, max_width)),
                TextWrap::NoWrap => lines.push(self.line(paragraph)),
            }
        }
        if lines.is_empty() {
            lines.push(self.line(""));
        }
        self.apply_max_lines(lines, max_width)
    }

    fn line(&self, text: impl Into<String>) -> Line {
        let text = text.into();
        Line {
            width: zui_render::measure_text(&text, self.font_size_value()),
            metrics: zui_render::text_run_metrics(&text, self.font_size_value()),
            text,
        }
    }

    fn wrap_characters(&self, text: &str, max_width: Dip) -> Vec<Line> {
        if text.is_empty() {
            return vec![self.line("")];
        }
        let mut lines = Vec::new();
        let mut current = String::new();
        for character in text.chars() {
            let mut candidate = current.clone();
            candidate.push(character);
            if !current.is_empty()
                && zui_render::measure_text(&candidate, self.font_size_value()).0 > max_width.0
            {
                lines.push(self.line(current));
                current = character.to_string();
            } else {
                current = candidate;
            }
        }
        lines.push(self.line(current));
        lines
    }

    fn wrap_words(&self, text: &str, max_width: Dip) -> Vec<Line> {
        if text.is_empty() {
            return vec![self.line("")];
        }
        let mut lines = Vec::new();
        let mut current = String::new();
        for word in text.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_owned()
            } else {
                format!("{current} {word}")
            };
            if !current.is_empty()
                && zui_render::measure_text(&candidate, self.font_size_value()).0 > max_width.0
            {
                lines.push(self.line(current));
                if zui_render::measure_text(word, self.font_size_value()).0 > max_width.0 {
                    let mut parts = self.wrap_characters(word, max_width);
                    current = parts
                        .pop()
                        .expect("character wrapping returns one line")
                        .text;
                    lines.extend(parts);
                } else {
                    current = word.to_owned();
                }
            } else {
                current = candidate;
            }
        }
        if !current.is_empty() {
            lines.push(self.line(current));
        }
        lines
    }

    fn apply_max_lines(&self, mut lines: Vec<Line>, max_width: Dip) -> Vec<Line> {
        let limit = self.max_lines.unwrap_or(usize::MAX);
        let truncated = lines.len() > limit;
        lines.truncate(limit);
        if (truncated
            || (self.wrap == TextWrap::NoWrap
                && lines.first().is_some_and(|line| line.width.0 > max_width.0)))
            && self.overflow == TextOverflow::Ellipsis
            && !lines.is_empty()
        {
            let last = lines.last_mut().expect("checked above");
            *last = self.ellipsized(&last.text, max_width, truncated);
        }
        lines
    }

    fn ellipsized(&self, text: &str, max_width: Dip, force: bool) -> Line {
        const ELLIPSIS: &str = "…";
        if !force && zui_render::measure_text(text, self.font_size_value()).0 <= max_width.0 {
            return self.line(text);
        }
        let ellipsis_width = zui_render::measure_text(ELLIPSIS, self.font_size_value()).0;
        if ellipsis_width > max_width.0 {
            return self.line("");
        }
        let mut value = String::new();
        for character in text.chars() {
            let mut candidate = value.clone();
            candidate.push(character);
            candidate.push_str(ELLIPSIS);
            if zui_render::measure_text(&candidate, self.font_size_value()).0 > max_width.0 {
                break;
            }
            value.push(character);
        }
        value.push_str(ELLIPSIS);
        self.line(value)
    }

    fn build_render_commands(&self, ctx: &mut crate::PaintContext<'_>) {
        let line_height = self.line_height_value().0;
        let content_height = line_height * self.lines.len() as f32;
        let top = match self.vertical_align {
            TextVerticalAlign::Top => self.bounds.origin.y.0,
            TextVerticalAlign::Center => {
                self.bounds.origin.y.0 + (self.bounds.size.height.0 - content_height).max(0.0) / 2.0
            }
            TextVerticalAlign::Bottom => {
                self.bounds.origin.y.0 + (self.bounds.size.height.0 - content_height).max(0.0)
            }
        };
        for (index, line) in self.lines.iter().enumerate() {
            let x = match self.align {
                TextAlign::Start => self.bounds.origin.x.0,
                TextAlign::Center => {
                    self.bounds.origin.x.0
                        + (self.bounds.size.width.0 - line.width.0).max(0.0) / 2.0
                }
                TextAlign::End => {
                    self.bounds.origin.x.0 + (self.bounds.size.width.0 - line.width.0).max(0.0)
                }
            };
            ctx.draw_text(
                &line.text,
                Point {
                    x: Dip(x),
                    y: Dip(top
                        + index as f32 * line_height
                        + (line_height - line.metrics.ink_height().0).max(0.0) / 2.0
                        - line.metrics.ink_top.0),
                },
                self.color_value(),
                self.font_size_value(),
            );
        }
    }
}

impl Widget for Text {
    fn id(&self) -> WidgetId {
        self.id
    }
    fn bounds(&self) -> Rect {
        self.bounds
    }
    fn arrange(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }
    fn measure(&mut self, constraints: Constraints) -> Size {
        self.lines = self.layout_lines(constraints.max.width);
        let line_height = self.line_height_value();
        let intrinsic = Size {
            width: Dip(self
                .lines
                .iter()
                .map(|line| line.width.0)
                .fold(0.0, f32::max)),
            height: Dip(line_height.0 * self.lines.len() as f32),
        };
        let size = constraints.constrain(intrinsic);
        self.bounds.size = size;
        size
    }
    fn event(&mut self, _event: &UiEvent, _ctx: &mut EventContext) -> EventResult {
        EventResult::Ignored
    }
    fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
    }
    fn build_render_node(&self, theme: &Theme) -> zui_render::RenderNode {
        let mut node = build_render_node_with_commands(self.id, self.bounds, theme, |ctx| {
            self.build_render_commands(ctx)
        });
        node.set_clip(Some(ClipShape::Rect(Rect {
            origin: Point::default(),
            size: self.bounds.size,
        })));
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loose(width: f32) -> Constraints {
        Constraints::loose(Size {
            width: Dip(width),
            height: Dip(500.0),
        })
    }

    #[test]
    fn word_wrap_respects_the_available_width() {
        let mut text = Text::new("alpha beta gamma")
            .font_size(1)
            .wrap(TextWrap::Word);
        text.measure(loose(38.0));
        assert!(text.lines.len() > 1);
        assert!(text.lines.iter().all(|line| line.width.0 <= 38.0));
    }

    #[test]
    fn ellipsis_is_applied_when_max_lines_truncates_content() {
        let mut text = Text::new("alpha beta gamma delta")
            .font_size(1)
            .max_lines(1)
            .ellipsis();
        text.measure(loose(42.0));
        assert_eq!(text.lines.len(), 1);
        assert!(text.lines[0].text.ends_with('…'));
    }

    #[test]
    fn custom_line_height_controls_measured_height() {
        let mut text = Text::new("one\ntwo").font_size(1).line_height(20.0);
        assert_eq!(text.measure(loose(200.0)).height, Dip(40.0));
    }
}
