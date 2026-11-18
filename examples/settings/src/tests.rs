use super::*;
use std::sync::Arc;
use zui_app::{Application, ViewApplication};
use zui_platform::{InputEvent, Key, KeyInput, Modifiers};

type Host =
    WidgetHost<ReactiveApplication<Settings>, zui_layout::TaffyLayout, zui_text::ParleyText>;
fn metrics() -> WindowMetrics {
    WindowMetrics {
        extent: zui_types::Extent::new(640, 500),
        scale: 1.0,
        occluded: false,
    }
}
fn host() -> Host {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let app = ReactiveApplication::with_tasks(|scope, events, tasks| {
        Settings::create(scope, events, tasks, families, false)
    });
    WidgetHost::new(
        app,
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap()
}
fn key(host: &mut Host, key: Key, pressed: bool, text: Option<&str>, control: bool) {
    host.update(
        WindowEvent::Input(InputEvent::Key(KeyInput {
            key,
            pressed,
            repeat: false,
            text: text.map(str::to_owned),
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
        })),
        metrics(),
        Duration::ZERO,
    )
    .unwrap();
}
fn tab(host: &mut Host) {
    key(host, Key::Tab, true, None, false);
}
fn replace(host: &mut Host, text: &str) {
    key(host, Key::Character("a".into()), true, None, true);
    key(host, Key::Character(text.into()), true, Some(text), false);
}
#[test]
fn controls_share_values_but_invalid_number_drafts_remain_local() {
    let mut host = host();
    tab(&mut host); // checkbox
    tab(&mut host); // slider
    key(&mut host, Key::ArrowRight, true, None, false);
    assert_eq!(host.model().component().size.get(), 19.0);
    tab(&mut host); // number editor
    replace(&mut host, "-");
    assert_eq!(host.model().component().size.get(), 19.0);
    // Rebuilding because an unrelated label changed must retain the draft.
    host.model()
        .component()
        .status
        .set("与编辑草稿无关的状态变化".into())
        .unwrap();
    host.update(WindowEvent::ExternalWake, metrics(), Duration::ZERO)
        .unwrap();
    key(&mut host, Key::Enter, true, None, false);
    assert_eq!(host.model().component().size.get(), 19.0);
    replace(&mut host, "2"); // Valid partial draft, not yet submitted.
    host.model()
        .component()
        .status
        .set("再次刷新其他属性".into())
        .unwrap();
    host.update(WindowEvent::ExternalWake, metrics(), Duration::ZERO)
        .unwrap();
    key(&mut host, Key::End, true, None, false);
    key(
        &mut host,
        Key::Character("7".into()),
        true,
        Some("7"),
        false,
    );
    // If the rebuild had reset the draft to 19, this would submit invalid 197.
    key(&mut host, Key::Enter, true, None, false);
    assert_eq!(host.model().component().size.get(), 27.0);
    // A normal host preparation consumes the observed changes.
    key(&mut host, Key::Enter, false, None, false);
    assert!(!host.model().view_is_dirty());
}
#[test]
fn save_uses_snapshot_and_completion_updates_ui_state() {
    let mut host = host();
    host.model().component().size.set(30.0).unwrap();
    for _ in 0..4 {
        tab(&mut host);
    } // checkbox, slider, number, save
    key(&mut host, Key::Space, true, None, false);
    key(&mut host, Key::Space, false, None, false);
    assert!(host.model().component().busy.get());
    // Owned snapshot must be saved even if a later model change supersedes it.
    host.model().component().size.set(31.0).unwrap();
    let started = std::time::Instant::now();
    while host.model().component().busy.get() {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "background save did not finish"
        );
        host.update(WindowEvent::ExternalWake, metrics(), Duration::ZERO)
            .unwrap();
        std::thread::yield_now();
    }
    assert_eq!(host.model().component().saved.get(), 30.0);
    assert_eq!(host.model().component().size.get(), 31.0);
    assert!(host.model().component().status.get().contains("30"));
}
