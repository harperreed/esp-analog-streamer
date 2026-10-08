// ABOUTME: Stores diagnostic events in a fixed array before resources are released.
// ABOUTME: Preserves event order and reports overflow without allocation or logging.

pub(crate) struct Events<T: Copy, const N: usize> {
    values: [Option<T>; N],
    len: usize,
    overflowed: bool,
}

impl<T: Copy, const N: usize> Events<T, N> {
    pub(crate) fn new() -> Self {
        Self {
            values: [None; N],
            len: 0,
            overflowed: false,
        }
    }

    pub(crate) fn push(&mut self, event: T) {
        if self.len == N {
            self.overflowed = true;
            return;
        }
        self.values[self.len] = Some(event);
        self.len += 1;
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.values[..self.len].iter().flatten()
    }

    pub(crate) fn overflowed(&self) -> bool {
        self.overflowed
    }
}

#[cfg(test)]
mod tests {
    use super::Events;

    #[test]
    fn events_retain_acquisition_order_until_deferred_reporting() {
        let mut events = Events::<u64, 3>::new();
        events.push(40_000);
        events.push(40_200);
        events.push(40_201);
        assert_eq!(
            events.iter().copied().collect::<Vec<_>>(),
            [40_000, 40_200, 40_201]
        );
        assert!(!events.overflowed());
    }

    #[test]
    fn overflow_preserves_prior_events_and_sets_failure_flag() {
        let mut events = Events::<u64, 2>::new();
        events.push(1);
        events.push(2);
        events.push(3);
        events.push(4);
        assert_eq!(events.iter().copied().collect::<Vec<_>>(), [1, 2]);
        assert!(events.overflowed());
    }

    #[test]
    fn zero_capacity_records_overflow_without_indexing() {
        let mut events = Events::<u8, 0>::new();
        events.push(1);
        assert_eq!(events.iter().count(), 0);
        assert!(events.overflowed());
    }
}
