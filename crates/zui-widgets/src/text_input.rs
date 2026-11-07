//! Single-line editor. Local draft/history are distinct from external View snapshots.
mod caret;
mod composition;
mod draft;
mod draft_cache;
mod mask;
use crate::TextView;
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use caret::CaretBlink;
use composition::Composition;
use core::time::Duration;
pub use draft::{TextInputDraft, TextInputDraftMemory};
pub use draft_cache::{
    DraftCacheError, DraftCachePolicy, DraftCacheReport, DraftCacheUsage, TextInputDraftCache,
};
use zui_api::{
    action::{ActionId, WidgetAction},
    clipboard::{Clipboard, ClipboardError, UnavailableClipboard},
    event::*,
    layout::{AvailableSpace, LayoutError},
    text::{
        InteractiveTextLayout, TextAffinity, TextError, TextGeometryError, TextPosition,
        TextRequest, TextService, TextStyle, TextWidth, VisualDirection,
    },
    view::ViewSpec,
    widget::{
        AnimationUpdate, Invalidation, MeasureContext, PaintContext, PrepareContext,
        TextInputState, Widget,
    },
};
use zui_edit::{EditError, EditLimits, Movement, Selection, TextBuffer};
use zui_paint::{Painter, Path};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Affine, Color, NodeId, Point, Rect, Size};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputError {
    DraftInUse,
    InvalidSingleLineText,
    InvalidPreeditCursor,
    Edit(EditError),
    Geometry(TextGeometryError),
    Clipboard(ClipboardError),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
    Changed(String),
    Submitted(String),
    Rejected(TextInputError),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInputStyle {
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
    pub focus_border: Color,
    pub invalid_border: Color,
    pub selection: Color,
    pub caret: Color,
    pub disabled_foreground: Color,
    pub natural_width: f32,
}
impl TextInputStyle {
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            background: theme.color(ColorRole::Surface),
            foreground: theme.color(ColorRole::Text),
            border: theme.color(ColorRole::Border),
            focus_border: theme.color(ColorRole::Accent),
            invalid_border: theme.color(ColorRole::Danger),
            selection: theme.color(ColorRole::SurfaceRaised),
            caret: theme.color(ColorRole::Accent),
            disabled_foreground: theme.color(ColorRole::TextMuted),
            natural_width: 240.0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TextInputView {
    /// Optional application-owned committed editing state, exclusively attached.
    pub draft: Option<TextInputDraft>,
    /// External snapshot. Unchanged snapshots do not overwrite locally edited text.
    pub text: String,
    /// Change to force replacement even if the external string stayed unchanged.
    pub revision: u64,
    pub text_style: TextStyle,
    pub style: TextInputStyle,
    pub action: ActionId,
    pub disabled: bool,
    pub read_only: bool,
    /// Displayed only when both committed text and IME preedit are empty.
    pub placeholder: String,
    /// Application-owned validation state; never rejects edits implicitly.
    pub invalid: bool,
    /// Mask each grapheme in rendering and suppress copy/cut. Model/history remain plaintext.
    pub password: bool,
    pub limits: EditLimits,
    /// Duration of each visible/hidden phase. None keeps the caret visible.
    pub caret_blink_interval: Option<Duration>,
}
impl TextInputView {
    pub fn themed(
        theme: &Theme,
        text: impl Into<String>,
        families: Vec<String>,
        action: ActionId,
    ) -> Self {
        let label = TextView::themed(theme, text, families, TypeRole::Body);
        Self {
            draft: None,
            text: label.text,
            revision: 0,
            text_style: label.style,
            style: TextInputStyle::from_theme(theme),
            action,
            disabled: false,
            read_only: false,
            placeholder: String::new(),
            invalid: false,
            password: false,
            limits: EditLimits::default(),
            caret_blink_interval: Some(Duration::from_millis(500)),
        }
    }
    pub fn with_draft(mut self, draft: TextInputDraft) -> Self {
        self.draft = Some(draft);
        self
    }
    fn buffer(&self) -> Result<TextBuffer, TextInputError> {
        single_line(&self.text)?;
        TextBuffer::new(self.text.clone(), self.limits).map_err(TextInputError::Edit)
    }
}
fn single_line(text: &str) -> Result<(), TextInputError> {
    if text
        .chars()
        .any(|c| c.is_control() || c == '\u{2028}' || c == '\u{2029}')
    {
        Err(TextInputError::InvalidSingleLineText)
    } else {
        Ok(())
    }
}
pub struct TextInput {
    draft_attached: bool,
    properties: TextInputView,
    buffer: Result<TextBuffer, TextInputError>,
    prepared: Option<InteractiveTextLayout>,
    hint: Option<zui_api::text::TextLayout>,
    mask: Option<mask::Mask>,
    selection_rects: Vec<Rect>,
    preedit_rects: Vec<Rect>,
    composition: Option<Composition>,
    caret: Rect,
    anchor_affinity: TextAffinity,
    focus_affinity: TextAffinity,
    scroll: f64,
    viewport: f64,
    focused: bool,
    dragging: bool,
    ime_generation: u64,
    blink: CaretBlink,
}
impl TextInput {
    pub fn text(&self) -> Option<&str> {
        self.buffer.as_ref().ok().map(TextBuffer::text)
    }
    /// Whether an IME currently owns the visible preedit and candidate keys.
    pub fn is_composing(&self) -> bool {
        self.composition.is_some()
    }
    pub fn selection(&self) -> Option<Selection> {
        self.buffer.as_ref().ok().map(TextBuffer::selection)
    }
    pub fn horizontal_scroll(&self) -> f64 {
        self.scroll
    }
    fn reset_affinity(&mut self) {
        self.anchor_affinity = TextAffinity::Downstream;
        self.focus_affinity = TextAffinity::Downstream;
    }
    fn cancel_composition(&mut self) -> bool {
        if self.composition.take().is_some() {
            self.prepared = None;
            self.preedit_rects.clear();
            true
        } else {
            false
        }
    }
    fn text_origin(&self, context: PaintContext, height: f64) -> Point {
        Point::new(
            context.content_bounds.origin.x - self.scroll,
            context.content_bounds.origin.y
                + ((context.content_bounds.size.height - height) / 2.0).max(0.0),
        )
    }
    fn validate(&self, id: NodeId) -> Result<(), LayoutError> {
        if single_line(&self.properties.placeholder).is_err()
            || self.buffer.is_err()
            || self
                .properties
                .caret_blink_interval
                .is_some_and(|interval| interval.is_zero())
            || !self.properties.style.natural_width.is_finite()
            || self.properties.style.natural_width <= 0.0
        {
            return Err(LayoutError::InvalidStyle(id));
        }
        Ok(())
    }
    fn layout_text(
        &mut self,
        service: &mut dyn TextService,
        id: NodeId,
    ) -> Result<(), LayoutError> {
        self.validate(id)?;
        if self
            .hint
            .as_ref()
            .is_none_or(|h| h.font_epoch != service.font_epoch())
        {
            self.hint = Some(
                service
                    .layout(TextRequest {
                        text: &self.properties.placeholder,
                        style: &self.properties.text_style,
                        width: TextWidth::Unbounded,
                    })
                    .map_err(|e| LayoutError::Text(id, e))?,
            );
        }
        if self
            .prepared
            .as_ref()
            .is_none_or(|layout| layout.layout.font_epoch != service.font_epoch())
        {
            let source = self.composition.as_ref().map_or_else(
                || self.buffer.as_ref().expect("validated buffer").text(),
                |composition| composition.text.as_str(),
            );
            self.mask = self.properties.password.then(|| mask::Mask::new(source));
            let display = self.mask.as_ref().map_or(source, |mask| mask.text.as_str());
            self.prepared = Some(
                service
                    .layout_interactive(TextRequest {
                        text: display,
                        style: &self.properties.text_style,
                        width: TextWidth::Unbounded,
                    })
                    .map_err(|e| LayoutError::Text(id, e))?,
            );
        }
        self.refresh_visual()
            .map_err(|e| LayoutError::Text(id, TextError::Geometry(e)))
    }
    fn refresh_visual(&mut self) -> Result<(), TextGeometryError> {
        let Some(layout) = &self.prepared else {
            return Ok(());
        };
        let selection = self.composition.as_ref().map_or_else(
            || self.buffer.as_ref().expect("prepared buffer").selection(),
            |composition| composition.selection,
        );
        let display = |i| self.mask.as_ref().map_or(i, |mask| mask.display(i));
        let anchor = TextPosition {
            byte_index: display(selection.anchor),
            affinity: self.anchor_affinity,
        };
        let focus = TextPosition {
            byte_index: display(selection.focus),
            affinity: self.focus_affinity,
        };
        self.caret = layout.geometry.caret(focus, 1.5)?;
        self.selection_rects = layout.geometry.selection_rects(anchor, focus)?;
        self.preedit_rects.clear();
        if let Some(composition) = &self.composition {
            self.preedit_rects = layout.geometry.selection_rects(
                TextPosition {
                    byte_index: display(composition.range.start),
                    affinity: TextAffinity::Downstream,
                },
                TextPosition {
                    byte_index: display(composition.range.end),
                    affinity: TextAffinity::Upstream,
                },
            )?;
        }
        if self.caret.origin.x < self.scroll {
            self.scroll = self.caret.origin.x;
        }
        if self.caret.origin.x + self.caret.size.width > self.scroll + self.viewport {
            self.scroll = self.caret.origin.x + self.caret.size.width - self.viewport;
        }
        self.scroll = self.scroll.clamp(
            0.0,
            (layout.layout.size.width + self.caret.size.width - self.viewport).max(0.0),
        );
        Ok(())
    }
    fn notify(&self, response: &mut EventResponse, event: TextInputEvent) {
        response
            .actions
            .push(WidgetAction::new(self.properties.action, event));
    }
    fn reject(&self, response: &mut EventResponse, error: TextInputError) {
        self.notify(response, TextInputEvent::Rejected(error));
    }
    fn insert(&mut self, text: &str) -> Result<bool, TextInputError> {
        single_line(text)?;
        self.buffer
            .as_mut()
            .map_err(|e| e.clone())?
            .replace_selection(text)
            .map_err(TextInputError::Edit)
    }
    fn clipboard_command(
        &mut self,
        command: &str,
        clipboard: &mut dyn Clipboard,
    ) -> Result<(), TextInputError> {
        if self.properties.password && matches!(command, "c" | "x") {
            return Ok(());
        }
        let buffer = self.buffer.as_mut().map_err(|e| e.clone())?;
        match command {
            "c" | "x" => {
                if buffer.selection().is_caret() || (command == "x" && self.properties.read_only) {
                    return Ok(());
                }
                clipboard
                    .write_text(buffer.selected_text())
                    .map_err(TextInputError::Clipboard)?;
                if command == "x" {
                    buffer.replace_selection("").map_err(TextInputError::Edit)?;
                }
            }
            "v" if !self.properties.read_only => {
                let remaining = buffer.text().len() - buffer.selected_text().len();
                let text = clipboard
                    .read_text(self.properties.limits.max_text_bytes - remaining)
                    .map_err(TextInputError::Clipboard)?;
                if !text.is_empty() {
                    self.insert(&text)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn hit(&mut self, context: EventContext, extend: bool) -> Result<(), TextInputError> {
        let (Some(point), Some(geometry), Some(layout)) =
            (context.local_position, context.geometry, &self.prepared)
        else {
            return Ok(());
        };
        let mut hit = layout
            .geometry
            .cursor_at(Point::new(
                point.x - geometry.content_bounds.origin.x + self.scroll,
                layout.layout.size.height / 2.0,
            ))
            .map_err(TextInputError::Geometry)?;
        hit.byte_index = self
            .mask
            .as_ref()
            .map_or(hit.byte_index, |mask| mask.original(hit.byte_index));
        let buffer = self.buffer.as_mut().map_err(|e| e.clone())?;
        buffer
            .set_selection(Selection {
                anchor: if extend {
                    buffer.selection().anchor
                } else {
                    hit.byte_index
                },
                focus: hit.byte_index,
            })
            .map_err(TextInputError::Edit)?;
        self.focus_affinity = hit.affinity;
        if !extend {
            self.anchor_affinity = hit.affinity;
        }
        Ok(())
    }
    fn navigate(&mut self, key: &KeyInput) -> Result<(), TextInputError> {
        let buffer = self.buffer.as_mut().map_err(|e| e.clone())?;
        let selection = buffer.selection();
        let display = |i| self.mask.as_ref().map_or(i, |mask| mask.display(i));
        match key.key {
            Key::Home => {
                buffer.move_cursor(Movement::TextStart, key.modifiers.shift);
                self.focus_affinity = TextAffinity::Downstream;
            }
            Key::End => {
                buffer.move_cursor(Movement::TextEnd, key.modifiers.shift);
                self.focus_affinity = TextAffinity::Upstream;
            }
            Key::ArrowLeft | Key::ArrowRight => {
                let Some(layout) = &self.prepared else {
                    return Ok(());
                };
                let right = key.key == Key::ArrowRight;
                let mut position = if !key.modifiers.shift && !selection.is_caret() {
                    // Collapse to the visual edge, not the lower logical byte index.
                    let anchor = TextPosition {
                        byte_index: display(selection.anchor),
                        affinity: self.anchor_affinity,
                    };
                    let focus = TextPosition {
                        byte_index: display(selection.focus),
                        affinity: self.focus_affinity,
                    };
                    let a = layout
                        .geometry
                        .caret(anchor, 0.0)
                        .map_err(TextInputError::Geometry)?
                        .origin
                        .x;
                    let f = layout
                        .geometry
                        .caret(focus, 0.0)
                        .map_err(TextInputError::Geometry)?
                        .origin
                        .x;
                    if (right && a > f) || (!right && a < f) {
                        anchor
                    } else {
                        focus
                    }
                } else {
                    layout
                        .geometry
                        .move_visual(
                            TextPosition {
                                byte_index: display(selection.focus),
                                affinity: self.focus_affinity,
                            },
                            if right {
                                VisualDirection::Right
                            } else {
                                VisualDirection::Left
                            },
                        )
                        .map_err(TextInputError::Geometry)?
                };
                position.byte_index = self.mask.as_ref().map_or(position.byte_index, |mask| {
                    mask.original(position.byte_index)
                });
                buffer
                    .set_selection(Selection {
                        anchor: if key.modifiers.shift {
                            selection.anchor
                        } else {
                            position.byte_index
                        },
                        focus: position.byte_index,
                    })
                    .map_err(TextInputError::Edit)?;
                self.focus_affinity = position.affinity;
            }
            _ => {}
        }
        if !key.modifiers.shift {
            self.anchor_affinity = self.focus_affinity;
        }
        Ok(())
    }
}
impl ViewSpec for TextInputView {
    type Widget = TextInput;
    fn build(&self) -> TextInput {
        let mut widget = TextInput {
            draft_attached: false,
            properties: self.clone(),
            buffer: self.buffer(),
            prepared: None,
            hint: None,
            mask: None,
            selection_rects: Vec::new(),
            preedit_rects: Vec::new(),
            composition: None,
            caret: Rect::default(),
            anchor_affinity: TextAffinity::Downstream,
            focus_affinity: TextAffinity::Downstream,
            scroll: 0.0,
            viewport: 0.0,
            focused: false,
            dragging: false,
            ime_generation: 0,
            blink: CaretBlink::default(),
        };
        widget.attach_draft();
        widget
    }
    fn update(&self, widget: &mut TextInput) -> Invalidation {
        if self.draft != widget.properties.draft || (self.draft.is_some() && !widget.draft_attached)
        {
            widget.release_draft();
            widget.cancel_composition();
            widget.ime_generation = widget.ime_generation.wrapping_add(1);
            widget.buffer = self.buffer();
            widget.reset_affinity();
            widget.scroll = 0.0;
            widget.dragging = false;
            widget.prepared = None;
            widget.blink.reset();
            widget.properties = self.clone();
            widget.hint = None;
            widget.attach_draft();
            return Invalidation::Layout;
        }

        if self.placeholder != widget.properties.placeholder
            || self.text_style != widget.properties.text_style
        {
            widget.hint = None;
        }
        let old = &widget.properties;
        let forced = self.revision != old.revision || self.limits != old.limits;
        let replacement = self.text != old.text && widget.text() != Some(self.text.as_str());
        let mut layout = self.password != old.password
            || self.placeholder != old.placeholder
            || forced
            || replacement
            || self.text_style != old.text_style
            || self.style.natural_width != old.style.natural_width;
        let changed = self != old;
        let reset_blink = forced
            || replacement
            || self.caret_blink_interval != old.caret_blink_interval
            || self.disabled != old.disabled
            || self.read_only != old.read_only
            || self.password != old.password;
        if forced
            || replacement
            || self.disabled != old.disabled
            || self.read_only != old.read_only
            || self.password != old.password
            || self.action != old.action
        {
            layout |= widget.cancel_composition();
            widget.ime_generation = widget.ime_generation.wrapping_add(1);
        }
        if forced || replacement {
            widget.buffer = self.buffer();
            widget.reset_affinity();
            widget.scroll = 0.0;
            widget.dragging = false;
        }
        if self.disabled
            || self.read_only != widget.properties.read_only
            || self.action != widget.properties.action
        {
            widget.dragging = false;
        }
        if layout {
            widget.prepared = None;
        }
        widget.properties = self.clone();
        if reset_blink {
            widget.blink.reset();
        }
        if layout {
            Invalidation::Layout
        } else if changed {
            Invalidation::Paint
        } else {
            Invalidation::None
        }
    }
}
impl Widget for TextInput {
    fn animate(&mut self, elapsed: Duration) -> AnimationUpdate {
        let active = self.focused
            && self.enabled()
            && !self.properties.read_only
            && self.composition.is_none()
            && self
                .selection()
                .is_some_and(|selection| selection.is_caret());
        self.blink
            .advance(elapsed, self.properties.caret_blink_interval, active)
    }
    fn enabled(&self) -> bool {
        !self.properties.disabled
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn text_input(&self, context: PaintContext) -> Option<TextInputState> {
        if !self.focused || !self.enabled() || self.properties.read_only {
            return None;
        }
        let layout = self.prepared.as_ref()?;
        let content = context.content_bounds;
        if content.size.width <= 0.0 || content.size.height <= 0.0 {
            return None;
        }
        let origin = self.text_origin(context, layout.layout.size.height);
        let left = (origin.x + self.caret.origin.x)
            .clamp(content.origin.x, content.origin.x + content.size.width);
        let top = (origin.y + self.caret.origin.y).max(content.origin.y);
        let right = (origin.x + self.caret.origin.x + self.caret.size.width)
            .clamp(left, content.origin.x + content.size.width);
        let bottom = (origin.y + self.caret.origin.y + self.caret.size.height)
            .min(content.origin.y + content.size.height);
        (bottom > top).then_some(TextInputState {
            generation: self.ime_generation,
            cursor_area: Rect::new(left, top, right - left, bottom - top),
        })
    }
    fn measure(&mut self, context: MeasureContext) -> Result<Size, LayoutError> {
        self.layout_text(context.text, context.id)?;
        let width = match context.input.available_width {
            AvailableSpace::Definite(w) => self.properties.style.natural_width.min(w),
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => self.properties.style.natural_width,
        };
        Ok(Size::new(
            context.input.known_width.unwrap_or(width) as f64,
            context
                .input
                .known_height
                .map(f64::from)
                .unwrap_or(self.prepared.as_ref().unwrap().layout.size.height),
        ))
    }
    fn prepare(&mut self, context: PrepareContext) -> Result<(), LayoutError> {
        self.viewport = context.content_size.width;
        self.layout_text(context.text, context.id)
    }
    fn paint(&self, context: PaintContext, painter: &mut Painter<'_>) {
        let s = self.properties.style;
        painter.fill_rect(
            Rect::new(0.0, 0.0, context.size.width, context.size.height),
            s.background,
        );
        if context.size.width > 2.0 && context.size.height > 2.0 {
            painter.stroke(
                Path::rectangle(Rect::new(
                    0.5,
                    0.5,
                    context.size.width - 1.0,
                    context.size.height - 1.0,
                )),
                if self.properties.invalid {
                    s.invalid_border
                } else if self.focused && self.enabled() {
                    s.focus_border
                } else {
                    s.border
                },
                1.0,
            );
        }
        if self.text() == Some("")
            && self.composition.is_none()
            && let Some(hint) = &self.hint
        {
            let origin = self.text_origin(context, hint.size.height);
            painter.with_clip(Path::rectangle(context.content_bounds), |p| {
                p.with_transform(Affine::translate(origin.x, origin.y), |p| {
                    hint.paint(p, s.disabled_foreground)
                });
            });
        }
        if let Some(layout) = &self.prepared {
            let origin = self.text_origin(context, layout.layout.size.height);
            painter.with_clip(Path::rectangle(context.content_bounds), |painter| {
                painter.with_transform(Affine::translate(origin.x, origin.y), |painter| {
                    if self.focused {
                        for rect in &self.selection_rects {
                            painter.fill_rect(*rect, s.selection);
                        }
                    }
                    layout.layout.paint(
                        painter,
                        if self.enabled() {
                            s.foreground
                        } else {
                            s.disabled_foreground
                        },
                    );
                    for rect in &self.preedit_rects {
                        painter.fill_rect(
                            Rect::new(
                                rect.origin.x,
                                rect.origin.y + rect.size.height - 1.0,
                                rect.size.width,
                                1.0,
                            ),
                            s.caret,
                        );
                    }
                    if self.focused
                        && self.blink.visible
                        && self.enabled()
                        && !self.properties.read_only
                        && self.composition.as_ref().map_or_else(
                            || self.selection().is_some_and(|s| s.is_caret()),
                            |composition| {
                                composition.show_caret && composition.selection.is_caret()
                            },
                        )
                    {
                        painter.fill_rect(self.caret, s.caret);
                    }
                })
            });
        }
    }
    fn event(&mut self, context: EventContext, event: &WidgetEvent) -> EventResponse {
        self.handle_event(context, event, &mut UnavailableClipboard)
    }
    fn event_with_services(
        &mut self,
        context: EventContext,
        event: &WidgetEvent,
        services: &mut EventServices<'_>,
    ) -> EventResponse {
        self.handle_event(context, event, services.clipboard)
    }
}
impl TextInput {
    fn handle_event(
        &mut self,
        context: EventContext,
        event: &WidgetEvent,
        clipboard: &mut dyn Clipboard,
    ) -> EventResponse {
        let mut response = EventResponse::default();
        let mut display_changed = false;
        // Pointer movement and focus notifications must not copy the whole buffer.
        let may_edit = match event {
            WidgetEvent::Input(InputEvent::Key(key)) => matches!(
                key.key,
                Key::Character(_) | Key::Space | Key::Backspace | Key::Delete
            ),
            WidgetEvent::Input(InputEvent::ImeCommit(_)) => true,
            _ => false,
        };
        let old_text = may_edit.then(|| self.text().map(ToString::to_string));
        let old_selection = self.selection();
        let old_state = (
            self.focused,
            self.dragging,
            self.anchor_affinity,
            self.focus_affinity,
        );
        let mut result: Result<(), TextInputError> = Ok(());
        match event {
            WidgetEvent::FocusChanged(focused) => {
                if self.focused && !focused {
                    self.ime_generation = self.ime_generation.wrapping_add(1);
                }
                self.focused = *focused;
                if !focused {
                    self.dragging = false;
                    display_changed |= self.cancel_composition();
                }
            }
            WidgetEvent::PointerCaptureLost => self.dragging = false,
            _ => {}
        }
        if context.phase == EventPhase::Target && self.enabled() {
            match event {
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed,
                    ..
                }) => {
                    response.handled = true;
                    // The visible preedit has different byte indices from committed text.
                    // Leave pointer selection to the IME while it owns the composition.
                    if self.composition.is_some() {
                        return response;
                    }
                    if *pressed {
                        result = self.hit(context, context.modifiers.shift);
                        self.dragging = true;
                        response.focus = FocusRequest::Focus;
                        response.capture = CaptureRequest::Capture;
                    } else {
                        if self.dragging {
                            result = self.hit(context, true);
                        }
                        self.dragging = false;
                        response.capture = CaptureRequest::Release;
                    }
                }
                WidgetEvent::Input(InputEvent::PointerMoved(_)) if self.dragging => {
                    response.handled = true;
                    result = self.hit(context, true);
                }
                WidgetEvent::Input(InputEvent::ImeDisabled) => {
                    display_changed |= self.cancel_composition();
                }
                WidgetEvent::Input(InputEvent::ImePreedit { text, cursor })
                    if self.focused && !self.properties.read_only =>
                {
                    response.handled = true;
                    if text.is_empty() {
                        display_changed |= self.cancel_composition();
                    } else {
                        result = (|| {
                            let buffer = self.buffer.as_ref().map_err(Clone::clone)?;
                            let composition = Composition::new(
                                buffer,
                                text,
                                *cursor,
                                self.properties.limits.max_text_bytes,
                            )?;
                            self.composition = Some(composition);
                            self.dragging = false;
                            response.capture = CaptureRequest::Release;
                            self.reset_affinity();
                            display_changed = true;
                            Ok(())
                        })();
                    }
                }
                WidgetEvent::Input(InputEvent::ImeCommit(text))
                    if self.focused && !self.properties.read_only =>
                {
                    display_changed |= self.cancel_composition();
                    response.handled = true;
                    result = self.insert(text).map(|_| ());
                }
                WidgetEvent::Input(InputEvent::Key(key))
                    if self.focused && key.pressed && self.composition.is_none() =>
                {
                    let command =
                        (key.modifiers.control || key.modifiers.super_key) && !key.modifiers.alt;
                    let plain =
                        !key.modifiers.control && !key.modifiers.super_key && !key.modifiers.alt;
                    let word_command = key.modifiers.control
                        && !key.modifiers.super_key
                        && !key.modifiers.alt
                        && matches!(
                            key.key,
                            Key::ArrowLeft | Key::ArrowRight | Key::Backspace | Key::Delete
                        );
                    if word_command {
                        response.handled = true;
                        if let Ok(buffer) = &mut self.buffer {
                            match key.key {
                                Key::ArrowLeft | Key::ArrowRight => {
                                    buffer.move_cursor(
                                        if key.key == Key::ArrowLeft {
                                            Movement::PreviousWord
                                        } else {
                                            Movement::NextWord
                                        },
                                        key.modifiers.shift,
                                    );
                                    self.focus_affinity = TextAffinity::Downstream;
                                    if !key.modifiers.shift {
                                        self.anchor_affinity = self.focus_affinity;
                                    }
                                }
                                Key::Backspace | Key::Delete if !self.properties.read_only => {
                                    result = if key.key == Key::Backspace {
                                        buffer.delete_word_backward()
                                    } else {
                                        buffer.delete_word_forward()
                                    }
                                    .map(|_| ())
                                    .map_err(TextInputError::Edit);
                                }
                                _ => {}
                            }
                        }
                    } else if command && let Key::Character(letter) = &key.key {
                        match letter.to_ascii_lowercase().as_str() {
                            operation @ ("c" | "x" | "v") => {
                                response.handled = true;
                                if !key.repeat {
                                    result = self.clipboard_command(operation, clipboard);
                                }
                            }
                            "a" => {
                                if let Ok(buffer) = &mut self.buffer {
                                    buffer.select_all();
                                    self.reset_affinity();
                                }
                                response.handled = true;
                            }
                            "z" | "y" if !self.properties.read_only => {
                                if let Ok(buffer) = &mut self.buffer {
                                    if letter.eq_ignore_ascii_case("y") || key.modifiers.shift {
                                        buffer.redo();
                                    } else {
                                        buffer.undo();
                                    }
                                    self.reset_affinity();
                                }
                                response.handled = true;
                            }
                            _ => {}
                        }
                    } else if plain {
                        match &key.key {
                            Key::ArrowLeft | Key::ArrowRight | Key::Home | Key::End => {
                                response.handled = true;
                                result = self.navigate(key);
                            }
                            Key::Backspace | Key::Delete if !self.properties.read_only => {
                                response.handled = true;
                                if let Ok(buffer) = &mut self.buffer {
                                    result = if key.key == Key::Backspace {
                                        buffer.delete_backward()
                                    } else {
                                        buffer.delete_forward()
                                    }
                                    .map(|_| ())
                                    .map_err(TextInputError::Edit);
                                }
                            }
                            Key::Enter => {
                                response.handled = true;
                                if !key.repeat
                                    && let Some(text) = self.text()
                                {
                                    self.notify(
                                        &mut response,
                                        TextInputEvent::Submitted(text.to_string()),
                                    );
                                }
                            }
                            Key::Escape if self.dragging => {
                                response.handled = true;
                                self.dragging = false;
                                response.capture = CaptureRequest::Release;
                            }
                            Key::Character(_) | Key::Space if !self.properties.read_only => {
                                if let Some(text) = &key.text {
                                    response.handled = true;
                                    result = self.insert(text).map(|_| ());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if let Err(error) = result {
            self.reject(&mut response, error);
        }
        // Candidate-window keys belong to the composition session, not window shortcuts.
        if self.composition.is_some()
            && context.phase == EventPhase::Target
            && matches!(event, WidgetEvent::Input(InputEvent::Key(_)))
        {
            response.handled = true;
        }
        if !self.enabled()
            && matches!(
                event,
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    ..
                })
            )
        {
            response.handled = true;
        }
        if old_text
            .as_ref()
            .is_some_and(|text| self.text() != text.as_deref())
        {
            self.prepared = None;
            self.reset_affinity();
            response.invalidation = Invalidation::Layout;
            if let Some(text) = self.text() {
                self.notify(&mut response, TextInputEvent::Changed(text.to_string()));
            }
        } else if !display_changed
            && (self.selection() != old_selection
                || old_state
                    != (
                        self.focused,
                        self.dragging,
                        self.anchor_affinity,
                        self.focus_affinity,
                    ))
        {
            response.invalidation = Invalidation::Paint;
            if let Err(error) = self.refresh_visual() {
                self.reject(&mut response, TextInputError::Geometry(error));
                self.prepared = None;
                response.invalidation = Invalidation::Layout;
            }
        }
        if display_changed {
            self.prepared = None;
            response.invalidation = Invalidation::Layout;
        }
        let interaction = matches!(event, WidgetEvent::FocusChanged(_))
            || (response.handled
                && matches!(
                    event,
                    WidgetEvent::Input(InputEvent::Key(KeyInput { pressed: true, .. }))
                        | WidgetEvent::Input(InputEvent::PointerButton { pressed: true, .. })
                        | WidgetEvent::Input(InputEvent::ImeCommit(_))
                ));
        if (interaction || display_changed || self.selection() != old_selection)
            && self.blink.reset()
        {
            response.invalidation = response.invalidation.max(Invalidation::Paint);
        }
        response
    }
}
