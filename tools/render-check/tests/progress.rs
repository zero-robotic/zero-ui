use std::time::Duration;
use zui_api::{
    event::*,
    layout::{Dimension, Edges, LayoutStyle},
    view::View,
    widget::Invalidation,
};
use zui_core::Tree;
use zui_paint::{Frame, OffscreenRenderer};
use zui_theme::Theme;
use zui_types::{Color, Extent, Size};
use zui_widgets::{ProgressAnimation, ProgressBarView, ProgressDirection, ProgressValue};
struct Fixture {
    view: ProgressBarView,
    tree: Tree,
    layout: zui_layout::TaffyLayout,
    size: Size,
}
impl Fixture {
    fn new(direction: ProgressDirection) -> Self {
        let mut view =
            ProgressBarView::themed(&Theme::dashboard(), ProgressValue::Determinate(0.25));
        view.direction = direction;
        view.style.track = Color::rgb(30, 40, 50);
        view.style.fill = Color::rgb(20, 210, 240);
        view.style.disabled_fill = Color::rgb(100, 110, 120);
        let size = if matches!(
            direction,
            ProgressDirection::LeftToRight | ProgressDirection::RightToLeft
        ) {
            Size::new(216.0, 32.0)
        } else {
            Size::new(32.0, 216.0)
        };
        let tree = Tree::from_view(description(view, size), Default::default()).unwrap();
        let mut fixture = Self {
            view,
            tree,
            layout: Default::default(),
            size,
        };
        fixture.prepare();
        fixture
    }
    fn prepare(&mut self) -> bool {
        self.tree
            .layout(&mut self.layout, Size::new(240.0, 240.0))
            .unwrap()
    }
    fn update(&mut self) -> bool {
        self.tree
            .reconcile(description(self.view, self.size))
            .unwrap();
        self.prepare()
    }
}
fn description(view: ProgressBarView, size: Size) -> View {
    View::new(view).keyed("progress").styled(LayoutStyle {
        width: Dimension::Length(size.width as f32),
        height: Dimension::Length(size.height as f32),
        padding: Edges::all(8.0),
        ..Default::default()
    })
}
#[test]
fn progress_value_updates_are_paint_only_and_the_control_does_not_take_focus() {
    let mut f = Fixture::new(ProgressDirection::LeftToRight);
    let root = f.tree.root();
    f.view.progress = ProgressValue::Determinate(0.75);
    assert!(!f.update());
    assert_eq!(f.tree.root(), root);
    assert_eq!(
        f.tree
            .advance_animations(Duration::ZERO)
            .unwrap()
            .next_frame,
        None
    );
    let result = f
        .tree
        .dispatch_input(InputEvent::Key(KeyInput {
            key: Key::Tab,
            pressed: true,
            repeat: false,
            text: None,
            modifiers: Default::default(),
        }))
        .unwrap();
    assert!(result.actions.is_empty());
    assert!(f.tree.focused().is_none());
    f.view.direction = ProgressDirection::RightToLeft;
    assert!(f.update());
}
#[test]
fn busy_animation_preserves_phase_across_theme_changes_and_stops_when_inactive() {
    let mut f = Fixture::new(ProgressDirection::LeftToRight);
    f.view.progress = ProgressValue::Indeterminate;
    f.update();
    assert_eq!(
        f.tree
            .advance_animations(Duration::ZERO)
            .unwrap()
            .next_frame,
        Some(Duration::from_millis(33))
    );
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_millis(375))
            .unwrap()
            .invalidation,
        Invalidation::Paint
    );
    f.view.style.fill = Color::WHITE;
    assert!(!f.update());
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_millis(375))
            .unwrap()
            .invalidation,
        Invalidation::None
    );
    f.view.disabled = true;
    f.update();
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_millis(400))
            .unwrap()
            .next_frame,
        None
    );
    f.view.disabled = false;
    f.view.animation = None;
    f.update();
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_millis(500))
            .unwrap()
            .next_frame,
        None
    );
    f.view.animation = Some(ProgressAnimation::default());
    f.size.width = 0.0;
    f.update();
    assert_eq!(
        f.tree
            .advance_animations(Duration::from_millis(600))
            .unwrap()
            .next_frame,
        None
    );
    f.size.width = 216.0;
    f.update();
    assert_eq!(
        f.tree.advance_animations(Duration::MAX).unwrap().next_frame,
        None
    );
}
#[test]
fn invalid_progress_and_motion_cannot_publish_geometry_and_can_recover() {
    let mut f = Fixture::new(ProgressDirection::LeftToRight);
    let valid = f.view;
    let mut cases = Vec::new();
    for value in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        let mut view = valid;
        view.progress = ProgressValue::Determinate(value);
        cases.push(view);
    }
    for fraction in [0.0, 1.0, f64::NAN] {
        let mut view = valid;
        view.style.segment_fraction = fraction;
        cases.push(view);
    }
    let mut view = valid;
    view.animation = Some(ProgressAnimation {
        cycle: Duration::ZERO,
        frame_interval: Duration::from_millis(16),
    });
    cases.push(view);
    let mut view = valid;
    view.style.thickness = f32::NAN;
    cases.push(view);
    for view in cases {
        f.tree.reconcile(description(view, f.size)).unwrap();
        assert!(
            f.tree
                .layout(&mut f.layout, Size::new(240.0, 240.0))
                .is_err()
        );
        assert!(f.tree.scene().is_err());
        f.view = valid;
        assert!(f.update());
    }
}
fn verify(renderer: &mut impl OffscreenRenderer) {
    for direction in [
        ProgressDirection::LeftToRight,
        ProgressDirection::RightToLeft,
        ProgressDirection::BottomToTop,
        ProgressDirection::TopToBottom,
    ] {
        let mut f = Fixture::new(direction);
        let horizontal = matches!(
            direction,
            ProgressDirection::LeftToRight | ProgressDirection::RightToLeft
        );
        let reverse = matches!(
            direction,
            ProgressDirection::RightToLeft | ProgressDirection::BottomToTop
        );
        for value in [0.0, 0.25, 1.0] {
            f.view.progress = ProgressValue::Determinate(value);
            f.update();
            for scale in [1.0, 2.0] {
                let pixels = renderer
                    .render(
                        &f.tree.scene().unwrap(),
                        Frame {
                            extent: Extent::new((240.0 * scale) as u32, (240.0 * scale) as u32),
                            scale,
                            background: Color::BLACK,
                        },
                    )
                    .unwrap();
                for along in [30.0, 100.0, 185.0] {
                    let filled = value == 1.0
                        || (value == 0.25 && if reverse { along > 158.0 } else { along < 58.0 });
                    let (x, y) = if horizontal {
                        (along, 16.0)
                    } else {
                        (16.0, along)
                    };
                    let offset = (y * scale) as usize * pixels.stride() + (x * scale) as usize * 4;
                    let c = if filled {
                        f.view.style.fill
                    } else {
                        f.view.style.track
                    };
                    assert_eq!(&pixels.pixels()[offset..offset + 4], &[c.r, c.g, c.b, 255]);
                }
                assert_eq!(&pixels.pixels()[0..4], &[0, 0, 0, 255]);
            }
        }
    }
    let mut f = Fixture::new(ProgressDirection::LeftToRight);
    f.view.progress = ProgressValue::Indeterminate;
    f.update();
    let frame = Frame {
        extent: Extent::new(240, 40),
        scale: 1.0,
        background: Color::BLACK,
    };
    f.tree.advance_animations(Duration::ZERO).unwrap();
    let start = renderer.render(&f.tree.scene().unwrap(), frame).unwrap();
    f.tree
        .advance_animations(Duration::from_millis(750))
        .unwrap();
    let end = renderer.render(&f.tree.scene().unwrap(), frame).unwrap();
    assert_ne!(start.pixels(), end.pixels());
    f.view.disabled = true;
    f.update();
    let disabled = renderer.render(&f.tree.scene().unwrap(), frame).unwrap();
    let c = f.view.style.disabled_fill;
    let offset = 16 * disabled.stride() + 108 * 4;
    assert_eq!(
        &disabled.pixels()[offset..offset + 4],
        &[c.r, c.g, c.b, 255]
    );
}
#[cfg(feature = "cpu")]
#[test]
fn progress_directions_endpoints_and_animation_render_cpu() {
    verify(&mut zui_render_cpu::CpuRenderer::default());
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires a real GPU adapter; explicitly run --ignored"]
fn progress_directions_endpoints_and_animation_render_gpu() {
    verify(&mut pollster::block_on(zui_render_gpu::GpuRenderer::new(Default::default())).unwrap());
}
