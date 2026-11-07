use std::sync::{Arc, OnceLock};
use zui_api::{
    action::ActionId,
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::{Tree, TreeLimits};
use zui_layout::TaffyLayout;
use zui_paint::{Frame, OffscreenRenderer};
use zui_text::ParleyText;
use zui_theme::Theme;
use zui_types::{Color, Extent, Point, Size};
use zui_widgets::{TextInput, TextInputEvent, TextInputView};
struct Fixture {
    tree: Tree,
    text: ParleyText,
    engine: TaffyLayout,
    view: TextInputView,
    width: f32,
}
impl Fixture {
    fn new(value: &str) -> Self {
        static FONT: OnceLock<Arc<[u8]>> = OnceLock::new();
        let mut text = ParleyText::default();
        let families = text
            .register_font(
                FONT.get_or_init(|| {
                    Arc::from(include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice())
                })
                .clone(),
            )
            .unwrap();
        let mut view = TextInputView::themed(&Theme::dashboard(), value, families, ActionId(5));
        view.text_style.font_size = 24.0;
        view.style.selection = Color::rgb(30, 75, 100);
        let tree = Tree::from_view(description(&view, 240.0), TreeLimits::default()).unwrap();
        let mut f = Self {
            tree,
            text,
            engine: Default::default(),
            view,
            width: 240.0,
        };
        f.prepare();
        f
    }
    fn prepare(&mut self) -> bool {
        self.tree
            .layout_with_text(&mut self.engine, &mut self.text, Size::new(300.0, 80.0))
            .unwrap()
    }
    fn update(&mut self) -> bool {
        self.tree
            .reconcile(description(&self.view, self.width))
            .unwrap();
        self.prepare()
    }
    fn input(&mut self, event: InputEvent) -> zui_core::DispatchResult {
        self.prepare();
        self.tree.dispatch_input(event).unwrap()
    }
    fn clipboard_input(
        &mut self,
        event: InputEvent,
        clipboard: &mut dyn zui_api::clipboard::Clipboard,
    ) -> zui_core::DispatchResult {
        self.prepare();
        self.tree
            .dispatch_input_with_services(event, &mut EventServices { clipboard })
            .unwrap()
    }
    fn state(&mut self) -> (String, usize, usize, f64) {
        let mut result = None;
        self.tree
            .update_widget::<TextInput>(self.tree.root(), Invalidation::None, |widget| {
                let selection = widget.selection().unwrap();
                result = Some((
                    widget.text().unwrap().into(),
                    selection.anchor,
                    selection.focus,
                    widget.horizontal_scroll(),
                ));
            })
            .unwrap();
        result.unwrap()
    }
}
fn description(view: &TextInputView, width: f32) -> View {
    View::new(view.clone()).keyed("input").styled(LayoutStyle {
        width: Dimension::Length(width),
        height: Dimension::Length(48.0),
        padding: Edges::all(8.0),
        ..Default::default()
    })
}
fn key(key: Key) -> InputEvent {
    InputEvent::Key(KeyInput {
        key,
        pressed: true,
        repeat: false,
        text: None,
        modifiers: Default::default(),
    })
}
fn typed(text: &str) -> InputEvent {
    InputEvent::Key(KeyInput {
        key: Key::Character(text.into()),
        pressed: true,
        repeat: false,
        text: Some(text.into()),
        modifiers: Default::default(),
    })
}
fn command(letter: &str) -> InputEvent {
    let mut event = key(Key::Character(letter.into()));
    if let InputEvent::Key(k) = &mut event {
        k.modifiers.control = true;
    }
    event
}
fn mouse(pressed: bool, x: f64) -> InputEvent {
    InputEvent::PointerButton {
        button: PointerButton::Primary,
        pressed,
        position: Some(Point::new(x, 24.0)),
    }
}
fn changed(result: zui_core::DispatchResult) -> String {
    assert_eq!(result.actions.len(), 1);
    assert_eq!(result.actions[0].action.id, ActionId(5));
    match result.actions[0]
        .action
        .payload::<TextInputEvent>()
        .unwrap()
    {
        TextInputEvent::Changed(text) => text.clone(),
        other => panic!("{other:?}"),
    }
}
#[test]
fn unicode_typing_delete_undo_and_external_acknowledgement_preserve_history() {
    let mut f = Fixture::new("中");
    f.input(key(Key::Tab));
    assert_eq!(changed(f.input(typed("e\u{301}"))), "中e\u{301}");
    f.view.text = "中e\u{301}".into();
    f.update(); // acknowledgement
    assert_eq!(changed(f.input(key(Key::Backspace))), "中");
    assert_eq!(changed(f.input(command("z"))), "中e\u{301}");
    assert_eq!(changed(f.input(command("z"))), "中");
    assert_eq!(changed(f.input(command("y"))), "中e\u{301}");
    f.input(command("a"));
    assert_eq!(changed(f.input(typed("替换"))), "替换");
    let result = f.input(key(Key::Enter));
    assert!(
        matches!(result.actions[0].action.payload::<TextInputEvent>(), Some(TextInputEvent::Submitted(text)) if text == "替换")
    );
}
#[test]
fn unchanged_views_keep_drafts_and_revision_forces_a_new_session() {
    let mut f = Fixture::new("a");
    f.input(key(Key::Tab));
    f.input(typed("b"));
    f.update();
    assert_eq!(f.state().0, "ab");
    f.view.style.foreground = Color::WHITE;
    f.update();
    assert_eq!(f.state().0, "ab");
    f.view.revision += 1;
    f.update();
    assert_eq!(f.state().0, "a");
    assert!(f.input(command("z")).actions.is_empty());
    f.view.text = "外部".into();
    f.update();
    assert_eq!(f.state().0, "外部");
}
#[test]
fn mouse_drag_selection_and_visual_keys_use_content_coordinates() {
    let mut f = Fixture::new("中文输入");
    f.input(mouse(true, 8.0));
    f.input(InputEvent::PointerMoved(Point::new(56.0, 200.0)));
    f.input(mouse(false, 56.0));
    let state = f.state();
    assert_eq!((state.1, state.2), (0, 6));
    assert!(f.tree.pointer_capture().is_none());
    f.input(key(Key::ArrowRight));
    assert_eq!((f.state().1, f.state().2), (6, 6));
    let mut shifted = key(Key::ArrowLeft);
    if let InputEvent::Key(k) = &mut shifted {
        k.modifiers.shift = true;
    }
    f.input(shifted);
    assert_eq!((f.state().1, f.state().2), (6, 3));
    assert_eq!(changed(f.input(key(Key::Delete))), "中输入");
}
#[test]
fn long_text_scrolls_caret_into_view_without_wrapping() {
    let mut f = Fixture::new("中文输入控件需要横向滚动显示完整内容");
    f.width = 120.0;
    f.update();
    f.input(key(Key::Tab));
    assert!(f.state().3 > 100.0);
    f.input(key(Key::Home));
    assert_eq!(f.state().3, 0.0);
    f.input(key(Key::End));
    assert!(f.state().3 > 100.0);
    let height = f.tree.layout_box(f.tree.root()).unwrap().bounds.size.height;
    assert_eq!(height, 48.0);
    f.input(mouse(true, 100.0));
    f.tree.cancel_input();
    assert!(
        f.input(InputEvent::PointerMoved(Point::new(0.0, 0.0)))
            .actions
            .is_empty()
    );
    assert!(f.tree.pointer_capture().is_none());
}
#[test]
fn rejected_input_read_only_disabled_and_ime_commit_do_not_corrupt_state() {
    let mut f = Fixture::new("中");
    f.view.limits.max_text_bytes = 6;
    f.update();
    f.input(key(Key::Tab));
    for text in ["\n", "\t", "过长"] {
        let result = f.input(typed(text));
        assert!(matches!(
            result.actions[0].action.payload::<TextInputEvent>(),
            Some(TextInputEvent::Rejected(_))
        ));
        assert_eq!(f.state().0, "中");
    }
    f.input(InputEvent::ImeEnabled);
    assert_eq!(changed(f.input(typed("a"))), "中a"); // IME enabled is not composition
    f.input(key(Key::Backspace));
    f.input(InputEvent::ImePreedit {
        text: "wen".into(),
        cursor: Some((3, 3)),
    });
    assert!(f.input(key(Key::Escape)).handled);
    assert!(f.input(typed("w")).actions.is_empty());
    assert_eq!(f.state().0, "中");
    f.input(InputEvent::ImePreedit {
        text: "".into(),
        cursor: None,
    });
    assert_eq!(changed(f.input(InputEvent::ImeCommit("文".into()))), "中文");
    f.view.read_only = true;
    f.update();
    assert!(f.input(typed("x")).actions.is_empty());
    assert!(f.input(key(Key::Backspace)).actions.is_empty());
    f.input(command("a"));
    assert_ne!(f.state().1, f.state().2);
    f.view.disabled = true;
    f.update();
    assert!(f.tree.focused().is_none());
    assert!(f.input(mouse(true, 20.0)).handled);
    assert!(f.tree.focused().is_none());
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    let mut f = Fixture::new("中文 Input");
    f.input(key(Key::Tab));
    for selected in [false, true] {
        if selected {
            f.input(command("a"));
        }
        let scene = f.tree.scene().unwrap();
        for scale in [1.0, 2.0] {
            let pixels = renderer
                .render(
                    &scene,
                    Frame {
                        extent: Extent::new((260.0 * scale) as u32, (64.0 * scale) as u32),
                        scale,
                        background: Color::BLACK,
                    },
                )
                .unwrap();
            let colors = if selected {
                vec![f.view.style.selection, f.view.style.foreground]
            } else {
                vec![f.view.style.caret, f.view.style.foreground]
            };
            for color in colors {
                assert!(
                    pixels
                        .pixels()
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|p| p[..3] == [color.r, color.g, color.b])
                );
            }
            let offset = (24.0 * scale) as usize * pixels.stride() + (250.0 * scale) as usize * 4;
            assert_eq!(&pixels.pixels()[offset..offset + 4], &[0, 0, 0, 255]);
        }
    }
}
#[cfg(feature = "cpu")]
#[test]
fn text_input_selection_caret_and_clip_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn text_input_selection_caret_and_clip_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}

#[test]
fn preedit_replaces_selection_visually_and_commits_as_one_undo_step() {
    let mut f = Fixture::new("原文");
    f.input(key(Key::Tab));
    f.input(command("a"));
    let before = f.state();
    for text in ["zhong", "中文"] {
        let result = f.input(InputEvent::ImePreedit {
            text: text.into(),
            cursor: Some((0, text.len())),
        });
        assert!(result.actions.is_empty());
        f.prepare();
        assert_eq!(f.state().0, before.0);
        assert_eq!(f.state().1, before.1);
        assert_eq!(f.state().2, before.2);
    }
    assert!(f.input(command("a")).actions.is_empty());
    assert_eq!(f.state().1, before.1);
    f.input(InputEvent::ImePreedit {
        text: String::new(),
        cursor: None,
    });
    assert_eq!(
        changed(f.input(InputEvent::ImeCommit("中文".into()))),
        "中文"
    );
    assert_eq!(changed(f.input(command("z"))), "原文");
    assert_eq!(changed(f.input(command("y"))), "中文");
}

#[test]
fn invalid_preedit_is_atomic_and_external_updates_cancel_temporary_text() {
    let mut f = Fixture::new("中");
    f.input(key(Key::Tab));
    f.view.limits.max_text_bytes = 12;
    f.update();
    f.input(InputEvent::ImePreedit {
        text: "文".into(),
        cursor: Some((3, 3)),
    });
    for (text, cursor) in [
        ("文", Some((1, 3))),
        ("x", Some((0, 9))),
        ("\n", None),
        ("文字超过限制", None),
    ] {
        let result = f.input(InputEvent::ImePreedit {
            text: text.into(),
            cursor,
        });
        assert!(matches!(
            result.actions[0].action.payload::<TextInputEvent>(),
            Some(TextInputEvent::Rejected(_))
        ));
        f.prepare();
        assert_eq!(f.state().0, "中");
    }
    f.view.text = "外部".into();
    f.update();
    assert_eq!(changed(f.input(typed("值"))), "外部值");
    f.input(InputEvent::ImePreedit {
        text: "a".into(),
        cursor: None,
    });
    f.input(InputEvent::ImeDisabled);
    assert_eq!(changed(f.input(command("z"))), "外部");
    f.input(InputEvent::ImePreedit {
        text: "a".into(),
        cursor: None,
    });
    f.view.read_only = true;
    f.update();
    f.view.read_only = false;
    f.update();
    assert_eq!(changed(f.input(typed("值"))), "外部值");
    f.input(InputEvent::ImePreedit {
        text: "a".into(),
        cursor: None,
    });
    f.tree.cancel_input();
    f.prepare();
    f.input(key(Key::Tab));
    assert_eq!(changed(f.input(key(Key::Backspace))), "外部");
}

#[cfg(feature = "cpu")]
#[test]
fn preedit_paints_and_cancel_restores_original_pixels() {
    fn pixels(f: &mut Fixture) -> Vec<u8> {
        f.prepare();
        zui_render_cpu::CpuRenderer::default()
            .render(
                &f.tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(260, 64),
                    scale: 1.0,
                    background: Color::BLACK,
                },
            )
            .unwrap()
            .pixels()
            .to_vec()
    }
    let mut f = Fixture::new("原文");
    f.input(key(Key::Tab));
    f.input(command("a"));
    let original = pixels(&mut f);
    f.input(InputEvent::ImePreedit {
        text: "中文".into(),
        cursor: Some((3, 3)),
    });
    let preedit = pixels(&mut f);
    assert_ne!(preedit, original);
    f.input(InputEvent::ImeDisabled);
    assert_eq!(pixels(&mut f), original);
    assert_eq!(f.state().0, "原文");
}

#[test]
fn ime_candidate_area_tracks_scrolling_and_only_resets_for_new_editing_sessions() {
    let mut f = Fixture::new("中文输入控件需要横向滚动显示完整内容");
    f.width = 120.0;
    f.update();
    assert!(f.tree.text_input().unwrap().is_none());
    f.input(key(Key::Tab));
    let end = f.tree.text_input().unwrap().unwrap();
    assert!(end.cursor_area.origin.x >= 8.0);
    assert!(end.cursor_area.origin.x + end.cursor_area.size.width <= 112.0);
    assert!(end.cursor_area.origin.y >= 8.0);
    assert!(end.cursor_area.origin.y + end.cursor_area.size.height <= 40.0);
    f.input(key(Key::Home));
    let start = f.tree.text_input().unwrap().unwrap();
    assert_eq!(start.generation, end.generation);
    assert!(start.cursor_area.origin.x < end.cursor_area.origin.x);
    f.input(InputEvent::ImePreedit {
        text: "中文".into(),
        cursor: Some((3, 3)),
    });
    f.prepare();
    let preedit = f.tree.text_input().unwrap().unwrap();
    assert_eq!(preedit.generation, start.generation);
    assert!(preedit.cursor_area.origin.x > start.cursor_area.origin.x);
    f.input(InputEvent::ImeDisabled);
    f.prepare();
    assert_eq!(
        f.tree.text_input().unwrap().unwrap().generation,
        start.generation
    );
    let value = changed(f.input(typed("值")));
    f.view.text = value;
    f.update();
    assert_eq!(
        f.tree.text_input().unwrap().unwrap().generation,
        start.generation
    );
    f.view.revision += 1;
    f.update();
    let reset = f.tree.text_input().unwrap().unwrap();
    assert_ne!(reset.generation, start.generation);
    f.view.read_only = true;
    f.update();
    assert!(f.tree.text_input().unwrap().is_none());
    f.view.read_only = false;
    f.update();
    assert_ne!(
        f.tree.text_input().unwrap().unwrap().generation,
        reset.generation
    );
    f.tree.cancel_input();
    f.prepare();
    assert!(f.tree.text_input().unwrap().is_none());
}

#[test]
fn missing_glyphs_preserve_typed_and_composed_text_and_remain_editable() {
    let mut f = Fixture::new("中");
    f.input(key(Key::Tab));
    assert_eq!(changed(f.input(typed("\u{10ffff}"))), "中\u{10ffff}");
    f.prepare();
    assert!(f.tree.text_input().unwrap().is_some());
    assert_eq!(changed(f.input(key(Key::Backspace))), "中");
    assert_eq!(changed(f.input(command("z"))), "中\u{10ffff}");
    f.prepare();
    f.input(command("a"));
    assert!(
        f.input(InputEvent::ImePreedit {
            text: "\u{10ffff}".into(),
            cursor: Some((4, 4))
        })
        .actions
        .is_empty()
    );
    f.prepare();
    assert_eq!(
        changed(f.input(InputEvent::ImeCommit("\u{10ffff}".into()))),
        "\u{10ffff}"
    );
    f.prepare();
    assert_eq!(changed(f.input(command("z"))), "中\u{10ffff}");
    f.prepare();
}

#[test]
fn control_word_commands_extend_delete_undo_and_respect_read_only_and_composition() {
    fn control(key_value: Key, shift: bool) -> InputEvent {
        let mut event = key(key_value);
        if let InputEvent::Key(k) = &mut event {
            k.modifiers.control = true;
            k.modifiers.shift = shift;
        }
        event
    }
    let mut f = Fixture::new("one two three");
    f.input(key(Key::Tab));
    f.input(key(Key::Home));
    f.input(control(Key::ArrowRight, false));
    assert_eq!((f.state().1, f.state().2), (3, 3));
    f.input(control(Key::ArrowRight, true));
    assert_eq!((f.state().1, f.state().2), (3, 7));
    assert_eq!(changed(f.input(control(Key::Delete, false))), "one three");
    assert_eq!(changed(f.input(command("z"))), "one two three");
    assert_eq!((f.state().1, f.state().2), (3, 7));
    f.input(key(Key::End));
    assert_eq!(changed(f.input(control(Key::Backspace, false))), "one two ");
    f.view.read_only = true;
    f.update();
    let result = f.input(control(Key::Backspace, false));
    assert!(result.handled && result.actions.is_empty());
    assert_eq!(f.state().0, "one two ");
    f.input(control(Key::ArrowLeft, false));
    assert_eq!(f.state().2, 4);
    f.view.read_only = false;
    f.update();
    f.input(InputEvent::ImePreedit {
        text: "wen".into(),
        cursor: Some((3, 3)),
    });
    let before = f.state();
    assert!(f.input(control(Key::ArrowRight, false)).handled);
    assert!(f.input(control(Key::Backspace, false)).actions.is_empty());
    assert_eq!(f.state().0, before.0);
    assert_eq!(f.state().2, before.2);
}

#[derive(Default)]
struct TestClipboard {
    text: String,
    writes: usize,
    reads: usize,
    fail_write: bool,
    fail_read: bool,
}
impl zui_api::clipboard::Clipboard for TestClipboard {
    fn read_text(
        &mut self,
        max_bytes: usize,
    ) -> Result<String, zui_api::clipboard::ClipboardError> {
        use zui_api::clipboard::ClipboardError;
        self.reads += 1;
        if self.fail_read {
            return Err(ClipboardError::ReadFailed);
        }
        if self.text.len() > max_bytes {
            return Err(ClipboardError::BudgetExceeded);
        }
        Ok(self.text.clone())
    }
    fn write_text(&mut self, text: &str) -> Result<(), zui_api::clipboard::ClipboardError> {
        self.writes += 1;
        if self.fail_write {
            return Err(zui_api::clipboard::ClipboardError::WriteFailed);
        }
        self.text = text.into();
        Ok(())
    }
}
#[test]
fn clipboard_copy_cut_paste_are_undoable_and_failed_cut_preserves_selection() {
    let mut f = Fixture::new("中文 e\u{301}");
    let mut clipboard = TestClipboard::default();
    f.input(key(Key::Tab));
    f.input(command("a"));
    let before = f.state();
    assert!(
        f.clipboard_input(command("c"), &mut clipboard)
            .actions
            .is_empty()
    );
    assert_eq!(clipboard.text, before.0);
    clipboard.fail_write = true;
    let result = f.clipboard_input(command("x"), &mut clipboard);
    assert!(matches!(
        result.actions[0].action.payload::<TextInputEvent>(),
        Some(TextInputEvent::Rejected(
            zui_widgets::TextInputError::Clipboard(zui_api::clipboard::ClipboardError::WriteFailed)
        ))
    ));
    assert_eq!(f.state(), before);
    clipboard.fail_write = false;
    assert_eq!(changed(f.clipboard_input(command("x"), &mut clipboard)), "");
    assert_eq!(changed(f.input(command("z"))), before.0);
    assert_eq!((f.state().1, f.state().2), (before.1, before.2));
    clipboard.text.clear();
    assert!(
        f.clipboard_input(command("v"), &mut clipboard)
            .actions
            .is_empty()
    );
    assert_eq!(f.state(), before);
    clipboard.text = "粘贴".into();
    assert_eq!(
        changed(f.clipboard_input(command("v"), &mut clipboard)),
        "粘贴"
    );
    assert_eq!(changed(f.input(command("z"))), before.0);
    assert_eq!(changed(f.input(command("y"))), "粘贴");
}
#[test]
fn clipboard_limits_read_only_repeat_and_composition_prevent_unwanted_io_or_edits() {
    let mut f = Fixture::new("中");
    f.view.limits.max_text_bytes = 6;
    f.update();
    f.input(key(Key::Tab));
    let mut clipboard = TestClipboard {
        text: "文".into(),
        ..Default::default()
    };
    // An empty selection must not overwrite the clipboard.
    f.clipboard_input(command("c"), &mut clipboard);
    assert_eq!(clipboard.writes, 0);
    for value in ["两字", "\n", "\t"] {
        clipboard.text = value.into();
        let result = f.clipboard_input(command("v"), &mut clipboard);
        assert!(matches!(
            result.actions[0].action.payload::<TextInputEvent>(),
            Some(TextInputEvent::Rejected(_))
        ));
        assert_eq!(f.state().0, "中");
    }
    clipboard.text = "文".into();
    clipboard.fail_read = true;
    assert!(
        !f.clipboard_input(command("v"), &mut clipboard)
            .actions
            .is_empty()
    );
    clipboard.fail_read = false;
    let mut repeated = command("v");
    if let InputEvent::Key(k) = &mut repeated {
        k.repeat = true;
    }
    let reads = clipboard.reads;
    f.clipboard_input(repeated, &mut clipboard);
    assert_eq!(clipboard.reads, reads);
    f.view.read_only = true;
    f.update();
    f.input(command("a"));
    for letter in ["x", "v"] {
        assert!(
            f.clipboard_input(command(letter), &mut clipboard)
                .actions
                .is_empty()
        );
    }
    assert_eq!(clipboard.reads, reads);
    assert_eq!(clipboard.writes, 0);
    f.clipboard_input(command("c"), &mut clipboard);
    assert_eq!(clipboard.text, "中");
    f.view.read_only = false;
    f.update();
    f.input(InputEvent::ImePreedit {
        text: "wen".into(),
        cursor: None,
    });
    for letter in ["c", "x", "v"] {
        assert!(
            f.clipboard_input(command(letter), &mut clipboard)
                .actions
                .is_empty()
        );
    }
    assert_eq!(clipboard.reads, reads);
    assert_eq!(clipboard.writes, 1);
    f.input(InputEvent::ImeDisabled);
    let result = f.input(command("v"));
    assert!(matches!(
        result.actions[0].action.payload::<TextInputEvent>(),
        Some(TextInputEvent::Rejected(
            zui_widgets::TextInputError::Clipboard(zui_api::clipboard::ClipboardError::Unavailable)
        ))
    ));
}

#[cfg(feature = "cpu")]
#[test]
fn caret_animation_changes_only_paint_and_input_restarts_visible_phase() {
    use std::time::Duration;
    fn pixels(f: &mut Fixture) -> Vec<u8> {
        f.prepare();
        zui_render_cpu::CpuRenderer::default()
            .render(
                &f.tree.scene().unwrap(),
                Frame {
                    extent: Extent::new(260, 64),
                    scale: 1.0,
                    background: Color::BLACK,
                },
            )
            .unwrap()
            .pixels()
            .to_vec()
    }
    let mut f = Fixture::new("中文");
    f.input(key(Key::Tab));
    assert_eq!(
        f.tree
            .advance_animations(Duration::ZERO)
            .unwrap()
            .next_frame,
        Some(Duration::from_millis(500))
    );
    let area = f.tree.text_input().unwrap();
    let visible = pixels(&mut f);
    let update = f
        .tree
        .advance_animations(Duration::from_millis(500))
        .unwrap();
    assert_eq!(update.invalidation, Invalidation::Paint);
    assert!(!f.prepare());
    assert_ne!(pixels(&mut f), visible);
    assert_eq!(f.tree.text_input().unwrap(), area);
    // Navigation at the end still restarts a hidden caret without changing text.
    f.input(key(Key::ArrowRight));
    assert_eq!(pixels(&mut f), visible);
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_millis(600))
            .unwrap()
            .next_frame,
        Some(Duration::from_millis(1100))
    );
    f.input(command("a"));
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_millis(700))
            .unwrap()
            .next_frame,
        None
    );
    f.input(key(Key::End));
    f.input(InputEvent::ImePreedit {
        text: "wen".into(),
        cursor: Some((3, 3)),
    });
    f.prepare();
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_millis(800))
            .unwrap()
            .next_frame,
        None
    );
    f.input(InputEvent::ImeDisabled);
    f.view.caret_blink_interval = None;
    f.update();
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_secs(9))
            .unwrap()
            .next_frame,
        None
    );
    f.tree.cancel_input();
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_secs(10))
            .unwrap()
            .next_frame,
        None
    );
}

#[test]
fn retained_draft_restores_committed_selection_and_history_but_not_ime_or_focus() {
    let mut f = Fixture::new("中");
    let draft = zui_widgets::TextInputDraft::new();
    f.view.draft = Some(draft.clone());
    f.update();
    f.input(key(Key::Tab));
    f.input(typed("文"));
    f.input(typed("字"));
    f.input(command("z")); // retain both undo and redo stacks
    f.input(command("a"));
    let before = f.state();
    let old = f.tree.root();
    f.input(InputEvent::ImePreedit {
        text: "临时".into(),
        cursor: Some((0, 6)),
    });
    assert!(draft.is_attached());
    assert_eq!(draft.clear(), Err(zui_widgets::TextInputError::DraftInUse));
    f.tree.reconcile(zui_widgets::column([]).build()).unwrap();
    assert!(!draft.is_attached());
    f.update();
    assert_ne!(f.tree.root(), old);
    assert_eq!(f.state(), before);
    assert!(f.tree.focused().is_none());
    assert!(f.tree.text_input().unwrap().is_none());
    f.input(key(Key::Tab));
    assert_eq!(changed(f.input(command("y"))), "中文字");
    assert_eq!(changed(f.input(command("z"))), "中文");
    assert_eq!(changed(f.input(command("z"))), "中");
}

#[test]
fn draft_exclusive_attachment_retries_and_external_reset_wins_over_saved_history() {
    use zui_api::view::ViewSpec;
    let draft = zui_widgets::TextInputDraft::new();
    let view = TextInputView::themed(&Theme::dark(), "initial", vec![], ActionId(1))
        .with_draft(draft.clone());
    let first = view.build();
    let mut second = view.build();
    assert!(second.text().is_none());
    drop(second);
    assert!(draft.is_attached()); // failed attachment must not release its owner
    second = view.build();
    drop(first);
    assert!(!draft.is_attached());
    assert_eq!(view.update(&mut second), Invalidation::Layout);
    assert_eq!(second.text(), Some("initial"));
    drop(second);
    draft.clear().unwrap();

    let mut f = Fixture::new("a");
    f.view.draft = Some(draft.clone());
    f.update();
    f.input(key(Key::Tab));
    f.input(typed("b"));
    f.tree.reconcile(zui_widgets::column([]).build()).unwrap();
    f.view.text = "ab".into(); // acknowledgement preserves history across remount
    f.update();
    f.input(key(Key::Tab));
    assert_eq!(changed(f.input(command("z"))), "a");
    f.tree.reconcile(zui_widgets::column([]).build()).unwrap();
    f.view.revision += 1;
    f.update();
    f.input(key(Key::Tab));
    assert_eq!(f.state().0, "ab");
    assert!(f.input(command("z")).actions.is_empty());
    f.tree.reconcile(zui_widgets::column([]).build()).unwrap();
    f.view.text = "external".into();
    f.update();
    assert_eq!(f.state().0, "external");
}

#[test]
fn virtual_row_recycling_returns_draft_and_remount_uses_a_new_node_with_saved_edit_state() {
    use zui_widgets::{ScrollAlignment, VirtualListView};
    let mut f = Fixture::new("原文");
    let draft = zui_widgets::TextInputDraft::new();
    f.view.draft = Some(draft.clone());
    let list = |index, revision| {
        let editor = f.view.clone();
        VirtualListView::new(1000, 48.0, move |i| {
            if i == 0 {
                description(&editor, 240.0)
            } else {
                zui_widgets::column([]).keyed(i as u64).build()
            }
        })
        .overscan(0)
        .scroll_to_index(index, ScrollAlignment::Start, revision)
        .style(|s| {
            s.width = Dimension::Length(240.0);
            s.height = Dimension::Length(48.0);
        })
        .build()
    };
    let initial = list(0, 0);
    let away = list(900, 1);
    let back = list(0, 2);
    f.tree.reconcile(initial).unwrap();
    f.prepare();
    let editor_id = |tree: &Tree| {
        let slot = tree.children(tree.root()).unwrap()[0];
        tree.children(slot).unwrap()[0]
    };
    let original = editor_id(&f.tree);
    f.tree.request_focus(Some(original)).unwrap();
    f.input(typed("修改"));
    f.input(command("a"));
    f.tree.reconcile(away).unwrap();
    f.prepare();
    assert!(!draft.is_attached());
    assert!(f.tree.focused().is_none());
    assert!(f.tree.request_focus(Some(original)).is_err());
    f.tree.reconcile(back).unwrap();
    f.prepare();
    let restored = editor_id(&f.tree);
    assert_ne!(restored, original);
    let mut state = None;
    f.tree
        .update_widget::<TextInput>(restored, Invalidation::None, |editor| {
            state = Some((
                editor.text().unwrap().to_owned(),
                editor.selection().unwrap().range(),
            ));
        })
        .unwrap();
    assert_eq!(state, Some(("原文修改".into(), 0..12)));
    f.tree.request_focus(Some(restored)).unwrap();
    assert_eq!(changed(f.input(command("z"))), "原文");
}

#[test]
fn rebinding_a_live_editor_returns_each_draft_to_its_own_business_field() {
    let mut f = Fixture::new("a");
    let first = zui_widgets::TextInputDraft::new();
    let second = zui_widgets::TextInputDraft::new();
    f.view.draft = Some(first.clone());
    f.update();
    f.input(key(Key::Tab));
    f.input(typed("b"));
    f.prepare();
    let session = f.tree.text_input().unwrap().unwrap().generation;
    f.view.draft = Some(second.clone());
    f.update();
    assert!(!first.is_attached());
    assert!(second.is_attached());
    assert_eq!(f.state().0, "a");
    assert_ne!(f.tree.text_input().unwrap().unwrap().generation, session);
    f.input(typed("c"));
    f.view.draft = Some(first.clone());
    f.update();
    assert_eq!(f.state().0, "ab");
    assert_eq!(changed(f.input(command("z"))), "a");
    f.view.draft = Some(second.clone());
    f.update();
    assert_eq!(f.state().0, "ac");
    drop(f);
    assert!(!first.is_attached());
    assert!(!second.is_attached());
    first.clear().unwrap();
    second.clear().unwrap();
}

#[test]
fn idle_draft_history_maintenance_reports_capacity_and_preserves_text_selection_on_remount() {
    let mut f = Fixture::new("中");
    let draft = zui_widgets::TextInputDraft::new();
    assert_eq!(draft.memory_usage().unwrap().allocated_bytes(), 0);
    assert_eq!(draft.release_history().unwrap(), 0);
    f.view.draft = Some(draft.clone());
    f.update();
    f.input(key(Key::Tab));
    f.input(typed("文"));
    f.input(typed("字"));
    f.input(command("z"));
    f.input(command("a"));
    let state = f.state();
    assert_eq!(
        draft.memory_usage(),
        Err(zui_widgets::TextInputError::DraftInUse)
    );
    assert_eq!(
        draft.release_history(),
        Err(zui_widgets::TextInputError::DraftInUse)
    );
    let mut active = None;
    f.tree
        .update_widget::<TextInput>(f.tree.root(), Invalidation::None, |editor| {
            active = Some(editor.editing_memory_usage())
        })
        .unwrap();
    f.tree.reconcile(zui_widgets::column([]).build()).unwrap();
    let before = draft.memory_usage().unwrap();
    assert_eq!(Some(before), active);
    assert!(before.edit.unwrap().history_capacity_bytes > 0);
    let released = draft.release_history().unwrap();
    let after = draft.memory_usage().unwrap();
    assert_eq!(before.allocated_bytes() - after.allocated_bytes(), released);
    assert_eq!(after.edit.unwrap().history_capacity_bytes, 0);
    assert_eq!(
        after.edit.unwrap().text_bytes,
        before.edit.unwrap().text_bytes
    );
    f.update();
    assert_eq!(f.state(), state);
    f.input(key(Key::Tab));
    assert!(f.input(command("z")).actions.is_empty());
    assert!(f.input(command("y")).actions.is_empty());
    f.tree.reconcile(zui_widgets::column([]).build()).unwrap();
    draft.clear().unwrap();
    assert_eq!(draft.memory_usage().unwrap().allocated_bytes(), 0);
}

fn seed_cached_draft(draft: &zui_widgets::TextInputDraft) {
    let mut f = Fixture::new("a");
    f.view.draft = Some(draft.clone());
    f.update();
    f.input(key(Key::Tab));
    f.input(typed("b"));
}

#[test]
fn bounded_draft_cache_maintains_idle_payload_in_access_order_without_changing_handle_identity() {
    use zui_widgets::{DraftCachePolicy, TextInputDraftCache};
    let mut cache = TextInputDraftCache::new(3);
    let a = cache.get_or_insert("a").unwrap();
    let b = cache.get_or_insert("b").unwrap();
    let c = cache.get_or_insert("c").unwrap();
    for draft in [&a, &b, &c] {
        seed_cached_draft(draft);
    }
    assert_eq!(cache.get(&"a"), Some(a.clone())); // b is now oldest
    let untouched = cache.maintain(usize::MAX, DraftCachePolicy::DiscardIdleDrafts);
    assert_eq!(untouched.before, untouched.after);
    assert_eq!(untouched.discarded_drafts, 0);
    let report = cache.maintain(0, DraftCachePolicy::HistoryOnly);
    assert!(!report.budget_met);
    assert!(report.history_released_bytes > 0);
    assert_eq!(report.discarded_drafts, 0);
    for draft in [&a, &b, &c] {
        let edit = draft.memory_usage().unwrap().edit.unwrap();
        assert_eq!(edit.text_bytes, 2);
        assert_eq!(edit.history_capacity_bytes, 0);
    }
    let budget = cache.usage().idle_allocated_bytes - b.memory_usage().unwrap().allocated_bytes();
    let report = cache.maintain(budget, DraftCachePolicy::DiscardIdleDrafts);
    assert!(report.budget_met);
    assert_eq!(report.discarded_drafts, 1);
    assert_eq!(b.memory_usage().unwrap().allocated_bytes(), 0);
    assert!(a.memory_usage().unwrap().allocated_bytes() > 0);
    assert!(c.memory_usage().unwrap().allocated_bytes() > 0);
    assert_eq!(cache.get_or_insert("b").unwrap(), b);
    assert_eq!(cache.len(), 3); // payload eviction preserves key registration
    let again = cache.maintain(budget, DraftCachePolicy::DiscardIdleDrafts);
    assert_eq!(again.history_released_bytes, 0);
    assert_eq!(again.discarded_drafts, 0);
}

#[test]
fn draft_cache_entry_limits_and_active_protection_are_explicit_and_retryable() {
    use zui_api::view::ViewSpec;
    use zui_widgets::{DraftCacheError, DraftCachePolicy, TextInputDraftCache};
    let mut cache = TextInputDraftCache::new(1);
    assert_eq!(cache.max_entries(), 1);
    let draft = cache.get_or_insert(7).unwrap();
    seed_cached_draft(&draft);
    let view =
        TextInputView::themed(&Theme::dark(), "a", vec![], ActionId(1)).with_draft(draft.clone());
    let editor = view.build();
    assert_eq!(editor.text(), Some("ab"));
    assert_eq!(cache.get_or_insert(8), Err(DraftCacheError::EntryLimit));
    assert_eq!(cache.get_or_insert(7).unwrap(), draft);
    assert_eq!(cache.remove(&7), Err(DraftCacheError::DraftInUse));
    let report = cache.maintain(0, DraftCachePolicy::DiscardIdleDrafts);
    assert_eq!(report.after.active_entries, 1);
    assert!(report.budget_met); // applies to idle payload only
    assert_eq!(editor.text(), Some("ab"));
    drop(editor);
    assert!(cache.usage().idle_allocated_bytes > 0); // no hidden background eviction
    assert!(cache.remove(&7).unwrap());
    assert_eq!(draft.memory_usage().unwrap().allocated_bytes(), 0);
    assert!(!cache.remove(&7).unwrap());
    assert!(cache.is_empty());
    cache.get_or_insert(8).unwrap();
    let mut zero = TextInputDraftCache::new(0);
    assert_eq!(zero.get_or_insert(0), Err(DraftCacheError::EntryLimit));
    assert!(zero.maintain(0, DraftCachePolicy::HistoryOnly).budget_met);
}
