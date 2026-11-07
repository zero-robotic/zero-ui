use crate::{ButtonView, Container, TextView, action_button::ActionButtonView, column, row};
use alloc::{string::String, vec::Vec};
use core::time::Duration;
use zui_api::{
    action::ActionId,
    layout::{Edges, LayoutError, LayoutKind, LayoutStyle},
    view::{View, ViewSpec},
    widget::*,
};
use zui_theme::{ColorRole, Theme, TypeRole};
use zui_types::{Point, Size};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationKind {
    Info,
    Success,
    Warning,
    Error,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotificationDismissed {
    pub id: u64,
}
#[derive(Clone, PartialEq)]
pub struct NotificationView {
    pub id: u64,
    pub title: String,
    pub message: String,
    pub kind: NotificationKind,
    pub dismiss: Option<ActionId>,
    pub theme: Theme,
    pub families: Vec<String>,
}
impl NotificationView {
    pub fn themed(
        theme: &Theme,
        id: u64,
        title: impl Into<String>,
        message: impl Into<String>,
        families: Vec<String>,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            message: message.into(),
            kind: NotificationKind::Info,
            dismiss: None,
            theme: theme.clone(),
            families,
        }
    }
    pub fn build(&self) -> View {
        let mut title = TextView::themed(
            &self.theme,
            self.title.clone(),
            self.families.clone(),
            TypeRole::Body,
        );
        title.color = match self.kind {
            NotificationKind::Error => self.theme.color(ColorRole::Danger),
            NotificationKind::Warning => self.theme.color(ColorRole::Warning),
            NotificationKind::Success => self.theme.color(ColorRole::Success),
            NotificationKind::Info => self.theme.color(ColorRole::Accent),
        };
        let mut header = alloc::vec![View::new(title)];
        header.push(Container::empty().style(|s| s.grow = 1.0).build());
        if let Some(action) = self.dismiss {
            header.push(View::new(ActionButtonView {
                button: ButtonView::themed(&self.theme, "关闭", self.families.clone(), action),
                payload: NotificationDismissed { id: self.id },
            }));
        }
        column([
            row(header)
                .style(|s| {
                    s.wrap = true;
                    s.gap = Size::new(8.0, 4.0);
                })
                .build(),
            View::new(TextView::themed(
                &self.theme,
                self.message.clone(),
                self.families.clone(),
                TypeRole::Caption,
            )),
        ])
        .background(self.theme.color(ColorRole::SurfaceRaised))
        .style(|s| {
            s.padding = Edges::all(12.0);
            s.gap = Size::new(0.0, 6.0);
        })
        .build()
    }
}
#[derive(Clone, PartialEq)]
pub struct ToastView {
    pub notification: NotificationView,
    pub duration: Duration,
    pub revision: u64,
}
impl ToastView {
    pub fn new(notification: NotificationView) -> Self {
        Self {
            notification,
            duration: Duration::from_secs(3),
            revision: 0,
        }
    }
    pub fn build(self) -> View {
        View::new(self)
            .styled(LayoutStyle {
                kind: LayoutKind::Column,
                ..Default::default()
            })
            .managed_children()
    }
}
pub struct Toast {
    properties: ToastView,
    start: Option<Duration>,
    expired: bool,
}
impl ViewSpec for ToastView {
    type Widget = Toast;
    fn build(&self) -> Toast {
        Toast {
            properties: self.clone(),
            start: None,
            expired: false,
        }
    }
    fn update(&self, w: &mut Toast) -> Invalidation {
        // Layout owners may regenerate identical descriptions on every pass.
        // An unchanged toast must not keep that layout cycle alive.
        if self == &w.properties {
            return Invalidation::None;
        }
        if self.revision != w.properties.revision
            || self.notification.id != w.properties.notification.id
            || self.duration != w.properties.duration
        {
            w.start = None;
            w.expired = false;
        }
        w.properties = self.clone();
        Invalidation::Layout
    }
}
impl Widget for Toast {
    fn paint(&self, _: PaintContext, _: &mut zui_paint::Painter<'_>) {}
    fn hit_test(&self, _: Point, _: Size) -> bool {
        false
    }
    fn child_views(&self, _: PaintContext) -> Result<Vec<View>, LayoutError> {
        Ok(if self.expired {
            Vec::new()
        } else {
            alloc::vec![self.properties.notification.build().keyed("notification")]
        })
    }
    fn animate(&mut self, elapsed: Duration) -> AnimationUpdate {
        if self.expired {
            return AnimationUpdate::default();
        }
        let start = *self.start.get_or_insert(elapsed);
        let Some(due) = start.checked_add(self.properties.duration) else {
            return AnimationUpdate::default();
        };
        if elapsed >= due {
            self.expired = true;
            AnimationUpdate {
                invalidation: Invalidation::Layout,
                next_frame: None,
            }
        } else {
            AnimationUpdate {
                invalidation: Invalidation::None,
                next_frame: Some(due),
            }
        }
    }
}
