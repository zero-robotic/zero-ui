//! Realized branch identity to logical row intent; no runtime tree dependency.
use super::*;
use zui_types::NodeId;

/// One logical-row navigation step. Resolve index against the application's
/// current data; indices are not durable business identities across data edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualListNavigate {
    /// None when entering from the list's scroll-focus stop or from outside.
    pub from_index: Option<usize>,
    pub index: usize,
    /// Select the last eligible descendant when true, otherwise the first.
    pub reverse: bool,
}

#[derive(Default)]
pub(super) struct NavigationState {
    rows: Vec<(NodeId, usize)>,
    validated: Option<(usize, Option<Rc<[usize]>>)>,
}
impl NavigationState {
    pub(super) fn validate(
        &mut self,
        rows: &Option<Rc<[usize]>>,
        count: usize,
        id: NodeId,
    ) -> Result<(), LayoutError> {
        if self
            .validated
            .as_ref()
            .is_some_and(|(old_count, old)| *old_count == count && same_rows(old, rows))
        {
            return Ok(());
        }
        if let Some(rows) = rows
            && (rows.last().is_some_and(|last| *last >= count)
                || rows.windows(2).any(|pair| pair[0] >= pair[1]))
        {
            return Err(LayoutError::InvalidStyle(id));
        }
        self.validated = Some((count, rows.clone()));
        Ok(())
    }

    pub(super) fn record(&mut self, children: &[ChildLayout], enabled: bool, generation: u64) {
        self.rows.clear();
        if !enabled {
            self.rows = Vec::new();
            return;
        }
        self.rows.extend(children.iter().filter_map(|child| {
            let tag = child.tag.filter(|tag| tag.revision == generation)?;
            Some((child.id, usize::try_from(tag.value).ok()?))
        }));
    }
    pub(super) fn navigate(
        &self,
        navigation: FocusNavigation,
        action: Option<ActionId>,
        count: usize,
        eligible: Option<&[usize]>,
    ) -> FocusNavigationResponse {
        let Some(action) = action else {
            return Default::default();
        };
        let from_index = match navigation.focused_child {
            Some(child) => {
                // Let ordinary Tab visit all controls inside the current row.
                if navigation.candidate_child == Some(child) {
                    return Default::default();
                }
                let Some((_, index)) = self.rows.iter().find(|(id, _)| *id == child) else {
                    return Default::default();
                };
                Some(*index)
            }
            None if navigation.reverse => return Default::default(),
            None => None,
        };
        let Some(index) = target(eligible, count, from_index, navigation.reverse) else {
            // At either logical boundary, allow traversal outside the list.
            return Default::default();
        };
        FocusNavigationResponse {
            handled: true,
            invalidation: Invalidation::None,
            actions: vec![WidgetAction::new(
                action,
                VirtualListNavigate {
                    from_index,
                    index,
                    reverse: navigation.reverse,
                },
            )],
        }
    }
}

// Forward entry retains the list's ordinary scroll-focus stop. Reverse entry
// must use the logical end, not whichever row happens to be mounted last.
pub(super) fn enter(
    entry: FocusEntry,
    action: Option<ActionId>,
    count: usize,
    eligible: Option<&[usize]>,
) -> FocusNavigationResponse {
    let Some(action) = action.filter(|_| entry.reverse && count > 0) else {
        return Default::default();
    };
    let Some(index) = target(eligible, count, None, true) else {
        return Default::default();
    };
    FocusNavigationResponse {
        handled: true,
        invalidation: Invalidation::None,
        actions: vec![WidgetAction::new(
            action,
            VirtualListNavigate {
                from_index: None,
                index,
                reverse: true,
            },
        )],
    }
}

pub(super) fn same_rows(a: &Option<Rc<[usize]>>, b: &Option<Rc<[usize]>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}

fn target(
    rows: Option<&[usize]>,
    count: usize,
    from: Option<usize>,
    reverse: bool,
) -> Option<usize> {
    if let Some(rows) = rows {
        return match (from, reverse) {
            (None, false) => rows.first().copied(),
            (None, true) => rows.last().copied(),
            (Some(from), false) => rows
                .get(rows.partition_point(|index| *index <= from))
                .copied(),
            (Some(from), true) => rows
                .partition_point(|index| *index < from)
                .checked_sub(1)
                .map(|i| rows[i]),
        };
    }
    match (from, reverse) {
        (None, true) => count.checked_sub(1),
        (None, false) => (count > 0).then_some(0),
        (Some(from), true) => from.checked_sub(1),
        (Some(from), false) => from.checked_add(1).filter(|index| *index < count),
    }
}
