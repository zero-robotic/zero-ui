//! Optional host-side wall-clock accounting. Portable layers do not read clocks.
use std::time::{Duration, Instant};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum RefreshPhase {
    View,
    Reconcile,
    Layout,
    Input,
    Animation,
    Scene,
    Present,
}
impl RefreshPhase {
    pub const ALL: [Self; 7] = [
        Self::View,
        Self::Reconcile,
        Self::Layout,
        Self::Input,
        Self::Animation,
        Self::Scene,
        Self::Present,
    ];
}
#[derive(Clone, Copy, Debug, Default)]
pub struct PhaseTiming {
    pub calls: u64,
    pub total: Duration,
    pub max: Duration,
}
#[derive(Clone, Debug, Default)]
pub struct RefreshMetrics {
    pub phases: [PhaseTiming; 7],
    pub rebuild_requests: u64,
    /// Requests merged into an already pending root rebuild.
    pub coalesced_rebuilds: u64,
    pub layouts: u64,
}
impl RefreshMetrics {
    pub fn phase(&self, phase: RefreshPhase) -> PhaseTiming {
        self.phases[phase as usize]
    }
    pub(crate) fn record(&mut self, phase: RefreshPhase, duration: Duration) {
        let entry = &mut self.phases[phase as usize];
        entry.calls = entry.calls.saturating_add(1);
        entry.total = entry.total.saturating_add(duration);
        entry.max = entry.max.max(duration);
    }
}
#[derive(Default)]
pub(crate) struct Profiler {
    pub enabled: bool,
    pub metrics: RefreshMetrics,
}
impl Profiler {
    pub fn start(&self) -> Option<Instant> {
        self.enabled.then(Instant::now)
    }
    pub fn finish(&mut self, phase: RefreshPhase, start: Option<Instant>) {
        if let Some(start) = start {
            self.metrics.record(phase, start.elapsed());
        }
    }
}
