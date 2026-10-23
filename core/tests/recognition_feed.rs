// ABOUTME: Exercises the firmware's optional recognition FIR and ring with real stereo samples.
// ABOUTME: Counts thread-local heap activity to protect disabled and steady capture paths.

#[path = "../../firmware/src/recognition_feed.rs"]
mod recognition_feed;

#[path = "support/alloc_count.rs"]
mod alloc_count;

use alloc_count::allocations_during;
use esp_shazam_core::decimate::Decimator;
use recognition_feed::RecognitionFeed;

#[test]
fn disabled_capture_never_allocates_recognition_state_or_processes_the_feed() {
    // A disabled processor must also ignore inputs larger than its configured chunk:
    // running the unconditional FIR would allocate here even if no ring were created.
    let input = [1 << 22; 2 * 480];
    let (ring, allocations) = allocations_during(|| {
        let mut recognition = RecognitionFeed::new(false, 192_000, 16_000, 240);
        recognition.process(&input);
        recognition.reset();
        recognition.retry_reset();
        recognition.shared_ring()
    });
    assert!(ring.is_none());
    assert_eq!(
        allocations, 0,
        "disabled capture allocated recognition state"
    );
}

#[test]
fn enabled_capture_publishes_the_existing_fir_output_across_chunk_boundaries() {
    let input: Vec<i32> = (0..2 * 480)
        .map(|i| ((i * 15_491) % 4_194_304) - 2_097_152)
        .collect();
    // The established FIR is the reference for this integration, independent of
    // the capture wrapper's state, chunking and ring publication.
    let mut expected = Vec::new();
    Decimator::new().process(&input, &mut expected);
    assert_eq!(expected.len(), 160);

    let mut recognition = RecognitionFeed::new(true, 512, 160, 240);
    for input in [&input[..2], &input[2..18], &input[18..496], &input[496..]] {
        recognition.process(input);
    }
    let ring = recognition.shared_ring().unwrap();
    let mut actual = Vec::new();
    ring.lock().unwrap().copy_latest(512, &mut actual);
    assert_eq!(actual, expected);
}

#[test]
fn reset_discards_fir_history_and_partial_decimation_phase() {
    let mut recognition = RecognitionFeed::new(true, 128, 160, 240);
    recognition.process(&[1 << 22; 2 * 64]);
    recognition.reset();
    recognition.process(&[0; 2 * 62]);

    let ring = recognition.shared_ring().unwrap();
    let mut samples = Vec::new();
    ring.lock().unwrap().copy_latest(128, &mut samples);
    // 62 fresh frames complete 20 samples, all silent. A retained partial phase
    // would complete 21, and retained FIR history would mix in old loud input.
    assert_eq!(samples, [0; 20]);
}

#[test]
fn reset_retries_the_real_snapshot_lock_without_waiting_for_new_audio() {
    let mut recognition = RecognitionFeed::new(true, 128, 160, 240);
    recognition.process(&[1 << 22; 2 * 240]);
    let ring = recognition.shared_ring().unwrap();
    let snapshot = ring.lock().unwrap();
    assert_eq!(snapshot.len(), 80);
    recognition.process(&[1 << 22; 2 * 240]);
    recognition.reset();
    recognition.retry_reset();
    assert_eq!(snapshot.len(), 80);
    drop(snapshot);
    recognition.retry_reset();
    assert!(ring.lock().unwrap().is_empty());
}

#[test]
fn capture_reuses_preallocated_buffers_through_waiting_overflow_and_reset() {
    let input = [1 << 22; 2 * 239];
    let silent = [0; 2 * 240];
    let mut recognition = RecognitionFeed::new(true, 512, 160, 240);
    let ring = recognition.shared_ring().unwrap();
    let snapshot = ring.lock().unwrap();
    let (_, allocations) = allocations_during(|| {
        // Uneven chunks exercise all FIR phases; the locked snapshot forces
        // repeated waiting-buffer overflow before an explicit capture gap.
        for _ in 0..8 {
            recognition.process(&input);
        }
        recognition.reset();
        recognition.process(&silent);
        recognition.retry_reset();
        drop(snapshot);
        recognition.retry_reset();
        recognition.process(&silent);
    });
    assert_eq!(allocations, 0, "capture grew recognition buffers");
    let mut samples = Vec::new();
    ring.lock().unwrap().copy_latest(512, &mut samples);
    assert_eq!(samples, [0; 160]);
}
