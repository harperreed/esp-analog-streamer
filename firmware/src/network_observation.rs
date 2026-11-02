// ABOUTME: Counts observed Wi-Fi driver disconnect events within one boot.
// ABOUTME: Uses a bounded wrapping atomic without locking the network lifecycle.

use std::sync::atomic::{AtomicU32, Ordering};

pub(super) struct DisconnectCount(AtomicU32);

impl DisconnectCount {
    pub(super) const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    pub(super) fn observe(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn get(&self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observations_count_once_each_and_reads_do_not_increment() {
        let count = DisconnectCount::new();
        assert_eq!(count.get(), 0);
        count.observe();
        assert_eq!(count.get(), 1);
        assert_eq!(count.get(), 1);
        count.observe();
        assert_eq!(count.get(), 2);
    }

    #[test]
    fn observations_wrap_at_u32_max() {
        let count = DisconnectCount(AtomicU32::new(u32::MAX));
        count.observe();
        assert_eq!(count.get(), 0);
        count.observe();
        assert_eq!(count.get(), 1);
    }
}
