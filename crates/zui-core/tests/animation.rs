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

struct DeferredAction(bool);
impl Widget for DeferredAction {
    fn animate_with_actions(
        &mut self,
        elapsed: Duration,
        actions: &mut Vec<zui_api::action::WidgetAction>,
    ) -> AnimationUpdate {
        if !self.0 {
            return AnimationUpdate::default();
        }
        if elapsed < Duration::from_secs(1) {
            return AnimationUpdate {
                next_frame: Some(Duration::from_secs(1)),
                ..Default::default()
            };
        }
        self.0 = false;
        actions.push(zui_api::action::WidgetAction::new(
            zui_api::action::ActionId(9),
            42_u32,
        ));
        AnimationUpdate::default()
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
}

#[test]
fn delayed_actions_keep_source_identity_and_removed_widgets_cannot_emit() {
    let mut tree = Tree::new(
        DeferredAction(true),
        LayoutStyle {
            kind: LayoutKind::Column,
            ..Default::default()
        },
        Default::default(),
    )
    .unwrap();
    let removed = tree
        .insert(tree.root(), 0, DeferredAction(true), Default::default())
        .unwrap();
    let mut actions = Vec::new();
    assert_eq!(
        tree.advance_animations_with_actions(Duration::ZERO, &mut actions)
            .unwrap()
            .next_frame,
        Some(Duration::from_secs(1))
    );
    assert!(actions.is_empty());
    tree.remove(removed).unwrap();
    tree.advance_animations_with_actions(Duration::from_secs(1), &mut actions)
        .unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].source, tree.root());
    assert_eq!(actions[0].action.payload::<u32>(), Some(&42));
    actions.clear();
    tree.advance_animations_with_actions(Duration::from_secs(2), &mut actions)
        .unwrap();
    assert!(actions.is_empty());
}
