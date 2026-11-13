use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::{ColorRole, Theme};
use zui_types::{Color, Extent};

fn contrast(a: Color, b: Color) -> f64 {
    let luminance = |c: Color| {
        let channel = |v: u8| {
            let s = f64::from(v) / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
    };
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn desktop_text_and_selection_have_readable_contrast() {
    for theme in [Theme::desktop_dark(), Theme::desktop_light()] {
        for surface in [
            ColorRole::Background,
            ColorRole::Surface,
            ColorRole::SurfaceRaised,
        ] {
            for text in [ColorRole::Text, ColorRole::TextMuted] {
                assert!(contrast(theme.color(text), theme.color(surface)) >= 4.5);
            }
        }
        assert!(contrast(theme.selection_foreground(), theme.selection_background()) >= 4.5);
        assert!(
            contrast(
                theme.color(ColorRole::OnAccent),
                theme.color(ColorRole::Accent)
            ) >= 4.5
        );
    }
}

fn verify_scales(renderer: &mut impl OffscreenRenderer) {
    for theme in [Theme::desktop_dark(), Theme::desktop_light()] {
        let scene = zui_render_check::desktop_preview(&theme).unwrap();
        scene.validate(Default::default()).unwrap();
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let frame = Frame {
                extent: Extent::new((640.0 * scale) as u32, (360.0 * scale) as u32),
                scale,
                background: Color::BLACK,
            };
            let pixels = renderer.render(&scene, frame).unwrap();
            let at = |x: f64, y: f64| {
                let i = (y * scale) as usize * pixels.stride() + (x * scale) as usize * 4;
                &pixels.pixels()[i..i + 4]
            };
            let background = theme.color(ColorRole::Background);
            let surface = theme.color(ColorRole::Surface);
            // Rounded panel corner stays on the page; interior is fully opaque.
            assert_eq!(
                at(16.0, 84.0),
                &[background.r, background.g, background.b, 255]
            );
            assert_eq!(at(24.0, 92.0), &[surface.r, surface.g, surface.b, 255]);
            assert!(
                pixels
                    .pixels()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| pixel[3] == 255)
            );
        }
    }
}

#[cfg(feature = "cpu")]
#[test]
fn desktop_cpu_fractional_and_retina_scales() {
    verify_scales(&mut zui_render_cpu::CpuRenderer::default());
}

#[cfg(feature = "gpu")]
#[test]
fn desktop_gpu_fractional_and_retina_scales() {
    let mut renderer =
        pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap();
    verify_scales(&mut renderer);
}
