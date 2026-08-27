use std::cell::RefCell;
use std::time::Instant;

use zui_backend_winit::WinitBackend;
use zui_core::{Dip, PhysicalSize, Point};
use zui_platform::{Host, InputEvent, PlatformEvent, WindowOptions};
use zui_render::{RenderError, Renderer};
use zui_ui::{
    Button, Checkbox, ColumnLayout, Constraints, IconButton, IconName, Layout, Padding, Switch,
    Text, TextInput, Theme, UiEvent, WidgetTree,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let renderer = RefCell::new(Renderer::new_blocking()?);
    let mut tree = WidgetTree::new(Padding::new(
        Layout::new(ColumnLayout::new().spacing(Dip(8.0)))
            .child(Text::new("zero-ui controls"))
            .child(IconButton::new(IconName::Menu))
            .child(Button::new("开始语音"))
            .child(Checkbox::new("启用语音识别"))
            .child(Switch::new("自动播放"))
            .child(TextInput::new()),
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

    let mut on_window = |host: &zui_backend_winit::WinitHost| {
        let size = host.scale_factor().to_physical(host.size());
        let mut tree = ui.borrow_mut();
        tree.layout(Constraints::loose(host.size()));
        tree.request_paint(None);
        drop(tree);
        renderer
            .borrow_mut()
            .attach_surface(host.id(), host, size, host.scale_factor())
            .expect("failed to attach render surface");
        host.request_redraw()
            .expect("failed to request first redraw");
    };

    let mut handler = |event: PlatformEvent| match event {
        PlatformEvent::RedrawRequested(window) => {
            if !ui.borrow().needs_redraw() {
                if ui.borrow().next_redraw().is_some() {
                    ui.borrow_mut().request_paint(None);
                } else {
                    return None;
                }
            }
            let (damage_regions, node, render_index, dirty_widgets) = {
                let mut tree = ui.borrow_mut();
                let damage_regions = tree.dirty_regions().to_vec();
                let node = tree.render_node_cached().clone();
                let render_index = tree.render_index().clone();
                let dirty_widgets = tree.dirty_widget_ids();
                (damage_regions, node, render_index, dirty_widgets)
            };
            if let Err(error) = renderer.borrow_mut().render_node_with_dirty_widgets(
                window,
                &node,
                &damage_regions,
                background,
                &render_index,
                &dirty_widgets,
            ) {
                match error {
                    RenderError::SurfaceLost => {
                        eprintln!("render surface lost; waiting for resize")
                    }
                    error => eprintln!("failed to render frame: {error}"),
                }
            } else {
                ui.borrow_mut().mark_clean();
            }
            ui.borrow().next_redraw()
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
            Some(Instant::now())
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
            let (result, actions) = ui.borrow_mut().event(&ui_event);
            for action in actions {
                println!("UI action: {:?}", action);
            }
            (result == zui_ui::EventResult::RequestRedraw).then(Instant::now)
        }
        PlatformEvent::CloseRequested(window) => {
            renderer.borrow_mut().detach_surface(window);
            None
        }
        _ => None,
    };

    WinitBackend::new()?.run_with_options_and_schedule(
        WindowOptions {
            title: "zero-ui empty window".into(),
            ..WindowOptions::default()
        },
        &mut on_window,
        &mut handler,
    )?;
    Ok(())
}
