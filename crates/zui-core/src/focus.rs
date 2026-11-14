//! Default traversal and container interception, independent of concrete controls.
use crate::Tree;
use alloc::vec::Vec;
use zui_api::{
    action::ActionEvent,
    event::{FocusEntry, FocusNavigation},
};
use zui_types::NodeId;

impl Tree {
    fn next_focus(&self, reverse: bool) -> Option<NodeId> {
        let mut nodes: Vec<_> = self
            .subtree(self.focus_scopes.last().map_or(self.root, |(id, _)| *id))
            .into_iter()
            .filter(|id| self.nodes[id].widget.enabled() && self.nodes[id].widget.focusable())
            .collect();
        if nodes.len() > 1
            && self
                .focus_scopes
                .last()
                .is_some_and(|(id, _)| nodes.first() == Some(id))
        {
            nodes.remove(0);
        }
        if nodes.is_empty() {
            return None;
        }
        let next = match self
            .focused()
            .and_then(|id| nodes.iter().position(|&candidate| candidate == id))
        {
            Some(index) if reverse => (index + nodes.len() - 1) % nodes.len(),
            Some(index) => (index + 1) % nodes.len(),
            None if reverse => nodes.len() - 1,
            None => 0,
        };
        Some(nodes[next])
    }

    fn focus_branch(&self, ancestor: NodeId, mut target: NodeId) -> Option<NodeId> {
        while let Some(parent) = self.nodes[&target].parent {
            if parent == ancestor {
                return Some(target);
            }
            target = parent;
        }
        None
    }

    fn focus_path(&self, mut id: NodeId) -> Vec<NodeId> {
        let mut path = Vec::new();
        loop {
            path.push(id);
            let Some(parent) = self.nodes[&id].parent else {
                break;
            };
            id = parent;
        }
        path.reverse();
        path
    }

    fn enter_focus(
        &mut self,
        candidate: NodeId,
        reverse: bool,
        actions: &mut Vec<ActionEvent>,
    ) -> bool {
        let focused = self.focused();
        let source = focused.map(|id| self.focus_path(id)).unwrap_or_default();
        let path = self.focus_path(candidate);
        let shared = source.iter().zip(&path).take_while(|(a, b)| a == b).count();
        for (index, &id) in path.iter().enumerate().skip(shared) {
            let node = self.nodes.get_mut(&id).expect("stable focus entry path");
            if !node.widget.enabled() {
                continue;
            }
            let response = node.widget.enter_focus(FocusEntry {
                reverse,
                focused,
                candidate,
                candidate_child: path.get(index + 1).copied(),
            });
            self.dirty = self.dirty.max(response.invalidation);
            if response.invalidation != zui_api::widget::Invalidation::None {
                self.presentation_cache.invalidate();
                self.paint_dirty.insert(id);
            }
            actions.extend(
                response
                    .actions
                    .into_iter()
                    .map(|action| ActionEvent { source: id, action }),
            );
            if response.handled {
                return true;
            }
        }
        false
    }

    pub(crate) fn navigate_focus(&mut self, reverse: bool, actions: &mut Vec<ActionEvent>) -> bool {
        let candidate = self.next_focus(reverse);
        if let Some(focused) = self.focused() {
            let mut receiver = Some(focused);
            let mut focused_child = None;
            while let Some(id) = receiver {
                let navigation = FocusNavigation {
                    reverse,
                    focused,
                    focused_child,
                    candidate,
                    candidate_child: candidate.and_then(|target| self.focus_branch(id, target)),
                };
                let node = self
                    .nodes
                    .get_mut(&id)
                    .expect("focus path is stable during callbacks");
                receiver = node.parent;
                focused_child = Some(id);
                if !node.widget.enabled() {
                    continue;
                }
                let response = node.widget.navigate_focus(navigation);
                self.dirty = self.dirty.max(response.invalidation);
                if response.invalidation != zui_api::widget::Invalidation::None {
                    self.presentation_cache.invalidate();
                    self.paint_dirty.insert(id);
                }
                actions.extend(
                    response
                        .actions
                        .into_iter()
                        .map(|action| ActionEvent { source: id, action }),
                );
                if response.handled {
                    return true;
                }
            }
        }
        if let Some(candidate) = self.next_focus(reverse)
            && self.enter_focus(candidate, reverse, actions)
        {
            return true;
        }
        // A callback may change its own focusability. Re-evaluate eligibility
        // before applying the default, rather than trusting the offered snapshot.
        if let Some(next) = self.next_focus(reverse) {
            self.set_focus(Some(next));
            true
        } else {
            false
        }
    }
}

impl Tree {
    pub(crate) fn in_focus_scope(&self, mut id: NodeId) -> bool {
        let Some(&(scope, _)) = self.focus_scopes.last() else {
            return true;
        };
        loop {
            if id == scope {
                return true;
            }
            let Some(parent) = self.nodes.get(&id).and_then(|n| n.parent) else {
                return false;
            };
            id = parent;
        }
    }
    pub(crate) fn sync_focus_scopes(&mut self) {
        let scopes: Vec<_> = self
            .subtree(self.root)
            .into_iter()
            .filter(|id| self.nodes[id].widget.enabled() && self.nodes[id].widget.focus_scope())
            .collect();
        let common = scopes
            .iter()
            .zip(&self.focus_scopes)
            .take_while(|(a, (b, _))| **a == *b)
            .count();
        while self.focus_scopes.len() > common {
            let (_, restore) = self.focus_scopes.pop().unwrap();
            let eligible = restore.filter(|id| {
                self.nodes
                    .get(id)
                    .is_some_and(|n| n.widget.enabled() && n.widget.focusable())
                    && self.in_focus_scope(*id)
            });
            self.set_focus(eligible);
        }
        for &scope in &scopes[common..] {
            self.focus_scopes.push((scope, self.focused()));
            self.set_focus(None);
        }
        if !scopes.is_empty() && self.focused().is_none_or(|id| !self.in_focus_scope(id)) {
            let scope = *scopes.last().unwrap();
            let descendants = self.subtree(scope);
            // Prefer a leaf focus target over its focusable scrolling ancestors.
            // Reverse order lets earlier siblings replace later candidates.
            let mut eligible_subtrees = alloc::collections::BTreeSet::new();
            let mut first = None;
            for &id in descendants.iter().rev() {
                let node = &self.nodes[&id];
                let child_focus = node
                    .children
                    .iter()
                    .any(|id| eligible_subtrees.contains(id));
                let own = node.widget.enabled() && node.widget.focusable();
                if own && !child_focus {
                    first = Some(id);
                }
                if own || child_focus {
                    eligible_subtrees.insert(id);
                }
            }
            self.set_focus(first);
        }
        if self
            .pointer_capture()
            .is_some_and(|id| !self.in_focus_scope(id))
        {
            self.set_capture(None);
        }
    }
}
