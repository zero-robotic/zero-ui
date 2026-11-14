//! Smallest interactive example: state -> view -> action -> state.
use std::{error::Error, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::Edges,
    view::View,
};
use zui_app::{Backend, ViewAction, ViewApplication, WidgetHost};
use zui_platform::{WindowConfig, WindowEvent, WindowMetrics};
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{ButtonActivated, ButtonView, TextView, column};

struct Counter {
    count: u64,
    families: Vec<String>,
}

impl ViewApplication for Counter {
    type Message = ();

    fn view(&self) -> View {
        let theme = Theme::desktop_dark();
        column([
            View::new(TextView::themed(
                &theme,
                format!("点击次数：{}", self.count),
                self.families.clone(),
                TypeRole::Heading,
            )),
            View::new(ButtonView::themed(
                &theme,
                "加一",
                self.families.clone(),
                ActionId(1),
            )),
        ])
        .style(|style| {
            style.padding = Edges::all(24.0);
            style.gap = Size::new(0.0, 16.0);
        })
        .build()
    }

    fn map_action(&self, event: &ActionEvent) -> Option<()> {
        (event.action.id == ActionId(1) && event.action.payload::<ButtonActivated>().is_some())
            .then_some(())
    }

    fn message(&mut self, (): ()) -> ViewAction {
        self.count = self.count.saturating_add(1);
        ViewAction::rebuild()
    }

    fn update(&mut self, event: WindowEvent, _: WindowMetrics, _: Duration) -> ViewAction {
        match event {
            WindowEvent::CloseRequested => ViewAction::exit(),
            _ => ViewAction::default(),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1).peekable();
    let backend = Backend::from_args(&mut args)?;
    if let Some(arg) = args.next() {
        return Err(format!("unknown argument: {arg}; expected optional cpu or gpu").into());
    }
    // Bundle a font so the example does not depend on installed system fonts.
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let host = WidgetHost::new(
        Counter { count: 0, families },
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )?;
    zui_app::run(
        WindowConfig {
            title: "ZUI · 最小计数器".into(),
            size: Size::new(360.0, 180.0),
            ..Default::default()
        },
        backend,
        host,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
