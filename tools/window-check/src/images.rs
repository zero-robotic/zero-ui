//! Application-owned background decoding and wake scheduling.
use std::{error::Error, num::NonZeroUsize, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
};
use zui_app::{Backend, ViewAction, ViewApplication, WidgetHost};
use zui_image::{DecodeLimits, DecodeRequestId, PngDecodeWorker};
use zui_paint::ImageResource;
use zui_platform::{
    InputEvent, Key, RunReport, WindowAction, WindowConfig, WindowEvent, WindowMetrics,
};
use zui_theme::{Theme, TypeRole};
use zui_types::{Color, Size};
use zui_widgets::{
    ButtonActivated, ButtonView, ImageFit, ImageView, ScrollView, TextView, column, row,
};

pub fn run(
    config: WindowConfig,
    backend: Backend,
    smoke: Option<Duration>,
) -> Result<RunReport, Box<dyn Error>> {
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let model = Demo::new(families, smoke)?;
    let host = WidgetHost::new(
        model,
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )?;
    Ok(zui_app::run(config, backend, host)?)
}
fn sample_png() -> Result<Arc<[u8]>, png::EncodingError> {
    let mut pixels = Vec::new();
    for y in 0..32 {
        for x in 0..64 {
            pixels.extend_from_slice(match (x < 32, y < 16) {
                (true, true) => &[30, 210, 240, 255],
                (false, true) => &[255, 100, 40, 255],
                (true, false) => &[140, 80, 255, 128],
                (false, false) => &[50, 230, 110, 255],
            });
        }
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 64, 32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&pixels)?;
        writer.finish()?;
    }
    Ok(bytes.into())
}
struct Demo {
    families: Vec<String>,
    worker: PngDecodeWorker,
    bytes: Arc<[u8]>,
    wanted: Option<DecodeRequestId>,
    image: Option<ImageResource>,
    status: String,
    closed: bool,
    smoke: Option<Duration>,
    deadline: Option<Duration>,
}
impl Demo {
    fn new(families: Vec<String>, smoke: Option<Duration>) -> Result<Self, Box<dyn Error>> {
        let mut model = Self {
            families,
            worker: PngDecodeWorker::new(DecodeLimits::default(), NonZeroUsize::new(2).unwrap())?,
            bytes: sample_png()?,
            wanted: None,
            image: None,
            status: String::new(),
            closed: false,
            smoke,
            deadline: None,
        };
        model.request(false);
        Ok(model)
    }
    fn request(&mut self, invalid: bool) {
        if self.closed {
            return;
        }
        let bytes = if invalid {
            Arc::from(&b"invalid PNG"[..])
        } else {
            self.bytes.clone()
        };
        match self.worker.try_submit(bytes) {
            Ok(id) => {
                self.wanted = Some(id);
                self.status = "正在后台解码…".into();
            }
            Err(error) => {
                self.wanted = None;
                self.status = format!("提交失败：{error}");
            }
        }
    }
    fn poll(&mut self) -> bool {
        if self.closed {
            return false;
        }
        let mut changed = false;
        loop {
            match self.worker.try_receive() {
                Ok(Some(done)) => {
                    if Some(done.id) != self.wanted {
                        continue;
                    }
                    self.wanted = None;
                    changed = true;
                    match done.result {
                        Ok(image) => {
                            self.image = Some(image);
                            self.status = "解码完成 · 64 × 32 · 含透明像素".into();
                            println!("image demo: decoded");
                        }
                        Err(error) => self.status = format!("解码失败：{error}"),
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    self.wanted = None;
                    self.closed = true;
                    self.status = error.to_string();
                    return true;
                }
            }
        }
        changed
    }
}
impl ViewApplication for Demo {
    type Message = bool;
    fn map_action(&self, event: &ActionEvent) -> Option<bool> {
        event.action.payload::<ButtonActivated>()?;
        match event.action.id {
            ActionId(1) => Some(false),
            ActionId(2) => Some(true),
            _ => None,
        }
    }
    fn message(&mut self, invalid: bool) -> ViewAction {
        self.request(invalid);
        ViewAction {
            rebuild: true,
            window: WindowAction {
                wake_at: Some(Duration::ZERO),
                ..Default::default()
            },
        }
    }
    fn view(&self) -> View {
        let theme = Theme::dashboard();
        let label = |value: &str| {
            View::new(TextView::themed(
                &theme,
                value,
                self.families.clone(),
                TypeRole::Body,
            ))
        };
        let tiles = [
            ImageFit::Contain,
            ImageFit::Cover,
            ImageFit::Fill,
            ImageFit::None,
        ]
        .into_iter()
        .map(|fit| {
            let content = if let Some(resource) = &self.image {
                let mut image = ImageView::new(resource.clone());
                image.fit = fit;
                View::new(image).keyed("image")
            } else {
                label("等待图片").keyed("placeholder")
            };
            column([
                label(&format!("{fit:?}")),
                content.styled(LayoutStyle {
                    width: Dimension::Length(160.0),
                    height: Dimension::Length(100.0),
                    ..Default::default()
                }),
            ])
            .keyed(format!("{fit:?}"))
            .style(|s| {
                s.padding = Edges::all(8.0);
                s.gap = Size::new(0.0, 8.0);
            })
            .build()
        });
        let button = |text: &str, id| {
            View::new(ButtonView::themed(
                &theme,
                text,
                self.families.clone(),
                ActionId(id),
            ))
            .keyed(id)
            .styled(LayoutStyle {
                padding: Edges::all(10.0),
                ..Default::default()
            })
        };
        ScrollView::vertical(
            column([
                label("PNG 后台解码 · 四种图片适配"),
                label(&self.status).keyed("status"),
                row([button("重新加载", 1), button("模拟损坏图片", 2)])
                    .style(|s| {
                        s.gap = Size::new(12.0, 8.0);
                        s.wrap = true;
                    })
                    .build(),
                row(tiles)
                    .style(|s| {
                        s.gap = Size::new(12.0, 12.0);
                        s.wrap = true;
                    })
                    .build(),
            ])
            .style(|s| {
                s.padding = Edges::all(20.0);
                s.gap = Size::new(0.0, 16.0);
            })
            .build(),
        )
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.height = Dimension::Percent(1.0);
        })
        .build()
    }
    fn update(&mut self, event: WindowEvent, _: WindowMetrics, elapsed: Duration) -> ViewAction {
        if matches!(event, WindowEvent::CloseRequested)
            || matches!(&event,WindowEvent::Input(InputEvent::Key(k)) if k.pressed && k.key==Key::Escape)
        {
            return ViewAction::exit();
        }
        if event == WindowEvent::Resumed && self.deadline.is_none() {
            self.deadline = self.smoke.and_then(|time| elapsed.checked_add(time));
        }
        if self.deadline.is_some_and(|time| elapsed >= time) {
            return ViewAction::exit();
        }
        let changed = self.poll();
        let next = if !self.closed && self.worker.outstanding() > 0 {
            elapsed.checked_add(Duration::from_millis(16))
        } else {
            None
        };
        ViewAction {
            rebuild: changed,
            window: WindowAction {
                wake_at: next.into_iter().chain(self.deadline).min(),
                ..Default::default()
            },
        }
    }
    fn background(&self) -> Color {
        Color::rgb(12, 18, 30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Instant};
    fn drain(model: &mut Demo) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while model.worker.outstanding() > 0 {
            model.poll();
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn loading_errors_stale_results_and_completion_stop_polling() {
        let mut model = Demo::new(vec![], None).unwrap();
        assert!(model.wanted.is_some());
        model.request(true); // The initial valid result is now stale.
        drain(&mut model);
        assert!(model.image.is_none());
        assert!(model.status.starts_with("解码失败"));
        model.request(false);
        drain(&mut model);
        let image = model.image.as_ref().unwrap().id();
        model.request(true);
        drain(&mut model);
        assert_eq!(model.image.as_ref().unwrap().id(), image); // Keep the last good image.
        let metrics = WindowMetrics {
            extent: zui_types::Extent::new(800, 480),
            scale: 1.0,
            occluded: false,
        };
        assert!(
            model
                .update(WindowEvent::Wake, metrics, Duration::from_secs(1))
                .window
                .wake_at
                .is_none()
        );
        model.request(false);
        model.request(false);
        model.request(false);
        assert!(model.status.starts_with("提交失败"));
        drain(&mut model);
        assert!(model.status.starts_with("提交失败")); // Rejected latest request is not replaced by old success.
        model.request(false);
        drain(&mut model);
        assert_ne!(model.image.as_ref().unwrap().id(), image);
    }
    #[test]
    fn host_publishes_decoded_images_and_keeps_button_focus_and_smoke_deadline() {
        use zui_app::Application;
        let mut text = zui_text::ParleyText::default();
        let families = text
            .register_font(Arc::from(
                include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
            ))
            .unwrap();
        let model = Demo::new(families, Some(Duration::from_secs(1))).unwrap();
        let mut host = WidgetHost::new(
            model,
            zui_layout::TaffyLayout::default(),
            text,
            Default::default(),
        )
        .unwrap();
        let metrics = WindowMetrics {
            extent: zui_types::Extent::new(360, 280),
            scale: 1.0,
            occluded: false,
        };
        host.update(WindowEvent::Resumed, metrics, Duration::ZERO)
            .unwrap();
        let key = |key| {
            WindowEvent::Input(InputEvent::Key(zui_platform::KeyInput {
                key,
                pressed: true,
                repeat: false,
                text: None,
                modifiers: Default::default(),
            }))
        };
        host.update(key(Key::Tab), metrics, Duration::ZERO).unwrap(); // Scroll.
        host.update(key(Key::Tab), metrics, Duration::ZERO).unwrap(); // Reload.
        let button = host.tree().focused().unwrap();
        host.update(key(Key::Enter), metrics, Duration::ZERO)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while host.model().worker.outstanding() > 0 {
            host.update(WindowEvent::Wake, metrics, Duration::from_millis(16))
                .unwrap();
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        assert!(host.model().image.is_some());
        let mut scene = zui_paint::Scene::new();
        host.draw(&mut scene, metrics, Duration::from_millis(20))
            .unwrap();
        assert_eq!(
            scene
                .commands()
                .iter()
                .filter(|c| matches!(c, zui_paint::Command::Image { .. }))
                .count(),
            4
        );
        assert_eq!(host.tree().focused(), Some(button));
        let action = host
            .update(WindowEvent::Wake, metrics, Duration::from_millis(20))
            .unwrap();
        assert_eq!(action.wake_at, Some(Duration::from_secs(1)));
        assert!(
            host.update(WindowEvent::Wake, metrics, Duration::from_secs(1))
                .unwrap()
                .exit
        );
    }
}
