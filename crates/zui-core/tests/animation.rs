use core::time::Duration;
use zui_api::{
    layout::{LayoutKind, LayoutStyle},
    widget::{AnimationUpdate, Invalidation, PaintContext, Widget},
};
use zui_core::{Tree, TreeError};
use zui_paint::Painter;
struct Timed {
    deadline: Option<Duration>,
    invalid: bool,
}
impl Widget for Timed {
    fn animate(&mut self, elapsed: Duration) -> AnimationUpdate {
        AnimationUpdate {
            invalidation: Invalidation::Paint,
            next_frame: if self.invalid {
                Some(elapsed)
            } else {
                self.deadline.filter(|time| *time > elapsed)
            },
        }
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
}
#[test]
fn deadlines_merge_and_removed_nodes_cannot_request_more_frames() {
    let mut tree = Tree::new(
        Timed {
            deadline: Some(Duration::from_secs(2)),
            invalid: false,
        },
        LayoutStyle {
            kind: LayoutKind::Column,
            ..Default::default()
        },
        Default::default(),
    )
    .unwrap();
    let child = tree
        .insert(
            tree.root(),
            0,
            Timed {
                deadline: Some(Duration::from_secs(1)),
                invalid: false,
            },
            Default::default(),
        )
        .unwrap();
    assert_eq!(
        tree.advance_animations(Duration::ZERO).unwrap().next_frame,
        Some(Duration::from_secs(1))
    );
    tree.remove(child).unwrap();
    assert_eq!(
        tree.advance_animations(Duration::ZERO).unwrap().next_frame,
        Some(Duration::from_secs(2))
    );
    assert_eq!(
        tree.advance_animations(Duration::from_secs(2))
            .unwrap()
            .next_frame,
        None
    );
    tree.update_widget::<Timed>(tree.root(), Invalidation::None, |widget| {
        widget.invalid = true
    })
    .unwrap();
    assert_eq!(
        tree.advance_animations(Duration::from_secs(3)),
        Err(TreeError::InvalidAnimationDeadline(tree.root()))
    );
}
