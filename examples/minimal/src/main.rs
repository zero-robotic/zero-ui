//! Smallest interactive example: scoped state, declarative view, typed event.
use std::error::Error;
use zui_api::{layout::Edges, view::View};
use zui_app::{
    Backend, ViewAction, WidgetHost,
    reactive::{Component, Event, Events, ReactiveApplication},
    state::{Property, Scope},
};
use zui_platform::WindowConfig;
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{ButtonActivated, ButtonView, TextView, column};

struct Counter {
    count: Property<u64>,
    increment: Event<ButtonActivated>,
    families: Vec<String>,
}
impl Counter {
    fn create(scope: &Scope, events: &mut Events<Self>, families: Vec<String>) -> Self {
        Self {
            count: scope.property(0),
            increment: events.on(|counter: &mut Self, _: &ButtonActivated| {
                counter
                    .count
                    .set(counter.count.get().saturating_add(1))
                    .expect("live event scope");
                ViewAction::default()
            }),
            families,
        }
    }
}
impl Component for Counter {
    fn view(&self) -> View {
        let theme = Theme::desktop_dark();
        column([
            View::new(TextView::themed(
                &theme,
                format!("点击次数：{}", self.count.get()),
                self.families.clone(),
                TypeRole::Heading,
            )),
            self.increment
                .view(|action| ButtonView::themed(&theme, "加一", self.families.clone(), action)),
        ])
        .style(|style| {
            style.padding = Edges::all(24.0);
            style.gap = Size::new(0.0, 16.0);
        })
        .build()
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1).peekable();
    let backend = Backend::from_args(&mut args)?;
    if let Some(arg) = args.next() {
        return Err(format!("unknown argument: {arg}; expected optional cpu or gpu").into());
    }
    // Resolve the system sans-serif family; shaping supplies script fallback.
    let mut text = zui_text::ParleyText::with_system_fonts(Default::default());
    let families = text.resolve_families(&[])?;
    let host = WidgetHost::new(
        ReactiveApplication::new(|scope, events| Counter::create(scope, events, families)),
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

#[cfg(test)]
mod regions;
