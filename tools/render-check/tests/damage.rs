use std::sync::Arc;
use zui_paint::{Frame, OffscreenRenderer, Path, Scene};
use zui_types::{Affine, Color, Extent, Rect};

fn frame(scale: f64) -> Frame {
    Frame {
        extent: Extent::new((320.0 * scale) as u32, (240.0 * scale) as u32),
        scale,
        background: Color::rgba(12, 30, 42, 180),
    }
}

fn scenes() -> Vec<Scene> {
    let image = zui_paint::ImageResource::from_rgba8(
        Extent::new(2, 2),
        vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0],
        Default::default(),
    )
    .unwrap();
    let font = zui_paint::FontResource::new(
        Arc::from(include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice()),
        0,
    )
    .unwrap();
    let mut result = Vec::new();
    for step in 0..12 {
        let mut scene = Scene::new();
        let mut painter = scene.painter();
        painter.fill_rect(
            Rect::new(0.0, 0.0, 320.0, 240.0),
            Color::rgba(80, 60, 30, 80),
        );
        painter.with_clip(
            Path::rounded_rectangle(
                Rect::new(10.25, 10.5, if step == 6 { 92.0 } else { 170.0 }, 125.0),
                12.0,
            )
            .unwrap(),
            |painter| {
                painter.with_transform(Affine([1.0, 0.1, 0.2, 1.0, 3.25, 5.5]), |painter| {
                    if step != 4 {
                        painter.fill(
                            Path::rounded_rectangle(
                                Rect::new(if step == 3 { 80.0 } else { 24.3 }, 24.4, 30.5, 26.5),
                                5.5,
                            )
                            .unwrap(),
                            if step == 1 {
                                Color::rgba(10, 180, 130, 128)
                            } else {
                                Color::rgba(230, 70, 10, 128)
                            },
                        );
                    }
                    painter.stroke(
                        Path::rectangle(Rect::new(20.0, 20.0, 80.0, 65.0)),
                        Color::rgba(30, 140, 200, 170),
                        if step == 7 { 5.25 } else { 2.0 },
                    );
                    painter.with_transform(
                        Affine::translate(if step == 8 { 100.0 } else { 60.0 }, 40.0)
                            .compose(Affine::scale(20.0, 15.0)),
                        |painter| {
                            painter.image(image.clone(), zui_paint::ImageSampling::Bilinear);
                        },
                    );
                });
            },
        );
        if step != 5 {
            painter.fill_rect(
                Rect::new(35.5, 33.25, 75.0, 20.0),
                Color::rgba(120, 70, 230, 70),
            );
        }
        painter.glyphs(
            zui_paint::GlyphRun {
                font: font.clone(),
                font_size: 18.0,
                glyphs: vec![
                    zui_paint::Glyph {
                        id: if step == 9 { 20 } else { 21 },
                        x: 220.25,
                        y: 80.75,
                    },
                    zui_paint::Glyph {
                        id: 1,
                        x: 235.0,
                        y: 80.75,
                    },
                ],
                normalized_coords: vec![6390],
            },
            if step == 10 {
                Color::rgb(80, 210, 100)
            } else {
                Color::WHITE
            },
        );
        result.push(scene);
    }
    result
}

fn same_pixels(expected: &[u8], actual: &[u8], label: &str) {
    assert_eq!(expected.len(), actual.len());
    let mismatches = expected
        .iter()
        .zip(actual)
        .filter(|(left, right)| left != right)
        .count();
    let first: Vec<_> = expected
        .iter()
        .zip(actual)
        .enumerate()
        .filter(|(_, (left, right))| left != right)
        .take(12)
        .collect();
    let maximum = expected
        .iter()
        .zip(actual)
        .map(|(left, right)| left.abs_diff(*right))
        .max()
        .unwrap_or(0);
    assert!(
        maximum <= 1,
        "{label}: {mismatches} differing channels, max={maximum}, first={first:?}"
    );
}

#[cfg(feature = "cpu")]
#[test]
fn damage_matches_full_cpu_with_alpha_clips_motion_removal_text_and_scales() {
    let mut retained = zui_render_cpu::RetainedCpuRenderer::default();
    let mut full = zui_render_cpu::CpuRenderer::default();
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for (step, scene) in scenes().iter().enumerate() {
            let expected = full.render(scene, frame(scale)).unwrap();
            let actual = retained.render(scene, frame(scale)).unwrap();
            same_pixels(
                expected.pixels(),
                actual.pixels(),
                &format!("scale {scale}, step {step}"),
            );
            if step == 1 {
                let stats = retained.damage_stats();
                assert!(
                    stats.rasterized_pixels < stats.total_pixels / 4,
                    "{stats:?}"
                );
            }
        }
    }
}

#[cfg(feature = "cpu")]
#[test]
fn cache_budgets_fall_back_and_invisible_changes_do_not_rasterize() {
    let mut retained = zui_render_cpu::RetainedCpuRenderer::default();
    let mut scene = Scene::new();
    retained.render(&scene, frame(1.0)).unwrap();
    scene.painter().with_clip(
        Path::rectangle(Rect::new(0.0, 0.0, 10.0, 10.0)),
        |painter| {
            painter.fill_rect(Rect::new(200.0, 200.0, 10.0, 10.0), Color::WHITE);
        },
    );
    retained.render(&scene, frame(1.0)).unwrap();
    assert_eq!(retained.damage_stats().rasterized_pixels, 0);
    for config in [
        zui_paint::DamageConfig {
            max_scene_bytes: 0,
            ..Default::default()
        },
        zui_paint::DamageConfig {
            max_pixel_bytes: 0,
            ..Default::default()
        },
    ] {
        let mut limited =
            zui_render_cpu::RetainedCpuRenderer::with_damage_config(Default::default(), config);
        for _ in 0..2 {
            limited.render(&scene, frame(1.0)).unwrap();
            assert_eq!(limited.damage_stats().rasterized_pixels, 320 * 240);
        }
    }
}

#[cfg(feature = "cpu")]
#[test]
fn shaped_text_color_and_content_changes_remain_local() {
    use zui_api::{
        layout::{Dimension, LayoutStyle},
        view::View,
    };
    let mut text = zui_text::ParleyText::default();
    let families = text
        .register_font(Arc::from(
            include_bytes!("../../../assets/fonts/NotoSansSC.ttf").as_slice(),
        ))
        .unwrap();
    let make = |content: &str| {
        View::new(zui_widgets::TextView::themed(
            &zui_theme::Theme::desktop_dark(),
            content,
            families.clone(),
            zui_theme::TypeRole::Body,
        ))
        .styled(LayoutStyle {
            width: Dimension::Length(100.0),
            height: Dimension::Length(30.0),
            ..Default::default()
        })
    };
    let mut tree = zui_core::Tree::from_view(make("T000: 00001"), Default::default()).unwrap();
    let mut retained = zui_render_cpu::RetainedCpuRenderer::default();
    let mut full = zui_render_cpu::CpuRenderer::default();
    for content in ["T000: 00001", "T000: 00002"] {
        tree.reconcile(make(content)).unwrap();
        tree.layout_with_text(
            &mut zui_layout::TaffyLayout::default(),
            &mut text,
            zui_types::Size::new(320.0, 240.0),
        )
        .unwrap();
        let scene = tree.scene().unwrap();
        same_pixels(
            full.render(&scene, frame(1.0)).unwrap().pixels(),
            retained.render(&scene, frame(1.0)).unwrap().pixels(),
            "shaped text",
        );
    }
    assert!(
        retained.damage_stats().rasterized_pixels < 320 * 240 / 4,
        "{:?}, {:?}",
        retained.damage_stats(),
        tree.scene().unwrap().commands()
    );
}

#[cfg(feature = "cpu")]
#[test]
fn disjoint_damage_preserves_untouched_pixels_and_region_limit_falls_back() {
    let make = |color| {
        let mut scene = Scene::new();
        scene
            .painter()
            .fill_rect(Rect::new(8.0, 8.0, 12.0, 12.0), color);
        scene
            .painter()
            .fill_rect(Rect::new(270.0, 200.0, 12.0, 12.0), color);
        scene
    };
    for maximum in [1, 24] {
        let config = zui_paint::DamageConfig {
            max_regions: maximum,
            ..Default::default()
        };
        let mut retained =
            zui_render_cpu::RetainedCpuRenderer::with_damage_config(Default::default(), config);
        retained.render(&make(Color::WHITE), frame(1.0)).unwrap();
        let changed = make(Color::rgb(90, 160, 230));
        let expected = zui_render_cpu::CpuRenderer::default()
            .render(&changed, frame(1.0))
            .unwrap();
        assert!(expected.pixels() == retained.render(&changed, frame(1.0)).unwrap().pixels());
        let stats = retained.damage_stats();
        if maximum == 1 {
            assert_eq!(stats.rasterized_pixels, stats.total_pixels);
        } else {
            assert_eq!(stats.regions, 2);
            assert_eq!(stats.rasterized_pixels, 4096);
        }
    }
}

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a GPU adapter"]
fn damage_matches_full_gpu_with_alpha_clips_motion_removal_text_and_scales() {
    let mut renderer =
        pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap();
    for scale in [1.0, 1.25, 1.5, 2.0] {
        for (step, scene) in scenes().iter().enumerate() {
            let expected = renderer.render(scene, frame(scale)).unwrap();
            let actual = renderer.render_retained(scene, frame(scale)).unwrap();
            same_pixels(
                expected.pixels(),
                actual.pixels(),
                &format!("scale {scale}, step {step}"),
            );
            if step == 1 {
                let stats = renderer.damage_stats();
                assert!(
                    stats.rasterized_pixels < stats.total_pixels / 4,
                    "{stats:?}"
                );
            }
        }
    }
}
