use std::{cell::RefCell, rc::Rc, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    clipboard::{Clipboard, ClipboardError},
    event::*,
    layout::{Dimension, LayoutStyle},
    view::View,
};
use zui_app::{Application, ViewAction, ViewApplication, WidgetHost};
use zui_platform::{WindowEvent, WindowMetrics};
use zui_types::Extent;
use zui_widgets::{TextInputEvent, TextInputView};
struct Memory(Rc<RefCell<String>>);
impl Clipboard for Memory {
    fn read_text(&mut self, limit: usize) -> Result<String, ClipboardError> {
        if self.0.borrow().len() > limit {
            return Err(ClipboardError::BudgetExceeded);
        }
        Ok(self.0.borrow().clone())
    }
    fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        *self.0.borrow_mut() = text.into();
        Ok(())
    }
}
struct Model(TextInputView);
impl ViewApplication for Model {
    type Message = String;
    fn view(&self) -> View {
        View::new(self.0.clone())
            .keyed("input")
            .styled(LayoutStyle {
                width: Dimension::Length(200.0),
                height: Dimension::Length(40.0),
                ..Default::default()
            })
    }
    fn map_action(&self, event: &ActionEvent) -> Option<String> {
        match event.action.payload::<TextInputEvent>()? {
            TextInputEvent::Changed(text) => Some(text.clone()),
            _ => None,
        }
    }
    fn message(&mut self, text: String) -> ViewAction {
        self.0.text = text;
        ViewAction::rebuild()
    }
    fn update(&mut self, _: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        ViewAction::default()
    }
}
#[test]
fn host_injects_clipboard_and_maps_cut_paste_back_to_model() {
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let clipboard = Rc::new(RefCell::new(String::new()));
    let mut host = WidgetHost::new(
        Model(TextInputView::themed(
            &zui_theme::Theme::dark(),
            "中文",
            families,
            ActionId(1),
        )),
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )
    .unwrap()
    .with_clipboard(Memory(clipboard.clone()));
    let metrics = WindowMetrics {
        extent: Extent::new(200, 80),
        scale: 1.0,
        occluded: false,
    };
    let send = |host: &mut WidgetHost<Model, zui_layout::TaffyLayout, zui_text::ParleyText>,
                key,
                command| {
        host.update(
            WindowEvent::Input(InputEvent::Key(KeyInput {
                key,
                pressed: true,
                repeat: false,
                text: None,
                modifiers: Modifiers {
                    control: command,
                    ..Default::default()
                },
            })),
            metrics,
            Duration::ZERO,
        )
        .unwrap();
    };
    send(&mut host, Key::Tab, false);
    send(&mut host, Key::Character("a".into()), true);
    send(&mut host, Key::Character("x".into()), true);
    assert_eq!(host.model().0.text, "");
    assert_eq!(*clipboard.borrow(), "中文");
    send(&mut host, Key::Character("v".into()), true);
    assert_eq!(host.model().0.text, "中文");
    send(&mut host, Key::Character("z".into()), true);
    assert_eq!(host.model().0.text, "");
}
