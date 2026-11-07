//! Persistent widget ownership, independent of concrete layouts and backends.
#![no_std]
extern crate alloc;
mod animation;
mod children;
mod event;
mod focus;
mod geometry;
mod presentation;
mod reveal;
pub use event::{DispatchResult, TextInputTarget};
mod reconcile;
mod tree;

pub use geometry::Hit;
pub use reconcile::ReconcileReport;
pub use tree::{Tree, TreeError, TreeLimits};
