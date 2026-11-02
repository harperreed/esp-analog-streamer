// ABOUTME: Checks real encoder allocation lifetime and fixed-format output.
// ABOUTME: Exercises failure cleanup and independent interleaved instances.
use esp_shazam_mp3::{Allocator, Encoder, Error, FRAME_BYTES, PCM_SAMPLES};
use std::ffi::c_void;

#[derive(Default)]
struct Counts {
    calls: usize,
    live: usize,
    fail_at: usize,
    allocations: Vec<(usize, usize)>,
}
unsafe extern "C" fn allocate(ctx: *mut c_void, size: usize) -> *mut c_void {
    let counts = &mut *ctx.cast::<Counts>();
    counts.calls += 1;
    if counts.calls == counts.fail_at {
        return std::ptr::null_mut();
    }
    let layout = std::alloc::Layout::from_size_align(size + 16, 16).unwrap();
    let raw = std::alloc::alloc(layout);
    if raw.is_null() {
        return raw.cast();
    }
    raw.cast::<usize>().write(size);
    counts.live += 1;
    counts.allocations.push((raw.add(16) as usize, size));
    raw.add(16).cast()
}
unsafe extern "C" fn deallocate(ctx: *mut c_void, ptr: *mut c_void) {
    let counts = &mut *ctx.cast::<Counts>();
    let raw = ptr.cast::<u8>().sub(16);
    let size = raw.cast::<usize>().read();
    std::alloc::dealloc(
        raw,
        std::alloc::Layout::from_size_align(size + 16, 16).unwrap(),
    );
    counts.live -= 1;
}
fn allocator(counts: &mut Counts) -> Allocator {
    Allocator {
        context: (counts as *mut Counts).cast(),
        allocate,
        deallocate,
    }
}
#[test]
fn allocation_failure_cleans_each_partial_instance() {
    for fail_at in [1, 2] {
        let mut counts = Counts {
            fail_at,
            ..Counts::default()
        };
        assert!(matches!(
            unsafe { Encoder::with_allocator(allocator(&mut counts)) },
            Err(Error::Allocation)
        ));
        assert_eq!(counts.live, 0);
        assert_eq!(counts.calls, fail_at);
    }
}
#[test]
fn interleaved_instances_allocate_only_at_creation_and_free_on_drop() {
    let mut first = Counts::default();
    let mut second = Counts::default();
    let mut a = unsafe { Encoder::with_allocator(allocator(&mut first)) }.unwrap();
    let mut b = unsafe { Encoder::with_allocator(allocator(&mut second)) }.unwrap();
    assert_eq!((first.live, second.live), (2, 2));
    let mut pcm = [0; PCM_SAMPLES];
    let mut left = [0; FRAME_BYTES];
    let mut right = [0; FRAME_BYTES];
    for _ in 0..8 {
        a.encode(&mut pcm, &mut left).unwrap();
        b.encode(&mut pcm, &mut right).unwrap();
        assert_eq!(left, right);
        assert_eq!(&left[..2], &[0xff, 0xfb]);
        assert_eq!(left[2] >> 4, 14);
        assert_eq!((left[2] >> 2) & 3, 1);
        assert_eq!(left[3] >> 6, 0);
        assert_eq!(left[4], 0);
        assert_eq!(left[5] >> 7, 0);
    }
    assert_eq!((first.calls, second.calls), (2, 2));
    drop(a);
    assert_eq!((first.live, second.live), (0, 2));
    drop(b);
    assert_eq!(second.live, 0);
}
#[test]
fn default_allocator_encodes() {
    let mut encoder = Encoder::new().unwrap();
    encoder
        .encode(&mut [0; PCM_SAMPLES], &mut [0; FRAME_BYTES])
        .unwrap();
}

#[test]
fn independent_noise_matches_normalized_upstream_bytes() {
    // Golden uses pinned upstream with MDCT normalization and compensated headroom.
    // See the October 13 headroom receipt for the independent build.
    let mut encoder = Encoder::new().unwrap();
    let mut state = 0x12345678_u32;
    let mut hash = 0xcbf29ce484222325_u64;
    for _ in 0..16 {
        let mut pcm = [0; PCM_SAMPLES];
        for sample in &mut pcm {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *sample = (state >> 16) as i16;
        }
        let mut output = [0; FRAME_BYTES];
        encoder.encode(&mut pcm, &mut output).unwrap();
        for byte in output {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
    }
    assert_eq!(hash, 0x3d8d37fffb18a62e);
}

#[cfg(feature = "encoder-probe")]
#[test]
fn live_receipt_matches_allocator_and_does_not_change_encoding() {
    let mut counts = Box::new(Counts::default());
    let mut queried = unsafe { Encoder::with_allocator(allocator(&mut counts)) }.unwrap();
    let mut control = Encoder::new().unwrap();
    let receipt = queried.allocation_receipt().unwrap();
    assert_eq!(
        (receipt.config.address, receipt.config.bytes),
        counts.allocations[0]
    );
    assert_eq!(
        (receipt.bitstream.address, receipt.bitstream.bytes),
        counts.allocations[1]
    );
    assert_eq!(receipt.bitstream.bytes, 4096);
    let mut left = [0; FRAME_BYTES];
    let mut right = [0; FRAME_BYTES];
    for _ in 0..8 {
        assert_eq!(queried.allocation_receipt(), Some(receipt));
        queried.encode(&mut [137; PCM_SAMPLES], &mut left).unwrap();
        control.encode(&mut [137; PCM_SAMPLES], &mut right).unwrap();
        assert_eq!(left, right);
    }
    assert_eq!((counts.calls, counts.live), (2, 2));
    drop(queried);
    assert_eq!(counts.live, 0);
}
