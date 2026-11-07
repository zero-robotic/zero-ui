#![cfg(feature = "worker")]
use std::{
    num::NonZeroUsize,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use zui_image::{DecodeCompletion, DecodeLimits, PngDecodeWorker, SubmitError};
fn png() -> Arc<[u8]> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255, 0, 0, 128]).unwrap();
        writer.finish().unwrap();
    }
    bytes.into()
}
fn receive(worker: &mut PngDecodeWorker) -> DecodeCompletion {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(result) = worker.try_receive().unwrap() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "worker did not complete within deadline"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn outstanding_capacity_includes_unreceived_results_and_ids_support_stale_filtering() {
    let mut worker =
        PngDecodeWorker::new(DecodeLimits::default(), NonZeroUsize::new(2).unwrap()).unwrap();
    assert_eq!(worker.capacity(), 2);
    assert!(worker.try_receive().unwrap().is_none());
    let bytes = png();
    let old = worker.try_submit(bytes.clone()).unwrap();
    let current = worker.try_submit(bytes.clone()).unwrap();
    assert_ne!(old, current);
    assert_eq!(worker.outstanding(), 2);
    assert_eq!(worker.try_submit(bytes.clone()), Err(SubmitError::Full));
    let first = receive(&mut worker);
    assert_eq!(first.id, old);
    // A UI waiting for `current` discards this result instead of showing stale data.
    assert_ne!(first.id, current);
    assert_eq!(worker.outstanding(), 1);
    let last = worker.try_submit(bytes).unwrap();
    assert_ne!(last, current);
    let second = receive(&mut worker);
    assert_eq!(second.id, current);
    assert_eq!(second.result.unwrap().bytes(), &[128, 0, 0, 128]);
    assert_eq!(receive(&mut worker).id, last);
    assert_eq!(worker.outstanding(), 0);
}
#[test]
fn decode_failure_and_submission_rejection_do_not_poison_worker() {
    let bytes = png();
    let limits = DecodeLimits {
        max_encoded_bytes: bytes.len(),
        ..Default::default()
    };
    let mut worker = PngDecodeWorker::new(limits, NonZeroUsize::new(1).unwrap()).unwrap();
    assert_eq!(
        worker.try_submit(vec![0; bytes.len() + 1].into()),
        Err(SubmitError::EncodedTooLarge)
    );
    assert_eq!(worker.outstanding(), 0);
    let bad = worker.try_submit(Arc::from(&b"bad png"[..])).unwrap();
    let result = receive(&mut worker);
    assert_eq!(result.id, bad);
    assert!(result.result.is_err());
    let good = worker.try_submit(bytes).unwrap();
    let result = receive(&mut worker);
    assert_eq!(result.id, good);
    assert!(result.result.is_ok());
    assert_eq!(worker.outstanding(), 0);
}
#[test]
fn dropping_owner_releases_queued_bytes_without_requiring_result_drain() {
    let bytes = png();
    let mut worker =
        PngDecodeWorker::new(DecodeLimits::default(), NonZeroUsize::new(4).unwrap()).unwrap();
    for _ in 0..4 {
        worker.try_submit(bytes.clone()).unwrap();
    }
    drop(worker);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Arc::strong_count(&bytes) != 1 {
        assert!(
            Instant::now() < deadline,
            "worker retained queued bytes after shutdown"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
