//! Shared edit state, pure derivation, typed bindings and background completion.
use std::{error::Error, time::Duration};
use zui_api::{layout::Edges, view::View};
use zui_app::{
    Backend, ViewAction, WidgetHost,
    reactive::{Component, Events, ReactiveApplication, Region, Tasks},
    state::{Computed, Property, Scope},
};
use zui_platform::{WindowConfig, WindowEvent, WindowMetrics};
use zui_theme::{Theme, TypeRole};
use zui_types::Size;
use zui_widgets::{
    ButtonActivated, ButtonView, CheckBoxView, CheckState, NumberInputView, NumericRange,
    SliderView, TextView, column,
};

struct Settings {
    size: Property<f64>,
    saved: Property<f64>,
    busy: Property<bool>,
    status: Property<String>,
    tasks: Tasks<Self>,
    form: Region,
    families: Vec<String>,
    smoke: bool,
    external_completion: bool,
}
impl Settings {
    fn create(
        scope: &Scope,
        events: &Events<Self>,
        tasks: Tasks<Self>,
        families: Vec<String>,
        smoke: bool,
    ) -> Self {
        let size = scope.named_property("字号", 18.0);
        let saved = scope.property(18.0);
        let busy = scope.property(false);
        let status = scope.property("调整字号后保存；此示例只保存到内存。".to_string());
        let enabled = scope.property(CheckState::Checked);
        let slider = events.binding::<SliderView>(&size);
        let number = events.binding::<NumberInputView>(&size);
        let check = events.binding::<CheckBoxView>(&enabled);
        let save = events.on(|model: &mut Self, _: &ButtonActivated| {
            model.save();
            ViewAction::default()
        });
        let can_save = {
            let (size, saved, busy) = (size.clone(), saved.clone(), busy.clone());
            Computed::new(move || size.get() != saved.get() && !busy.get())
        };
        let form = {
            let (size, busy, status, families) =
                (size.clone(), busy.clone(), status.clone(), families.clone());
            Region::named("设置表单", move || {
                let theme = Theme::desktop_dark();
                let range = NumericRange::new(12.0, 48.0, 1.0).unwrap();
                let disabled = enabled.get() != CheckState::Checked || busy.get();
                let mut preview = TextView::themed(
                    &theme,
                    "共享字号 · Shared font size",
                    families.clone(),
                    TypeRole::Body,
                );
                preview.style.font_size = size.get() as f32;
                preview.style.line_height = preview.style.font_size * 1.4;
                column([
                    check.view(|value, action| {
                        CheckBoxView::themed(&theme, "允许编辑", families.clone(), value, action)
                    }),
                    slider.view(|value, action| {
                        let mut spec = SliderView::themed(&theme, range, value, action);
                        spec.disabled = disabled;
                        spec
                    }),
                    number.view(|value, action| {
                        let mut spec =
                            NumberInputView::themed(&theme, value, range, families.clone(), action);
                        spec.editor.disabled = disabled;
                        spec.precision = 0;
                        spec
                    }),
                    View::new(preview),
                    save.view(|action| {
                        let mut spec = ButtonView::themed(
                            &theme,
                            if busy.get() {
                                "正在保存…"
                            } else {
                                "保存"
                            },
                            families.clone(),
                            action,
                        );
                        spec.disabled = !can_save.get();
                        spec
                    }),
                    View::new(TextView::themed(
                        &theme,
                        status.get(),
                        families.clone(),
                        TypeRole::Body,
                    )),
                ])
                .style(|style| style.gap = Size::new(0.0, 14.0))
                .build()
            })
        };
        Self {
            size,
            saved,
            busy,
            status,
            tasks,
            form,
            families,
            smoke,
            external_completion: false,
        }
    }
    fn save(&mut self) {
        if self.busy.get() || self.size.get() == self.saved.get() {
            return;
        }
        let snapshot = self.size.get();
        self.busy.set(true).unwrap();
        self.status.set("正在处理设置快照…".into()).unwrap();
        // A real application replaces the worker with its persistence service.
        // The worker receives a snapshot; only the UI completion changes state.
        if let Err(error) = self.tasks.spawn(
            move || snapshot,
            |model, saved| {
                model.saved.set(saved).unwrap();
                model.busy.set(false).unwrap();
                model
                    .status
                    .set(format!("已保存到内存：字号 {saved:.0}"))
                    .unwrap();
                ViewAction::default()
            },
        ) {
            self.busy.set(false).unwrap();
            self.status
                .set(format!("无法启动保存任务：{error}"))
                .unwrap();
        }
    }
}
impl Component for Settings {
    fn view(&self) -> View {
        column([
            View::new(TextView::themed(
                &Theme::desktop_dark(),
                "字体设置",
                self.families.clone(),
                TypeRole::Heading,
            )),
            self.form.view(),
        ])
        .style(|style| {
            style.padding = Edges::all(24.0);
            style.gap = Size::new(0.0, 20.0);
        })
        .build()
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, elapsed: Duration) -> ViewAction {
        if event == WindowEvent::ExternalWake && !self.busy.get() && self.saved.get() == 24.0 {
            self.external_completion = true;
        }
        if self.smoke && elapsed >= Duration::from_secs(2) {
            assert!(
                self.external_completion,
                "background completion did not externally wake the UI"
            );
        }
        if event == WindowEvent::CloseRequested || (self.smoke && elapsed >= Duration::from_secs(2))
        {
            return ViewAction::exit();
        }
        if self.smoke && event == WindowEvent::Resumed {
            self.size.set(24.0).unwrap();
            self.save();
            return ViewAction {
                window: zui_platform::WindowAction {
                    wake_at: Some(Duration::from_secs(2)),
                    ..Default::default()
                },
                ..Default::default()
            };
        }
        ViewAction::default()
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1).peekable();
    let backend = Backend::from_args(&mut args)?;
    let smoke = match args.next().as_deref() {
        Some("--smoke") => true,
        None => false,
        Some(other) => return Err(format!("unknown option: {other}").into()),
    };
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let mut text = zui_text::ParleyText::with_system_fonts(Default::default());
    let families = text.resolve_families(&[])?;
    let app = ReactiveApplication::with_tasks(|scope, events, tasks| {
        Settings::create(scope, events, tasks, families, smoke)
    });
    let host = WidgetHost::new(
        app,
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )?;
    zui_app::run(
        WindowConfig {
            title: "ZUI · 属性绑定与设置".into(),
            size: Size::new(640.0, 500.0),
            ..Default::default()
        },
        backend,
        host,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
