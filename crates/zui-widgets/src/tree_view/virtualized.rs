use super::*;
use alloc::rc::Rc;
use zui_api::layout::{Dimension, LayoutKind, LayoutStyle};
pub(super) fn build(view: &TreeView, row_height: f64, height: f32) -> Result<View, TreeViewError> {
    if !row_height.is_finite() || row_height <= 0.0 || !height.is_finite() || height <= 0.0 {
        return Err(TreeViewError::InvalidViewport);
    }
    if view.draggable {
        return Err(TreeViewError::VirtualDragUnsupported);
    }
    let spec = Rc::new(view.specification()?);
    let rows = Rc::new(spec.rows.clone());
    let source = spec.clone();
    let mut template = spec.list.clone();
    template.items.clear();
    template.multi_selection = None;
    let body = crate::VirtualListView::new(rows.len(), row_height, move |index| {
        let r = source.rows[index].clone();
        let mut list = template.clone();
        list.items = alloc::vec![source.list.items[index].clone()];
        list.selected = list.selected.filter(|id| id.0 == r.id.0);
        list.multi_selection = source.selection.as_ref().map(|ids| {
            ids.iter()
                .filter(|id| **id == r.id)
                .map(|id| ListItemId(id.0))
                .collect()
        });
        let id = r.id.0;
        View::new(TreeSpec {
            list,
            rows: alloc::vec![r],
            selection: source.selection.clone(),
            draggable: false,
            double_click_interval: source.double_click_interval,
            global_rows: Some(rows.clone()),
        })
        .keyed(id)
        .styled(LayoutStyle {
            height: Dimension::Length(row_height as f32),
            width: Dimension::Percent(1.0),
            ..Default::default()
        })
    })
    .style(|s| {
        s.height = Dimension::Length(height);
        s.width = Dimension::Percent(1.0);
    })
    .disabled(spec.list.disabled);
    let body = if let Some((index, id)) = spec.list.selected.and_then(|id| {
        spec.rows
            .iter()
            .position(|r| r.id.0 == id.0)
            .map(|i| (i, id))
    }) {
        body.scroll_to_index(
            index,
            crate::ScrollAlignment::Nearest,
            id.0.wrapping_add((index as u64).rotate_left(32)),
        )
    } else {
        body
    };
    Ok(View::new(NavigationView(spec))
        .styled(LayoutStyle {
            kind: LayoutKind::Stack,
            height: Dimension::Length(height),
            ..Default::default()
        })
        .with_children([body.build()]))
}
struct NavigationView(Rc<TreeSpec>);
struct Navigation {
    spec: Rc<TreeSpec>,
    anchor: Option<TreeItemId>,
}
impl ViewSpec for NavigationView {
    type Widget = Navigation;
    fn build(&self) -> Navigation {
        Navigation {
            spec: self.0.clone(),
            anchor: None,
        }
    }
    fn update(&self, w: &mut Navigation) -> Invalidation {
        w.spec = self.0.clone();
        Invalidation::Paint
    }
}
impl Widget for Navigation {
    fn enabled(&self) -> bool {
        !self.spec.list.disabled
    }
    fn focusable(&self) -> bool {
        self.enabled()
    }
    fn paint(&self, _: PaintContext, _: &mut Painter<'_>) {}
    fn event(&mut self, c: EventContext, e: &WidgetEvent) -> EventResponse {
        if !self.enabled() {
            return EventResponse::default();
        }
        if c.phase == EventPhase::Capture
            && matches!(
                e,
                WidgetEvent::Input(InputEvent::PointerButton {
                    button: PointerButton::Primary,
                    pressed: true,
                    ..
                })
            )
        {
            self.anchor = None;
            return EventResponse {
                focus: FocusRequest::Focus,
                ..Default::default()
            };
        }
        let WidgetEvent::Input(InputEvent::Key(k)) = e else {
            return EventResponse::default();
        };
        if !matches!(c.phase, EventPhase::Target | EventPhase::Capture)
            || !k.pressed
            || k.modifiers.alt
            || k.modifiers.super_key
        {
            return EventResponse::default();
        }
        let rows = &self.spec.rows;
        let active = self
            .spec
            .list
            .selected
            .and_then(|id| rows.iter().position(|r| r.id.0 == id.0 && !r.disabled))
            .or_else(|| rows.iter().position(|r| !r.disabled));
        let Some(index) = active else {
            return EventResponse::default();
        };
        let current = &rows[index];
        let action = self.spec.list.action;
        if k.modifiers.control {
            if matches!(&k.key,Key::Character(s) if s.eq_ignore_ascii_case("a"))
                && self.spec.selection.is_some()
            {
                return EventResponse {
                    handled: true,
                    actions: alloc::vec![WidgetAction::new(
                        action,
                        TreeSelectionChanged {
                            selected: rows.iter().filter(|r| !r.disabled).map(|r| r.id).collect()
                        }
                    )],
                    ..Default::default()
                };
            }
            return EventResponse::default();
        }
        let mut expand = None;
        let target = match k.key {
            Key::ArrowUp => (0..index).rev().find(|&i| !rows[i].disabled),
            Key::ArrowDown => (index + 1..rows.len()).find(|&i| !rows[i].disabled),
            Key::Home => rows.iter().position(|r| !r.disabled),
            Key::End => rows.iter().rposition(|r| !r.disabled),
            Key::PageUp => (0..=index.saturating_sub(10))
                .rev()
                .find(|&i| !rows[i].disabled),
            Key::PageDown => {
                ((index + 10).min(rows.len() - 1)..rows.len()).find(|&i| !rows[i].disabled)
            }
            Key::ArrowRight if current.branch && !current.expanded => {
                expand = Some(true);
                None
            }
            Key::ArrowRight => rows
                .iter()
                .position(|r| r.parent == Some(current.id) && !r.disabled),
            Key::ArrowLeft if current.branch && current.expanded => {
                expand = Some(false);
                None
            }
            Key::ArrowLeft => current
                .parent
                .and_then(|id| rows.iter().position(|r| r.id == id && !r.disabled)),
            Key::Enter | Key::Space if current.branch => {
                expand = Some(!current.expanded);
                None
            }
            Key::Enter | Key::Space => Some(index),
            _ => return EventResponse::default(),
        };
        let mut response = EventResponse {
            handled: true,
            stop_propagation: true,
            focus: FocusRequest::Focus,
            ..Default::default()
        };
        if let Some(expanded) = expand
            && !k.repeat
        {
            response.actions.push(WidgetAction::new(
                action,
                TreeEvent::Expansion {
                    id: current.id,
                    expanded,
                },
            ));
            if expanded && current.load {
                response.actions.push(WidgetAction::new(
                    action,
                    TreeLoadRequested { id: current.id },
                ));
            }
        }
        if let Some(target) = target {
            response.actions.push(WidgetAction::new(
                action,
                TreeEvent::Selected(rows[target].id),
            ));
            if let Some(previous) = &self.spec.selection {
                let mut selected: BTreeSet<_> = previous
                    .iter()
                    .filter(|id| !rows.iter().any(|r| r.id == **id))
                    .copied()
                    .collect();
                if k.modifiers.shift {
                    let anchor = *self.anchor.get_or_insert(current.id);
                    let from = rows.iter().position(|r| r.id == anchor).unwrap_or(index);
                    selected.extend(
                        rows[from.min(target)..=from.max(target)]
                            .iter()
                            .filter(|r| !r.disabled)
                            .map(|r| r.id),
                    );
                } else {
                    self.anchor = None;
                    selected.insert(rows[target].id);
                }
                response
                    .actions
                    .push(WidgetAction::new(action, TreeSelectionChanged { selected }));
            }
        }
        response
    }
}
