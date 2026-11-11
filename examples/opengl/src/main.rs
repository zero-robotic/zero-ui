mod triangle;
use std::{cell::RefCell, error::Error, rc::Rc, sync::Arc, time::Duration};
use zui_api::{
    action::{ActionEvent, ActionId},
    layout::{Dimension, Edges},
    view::View,
};
use zui_app::{Application, Backend, ViewAction, ViewApplication, WidgetHost};
use zui_opengl::OpenGlView;
use zui_platform::{WindowAction, WindowConfig, WindowEvent, WindowMetrics};
use zui_theme::{Theme, TypeRole};
use zui_types::{Color, Extent, Size};
use zui_widgets::{ButtonActivated, ButtonView, Container, TextView, column, row};

struct Demo {
    gl: OpenGlView,
    families: Vec<String>,
    smoke: Option<Duration>,
    deadline: Option<Duration>,
    error: Rc<RefCell<Option<String>>>,
}
impl ViewApplication for Demo {
    type Message = u64;
    fn view(&self) -> View {
        let theme = Theme::desktop_dark();
        let title = TextView::themed(
            &theme,
            "OpenGL · 原生着色器绘制",
            self.families.clone(),
            TypeRole::Heading,
        );
        let hint = TextView::themed(
            &theme,
            "点击画面或按左右方向键旋转；可暂停连续刷新后按需重绘。",
            self.families.clone(),
            TypeRole::Body,
        );
        column([
            View::new(title),
            View::new(hint),
            Container::new(View::new(self.gl.clone()).keyed("opengl").styled(
                zui_api::layout::LayoutStyle {
                    width: Dimension::Percent(1.0),
                    height: Dimension::Length(300.0),
                    ..Default::default()
                },
            ))
            .border(theme.color(zui_theme::ColorRole::Border))
            .radius(8)
            .style(|s| s.padding = Edges::all(6.0))
            .build(),
            row([
                View::new(ButtonView::themed(
                    &theme,
                    if self.gl.frame_interval.is_some() {
                        "暂停"
                    } else {
                        "继续"
                    },
                    self.families.clone(),
                    ActionId(1),
                )),
                View::new(ButtonView::themed(
                    &theme,
                    "重绘",
                    self.families.clone(),
                    ActionId(2),
                )),
            ])
            .style(|s| s.gap = Size::new(8.0, 0.0))
            .build(),
        ])
        .style(|s| {
            s.width = Dimension::Percent(1.0);
            s.padding = Edges::all(16.0);
            s.gap = Size::new(0.0, 12.0);
        })
        .build()
    }
    fn map_action(&self, event: &ActionEvent) -> Option<u64> {
        event
            .action
            .payload::<ButtonActivated>()
            .map(|_| event.action.id.0)
    }
    fn message(&mut self, message: u64) -> ViewAction {
        if message == 1 {
            self.gl.frame_interval = if self.gl.frame_interval.is_some() {
                None
            } else {
                Some(Duration::from_millis(16))
            };
        }
        self.gl.request_redraw();
        ViewAction::rebuild()
    }
    fn update(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> ViewAction {
        if let Some(error) = self.error.borrow().as_ref() {
            eprintln!("OpenGL failed: {error}");
            return ViewAction::exit();
        }
        match event {
            WindowEvent::Resumed => {
                self.gl.scale_factor = metrics.scale;
                self.deadline = self.smoke.map(|d| elapsed + d);
                ViewAction {
                    window: WindowAction {
                        redraw: true,
                        wake_at: self.deadline,
                        ..Default::default()
                    },
                    rebuild: true,
                }
            }
            WindowEvent::ScaleChanged | WindowEvent::Resized => {
                self.gl.scale_factor = metrics.scale;
                ViewAction::rebuild()
            }
            WindowEvent::Wake if self.deadline.is_some_and(|d| elapsed >= d) => ViewAction::exit(),
            WindowEvent::CloseRequested => ViewAction::exit(),
            _ => ViewAction::default(),
        }
    }
    fn background(&self) -> Color {
        Color::rgb(16, 21, 28)
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let backend = match args.next().as_deref().unwrap_or("cpu") {
        "cpu" => Backend::Cpu,
        "gpu" => Backend::Gpu,
        _ => return Err("expected cpu or gpu".into()),
    };
    let mut smoke = None;
    let mut snapshot = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smoke-ms" => {
                smoke = Some(Duration::from_millis(
                    args.next().ok_or("missing duration")?.parse()?,
                ))
            }
            "--snapshot" => snapshot = Some(args.next().ok_or("missing image path")?),
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    let mut text = zui_text::ParleyText::default();
    let families = text.register_font(Arc::from(
        include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
    ))?;
    let error = Rc::new(RefCell::new(None));
    let capture = error.clone();
    let mut gl = OpenGlView::new(Size::new(640.0, 300.0), triangle::Triangle::default)
        .on_error(move |e| *capture.borrow_mut() = Some(e.to_string()));
    gl.frame_interval = Some(Duration::from_millis(16));
    gl.scale_factor = if snapshot.is_some() { 2.0 } else { 1.0 };
    let mut host = WidgetHost::new(
        Demo {
            gl,
            families,
            smoke,
            deadline: None,
            error: error.clone(),
        },
        zui_layout::TaffyLayout::default(),
        text,
        Default::default(),
    )?;
    if let Some(path) = snapshot {
        use zui_paint::OffscreenRenderer;
        let metrics = WindowMetrics {
            extent: Extent::new(1440, 960),
            scale: 2.0,
            occluded: false,
        };
        let mut scene = zui_paint::Scene::new();
        host.draw(&mut scene, metrics, Duration::ZERO)
            .map_err(|e| e.to_string())?;
        if let Some(e) = error.borrow().as_ref() {
            return Err(e.clone().into());
        }
        let pixels = zui_render_cpu::CpuRenderer::default().render(
            &scene,
            zui_paint::Frame {
                extent: metrics.extent,
                scale: metrics.scale,
                background: host.background(),
            },
        )?;
        let mut encoder = png::Encoder::new(std::fs::File::create(path)?, 1440, 960);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(pixels.pixels())?;
    } else {
        let report = zui_app::run(
            WindowConfig {
                title: "ZUI · OpenGL".into(),
                size: Size::new(720.0, 480.0),
                ..Default::default()
            },
            backend,
            host,
        )?;
        println!("events={}, redraws={}", report.events, report.redraws);
        if report.redraws == 0 {
            return Err("window never drew".into());
        }
    }
    if let Some(e) = error.borrow().as_ref() {
        return Err(e.clone().into());
    }
    Ok(())
}
