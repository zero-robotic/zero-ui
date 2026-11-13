use std::{error::Error, fs::File, io::BufWriter, path::PathBuf};
use zui_paint::{Frame, OffscreenRenderer, PixelBuffer};
use zui_types::{Color, Extent};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let backend = args.next().unwrap_or_else(|| "gpu".into());
    let output = PathBuf::from(
        args.next()
            .unwrap_or_else(|| format!("artifacts/{backend}.png")),
    );
    let frame = Frame {
        extent: Extent::new(640, 360),
        scale: 1.0,
        background: Color::rgb(5, 14, 25),
    };
    let scene = match args.next().as_deref().unwrap_or("primitives") {
        "primitives" => zui_render_check::preview(),
        "layout" => zui_render_check::layout_preview(zui_types::Size::new(640.0, 360.0))?,
        "text" => zui_render_check::text_preview()?,
        "text-widget" => zui_render_check::text_widget_preview()?,
        "text-selection" => zui_render_check::text_selection_preview()?,
        "text-input" => zui_render_check::text_input_preview()?,
        "button" => zui_render_check::button_preview()?,
        "checkbox" => zui_render_check::checkbox_preview()?,
        "radio-horizontal" => zui_render_check::radio_horizontal_preview()?,
        "panel" => zui_render_check::panel_preview()?,
        "metric" => zui_render_check::metric_preview()?,
        "sparkline" => zui_render_check::sparkline_preview()?,
        "accordion" => zui_render_check::accordion_preview()?,
        "tabs" => zui_render_check::tabs_preview()?,
        "segmented" => zui_render_check::segmented_preview()?,
        "radio" => zui_render_check::radio_preview()?,
        "switch" => zui_render_check::switch_preview()?,
        "slider" => zui_render_check::slider_preview()?,
        "progress" => zui_render_check::progress_preview()?,
        "stack" => zui_render_check::stack_preview()?,
        "virtual-list-measured" => zui_render_check::virtual_list_measured_preview()?,
        "virtual-list" => zui_render_check::virtual_list_preview()?,
        "scroll" => zui_render_check::scroll_preview()?,
        "scroll-both" => zui_render_check::scroll_both_preview()?,
        "dialog-header" => zui_render_check::dialog_preview(true)?,
        "dialog-plain" => zui_render_check::dialog_preview(false)?,
        "desktop-dark" => zui_render_check::desktop_preview(&zui_theme::Theme::desktop_dark())?,
        "desktop-light" => zui_render_check::desktop_preview(&zui_theme::Theme::desktop_light())?,
        "theme-dark" => zui_render_check::theme_preview(&zui_theme::Theme::dark())?,
        "theme-light" => zui_render_check::theme_preview(&zui_theme::Theme::light())?,
        "theme-dashboard" => zui_render_check::theme_preview(&zui_theme::Theme::dashboard())?,
        other => {
            return Err(format!(
                "unknown scene: {other}; expected primitives, layout, text, text-widget, text-selection, text-input, button, checkbox, segmented, tabs, accordion, sparkline, metric, panel, radio, radio-horizontal, slider, switch, progress, stack, scroll, scroll-both, virtual-list, virtual-list-measured, dialog-header, dialog-plain, desktop-dark, desktop-light, theme-dark, theme-light or theme-dashboard"
            )
            .into());
        }
    };
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    let pixels = match backend.as_str() {
        #[cfg(feature = "cpu")]
        "cpu" => zui_render_cpu::CpuRenderer::default().render(&scene, frame)?,
        #[cfg(feature = "gpu")]
        "gpu" => {
            let mut renderer =
                pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default()))?;
            eprintln!("GPU adapter: {}", renderer.adapter_name());
            renderer.render(&scene, frame)?
        }
        _ => {
            return Err(format!(
                "backend {backend:?} not enabled; build with --features cpu or gpu"
            )
            .into());
        }
    };
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    save_png(&pixels, &output)?;
    println!(
        "{}: {}×{}",
        output.display(),
        frame.extent.width,
        frame.extent.height
    );
    Ok(())
}

fn save_png(buffer: &PixelBuffer, path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path)?),
        buffer.extent().width,
        buffer.extent().height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // PNG expects straight alpha; renderer output is explicitly premultiplied.
    let mut pixels = buffer.pixels().to_vec();
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = (u32::from(*channel) * 255 + alpha / 2)
                .checked_div(alpha)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
    encoder.write_header()?.write_image_data(&pixels)?;
    Ok(())
}
