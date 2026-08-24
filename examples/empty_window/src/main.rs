use std::cell::RefCell;

use zui_backend_winit::WinitBackend;
use zui_core::{Dip, PhysicalSize, Point};
use zui_platform::{Host, InputEvent, PlatformEvent, WindowOptions};
use zui_render::{DisplayList, RenderError, Renderer};
use zui_ui::{Button, Column, Constraints, Padding, Text, TextInput, Theme, UiEvent, WidgetTree};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let renderer = RefCell::new(Renderer::new_blocking()?);
    let display_list = RefCell::new(DisplayList::new());
    let mut tree = WidgetTree::new(Padding::new(
        Column::new(vec![
            Box::new(Text::new("zero-ui controls")),
            Box::new(Button::new("开始语音")),
            Box::new(TextInput::new()),
        ])
        .with_spacing(Dip(8.0)),
        Dip(16.0),
    ));
    let theme = Theme::from_toml_str(include_str!("../theme.toml"))?;
    let background = theme.background;
    tree.set_theme(theme);
    let ui = RefCell::new(tree);
    let pointer_position = RefCell::new(Point {
        x: Dip(0.0),
        y: Dip(0.0),
    });

    display_list.borrow_mut().clear(background);

    let mut on_window = |host: &zui_backend_winit::WinitHost| {
        let size = host.scale_factor().to_physical(host.size());
        ui.borrow_mut().layout(Constraints::loose(host.size()));
        renderer
            .borrow_mut()
            .attach_surface(host.id(), host, size, host.scale_factor())
            .expect("failed to attach render surface");
        host.request_redraw()
            .expect("failed to request first redraw");
    };

    let mut handler = |event: PlatformEvent| match event {
        PlatformEvent::RedrawRequested(window) => {
            let mut commands = display_list.borrow_mut();
            commands.clear(background);
            ui.borrow().paint(&mut commands);
            if let Err(error) = renderer.borrow_mut().render_frame(window, &commands) {
                match error {
                    RenderError::SurfaceLost => {
                        eprintln!("render surface lost; waiting for resize")
                    }
                    error => eprintln!("failed to render frame: {error}"),
                }
            }
        }
        PlatformEvent::WindowResized {
            window,
            size,
            scale_factor,
        } => {
            let physical = scale_factor.to_physical(size);
            if physical != PhysicalSize::default() {
                ui.borrow_mut().layout(Constraints::loose(size));
                renderer
                    .borrow_mut()
                    .resize(window, physical, scale_factor)
                    .expect("failed to resize render surface");
            }
        }
        PlatformEvent::Input { window, event } => {
            if let InputEvent::CursorMoved { position } = &event {
                *pointer_position.borrow_mut() = *position;
            }
            let ui_event = match event {
                InputEvent::CursorMoved { position } => {
                    UiEvent::pointer(Some(window), position, event)
                }
                InputEvent::MouseInput { .. } => {
                    UiEvent::pointer(Some(window), *pointer_position.borrow(), event)
                }
                _ => UiEvent::input(event),
            };
            let (_, actions) = ui.borrow_mut().event(&ui_event);
            for action in actions {
                println!("UI action: {:?}", action);
            }
        }
        PlatformEvent::CloseRequested(window) => renderer.borrow_mut().detach_surface(window),
        _ => {}
    };

    WinitBackend::new()?.run_with_options(
        WindowOptions {
            title: "zero-ui empty window".into(),
            ..WindowOptions::default()
        },
        &mut on_window,
        &mut handler,
    )?;
    Ok(())
}
