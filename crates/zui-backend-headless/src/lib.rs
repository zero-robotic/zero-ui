//! Deterministic backend for tests and no-display environments.

use std::cell::Cell;
use zui_core::{Id, ScaleFactor, Size, WindowId};
use zui_platform::{Backend, Host, PlatformError, PlatformEvent, WindowOptions};

pub struct HeadlessBackend {
    next_id: u64,
    pending: Vec<PlatformEvent>,
}

impl Default for HeadlessBackend {
    fn default() -> Self {
        Self {
            next_id: 1,
            pending: Vec::new(),
        }
    }
}

pub struct HeadlessHost {
    id: WindowId,
    size: Size,
    scale_factor: ScaleFactor,
    redraws: Cell<u32>,
}

impl HeadlessHost {
    pub fn redraw_count(&self) -> u32 {
        self.redraws.get()
    }
}

impl Host for HeadlessHost {
    fn id(&self) -> WindowId {
        self.id
    }
    fn size(&self) -> Size {
        self.size
    }
    fn scale_factor(&self) -> ScaleFactor {
        self.scale_factor
    }
    fn request_redraw(&self) -> Result<(), PlatformError> {
        self.redraws.set(self.redraws.get() + 1);
        Ok(())
    }
}

impl Backend for HeadlessBackend {
    type Host = HeadlessHost;

    fn create_window(&mut self, options: WindowOptions) -> Result<Self::Host, PlatformError> {
        let id = WindowId(Id::new(self.next_id));
        self.next_id += 1;
        self.pending.push(PlatformEvent::WindowCreated(id));
        Ok(HeadlessHost {
            id,
            size: options.size,
            scale_factor: ScaleFactor::default(),
            redraws: Cell::new(0),
        })
    }

    fn run(mut self, handler: &mut dyn FnMut(PlatformEvent)) -> Result<(), PlatformError> {
        for event in self.pending.drain(..) {
            handler(event);
        }
        handler(PlatformEvent::AboutToWait);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submits_one_frame_without_a_display() {
        let mut backend = HeadlessBackend::default();
        let host = backend.create_window(WindowOptions::default()).unwrap();
        host.request_redraw().unwrap();
        assert_eq!(host.redraw_count(), 1);
        let mut events = Vec::new();
        backend.run(&mut |event| events.push(event)).unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, PlatformEvent::WindowCreated(_))));
    }
}
