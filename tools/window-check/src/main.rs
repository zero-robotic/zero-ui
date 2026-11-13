use std::{error::Error, time::Duration};
use zui_app::{Application, Backend};
use zui_paint::{Path, Scene};
use zui_platform::{
    InputEvent, Key, NativeWindow, PointerButton, WindowAction, WindowConfig, WindowEvent,
    WindowMetrics,
};
use zui_types::{Color, Point, Rect, Size};
mod dashboard;
mod images;
mod tabs;
mod widgets;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1).peekable();
    let backend = Backend::from_args(&mut args)?;
    let mut smoke = None;
    let mut widgets = false;
    let mut tabs = false;
    let mut dashboard = false;
    let mut images = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--widgets" => widgets = true,
            "--tabs" => tabs = true,
            "--dashboard" => dashboard = true,
            "--images" => images = true,
            "--smoke-ms" => {
                smoke = Some(Duration::from_millis(
                    args.next().ok_or("missing smoke duration")?.parse()?,
                ))
            }
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    if usize::from(widgets) + usize::from(tabs) + usize::from(dashboard) + usize::from(images) > 1 {
        return Err("choose one of --widgets, --tabs, --dashboard or --images".into());
    }
    let config = WindowConfig {
        title: format!("ZUI {backend:?} — click / space / resize"),
        size: Size::new(800.0, 480.0),
        ..Default::default()
    };
    let report = if images {
        images::run(config, backend, smoke)?
    } else if dashboard {
        dashboard::run(config, backend, smoke)?
    } else if tabs {
        tabs::run(config, backend, smoke)?
    } else if widgets {
        widgets::run(config, backend, smoke)?
    } else {
        zui_app::run(
            config,
            backend,
            Preview {
                pointer: None,
                active: false,
                smoke,
                deadline: None,
            },
        )?
    };
    println!(
        "window closed: events={}, redraws={}",
        report.events, report.redraws
    );
    if report.redraws == 0 {
        return Err("window never received a redraw".into());
    }
    Ok(())
}

struct Preview {
    pointer: Option<Point>,
    active: bool,
    smoke: Option<Duration>,
    deadline: Option<Duration>,
}

impl Application for Preview {
    fn connected(&mut self, window: &dyn NativeWindow) {
        window.set_ime_enabled(true);
        window.set_ime_cursor_area(Rect::new(24.0, 24.0, 1.0, 24.0));
    }

    fn update(
        &mut self,
        event: WindowEvent,
        metrics: WindowMetrics,
        elapsed: Duration,
    ) -> Result<WindowAction, zui_app::ApplicationError> {
        Ok(match event {
            WindowEvent::Resumed => {
                println!(
                    "resumed: physical={:?}, logical={:?}, scale={}",
                    metrics.extent,
                    metrics.logical_size(),
                    metrics.scale
                );
                self.deadline = self.smoke.map(|duration| elapsed + duration);
                WindowAction {
                    redraw: true,
                    wake_at: self.deadline,
                    ..Default::default()
                }
            }
            WindowEvent::CloseRequested => WindowAction::exit(),
            WindowEvent::Wake if self.deadline.is_some_and(|deadline| elapsed >= deadline) => {
                WindowAction::exit()
            }
            WindowEvent::Input(InputEvent::PointerMoved(point)) => {
                self.pointer = Some(point);
                WindowAction::redraw()
            }
            WindowEvent::Input(InputEvent::PointerLeft) => {
                self.pointer = None;
                WindowAction::redraw()
            }
            WindowEvent::Input(InputEvent::PointerButton {
                button: PointerButton::Primary,
                pressed: true,
                position,
            }) => {
                self.active = !self.active;
                println!("pointer pressed at {position:?}; active={}", self.active);
                WindowAction::redraw()
            }
            WindowEvent::Input(InputEvent::Key(input)) if input.pressed => {
                println!("key: {:?}, text={:?}", input.key, input.text);
                match input.key {
                    Key::Escape => WindowAction::exit(),
                    Key::Space | Key::Enter => {
                        self.active = !self.active;
                        WindowAction::redraw()
                    }
                    _ => WindowAction::default(),
                }
            }
            WindowEvent::Input(InputEvent::ImePreedit { text, cursor }) => {
                println!("IME preedit: {text:?} {cursor:?}");
                WindowAction::default()
            }
            WindowEvent::Input(InputEvent::ImeCommit(text)) => {
                println!("IME commit: {text:?}");
                WindowAction::default()
            }
            WindowEvent::Resized | WindowEvent::ScaleChanged => {
                println!(
                    "metrics: physical={:?}, logical={:?}, scale={}",
                    metrics.extent,
                    metrics.logical_size(),
                    metrics.scale
                );
                WindowAction::redraw()
            }
            _ => WindowAction::default(),
        })
    }

    fn draw(
        &mut self,
        scene: &mut Scene,
        metrics: WindowMetrics,
        _elapsed: Duration,
    ) -> Result<(), zui_app::ApplicationError> {
        let size = metrics.logical_size();
        let mut painter = scene.painter();
        let cyan = if self.active {
            Color::rgb(255, 190, 65)
        } else {
            Color::rgb(35, 210, 225)
        };
        for column in 0..=(size.width / 40.0) as usize {
            painter.fill_rect(
                Rect::new(column as f64 * 40.0, 0.0, 1.0, size.height),
                Color::rgb(15, 35, 54),
            );
        }
        for row in 0..=(size.height / 40.0) as usize {
            painter.fill_rect(
                Rect::new(0.0, row as f64 * 40.0, size.width, 1.0),
                Color::rgb(15, 35, 54),
            );
        }
        let width = (size.width - 64.0).max(1.0);
        let height = (size.height - 64.0).max(1.0);
        let panel = Rect::new(32.0, 32.0, width, height);
        painter.fill_rect(panel, Color::rgba(10, 27, 44, 240));
        painter.stroke(Path::rectangle(panel), cyan, 2.0);
        painter.with_clip(Path::rectangle(panel), |painter| {
            for (index, fraction) in [0.2, 0.4, 0.3, 0.6, 0.8, 0.7, 0.9].into_iter().enumerate() {
                let h = (height - 40.0).max(1.0) * fraction;
                let bar_width = width / 10.0;
                let x = 48.0 + index as f64 * (width - 32.0) / 7.0;
                painter.fill_rect(
                    Rect::new(x, 32.0 + height - 20.0 - h, bar_width, h),
                    Color::rgba(cyan.r, cyan.g, cyan.b, 120),
                );
                painter.fill_rect(Rect::new(x, 32.0 + height - 20.0 - h, bar_width, 3.0), cyan);
            }
            if let Some(point) = self.pointer {
                painter.fill_rect(
                    Rect::new(point.x - 1.0, 32.0, 2.0, height),
                    Color::rgba(255, 255, 255, 100),
                );
                painter.fill_rect(
                    Rect::new(32.0, point.y - 1.0, width, 2.0),
                    Color::rgba(255, 255, 255, 100),
                );
            }
        });
        Ok(())
    }

    fn background(&self) -> Color {
        Color::rgb(5, 14, 25)
    }
}
