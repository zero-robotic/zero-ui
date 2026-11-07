//! Optional single-thread decoder with bounded outstanding requests.
use crate::{DecodeError, DecodeLimits, decode_png};
use std::{
    fmt, io,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    },
    thread,
};
use zui_paint::ImageResource;

/// Identity scoped to the PngDecodeWorker that issued it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DecodeRequestId(u64);
#[derive(Debug)]
pub struct DecodeCompletion {
    pub id: DecodeRequestId,
    pub result: Result<ImageResource, DecodeError>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    Full,
    Closed,
    EncodedTooLarge,
    IdentityExhausted,
}
impl fmt::Display for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Full => "decode worker outstanding request limit reached",
            Self::Closed => "decode worker closed",
            Self::EncodedTooLarge => "encoded image exceeds byte limit",
            Self::IdentityExhausted => "decode request identities exhausted",
        })
    }
}
impl std::error::Error for SubmitError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeWorkerClosed;
impl fmt::Display for DecodeWorkerClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("decode worker closed before all results were received")
    }
}
impl std::error::Error for DecodeWorkerClosed {}
struct Request {
    id: DecodeRequestId,
    bytes: Arc<[u8]>,
}

/// Submission and polling never wait for decoding. Capacity includes queued,
/// executing and completed-but-unreceived requests, not just the input queue.
/// Drop signals shutdown without joining: the current decode may finish, but
/// remaining queued requests are discarded and no UI callback is invoked.
pub struct PngDecodeWorker {
    requests: SyncSender<Request>,
    completions: Receiver<DecodeCompletion>,
    stopped: Arc<AtomicBool>,
    limits: DecodeLimits,
    capacity: usize,
    outstanding: usize,
    next: u64,
}
impl PngDecodeWorker {
    pub fn new(limits: DecodeLimits, capacity: NonZeroUsize) -> io::Result<Self> {
        let (requests, input) = mpsc::sync_channel::<Request>(capacity.get());
        let (output, completions) = mpsc::sync_channel(capacity.get());
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        thread::Builder::new()
            .name("zui-png-decoder".into())
            .spawn(move || {
                while let Ok(request) = input.recv() {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let result = decode_png(&request.bytes, limits);
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    if output
                        .send(DecodeCompletion {
                            id: request.id,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests,
            completions,
            stopped,
            limits,
            capacity: capacity.get(),
            outstanding: 0,
            next: 0,
        })
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn outstanding(&self) -> usize {
        self.outstanding
    }
    /// Keep an Arc clone if the caller needs to retry a rejected submission.
    pub fn try_submit(&mut self, bytes: Arc<[u8]>) -> Result<DecodeRequestId, SubmitError> {
        if bytes.len() > self.limits.max_encoded_bytes {
            return Err(SubmitError::EncodedTooLarge);
        }
        if self.outstanding == self.capacity {
            return Err(SubmitError::Full);
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or(SubmitError::IdentityExhausted)?;
        let id = DecodeRequestId(self.next);
        self.requests
            .try_send(Request { id, bytes })
            .map_err(|error| match error {
                TrySendError::Full(_) => SubmitError::Full,
                TrySendError::Disconnected(_) => SubmitError::Closed,
            })?;
        self.next = next;
        self.outstanding += 1;
        Ok(id)
    }
    /// None means no result is ready; applications choose their own wake/poll policy.
    /// Compare completion IDs with the currently wanted request before publishing.
    pub fn try_receive(&mut self) -> Result<Option<DecodeCompletion>, DecodeWorkerClosed> {
        match self.completions.try_recv() {
            Ok(completion) => {
                self.outstanding -= 1;
                Ok(Some(completion))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(DecodeWorkerClosed),
        }
    }
}
impl Drop for PngDecodeWorker {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}
