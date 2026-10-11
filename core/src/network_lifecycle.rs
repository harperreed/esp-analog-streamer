// ABOUTME: Shares one bounded Wi-Fi/address generation across network consumers.
// ABOUTME: Leases expire on link changes without affecting capture or encoding.

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub generation: u32,
    pub address: Option<Ipv4Addr>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lease {
    pub generation: u32,
    pub address: Ipv4Addr,
}

#[derive(Clone)]
pub struct Network(Arc<Mutex<Snapshot>>);

impl Default for Network {
    fn default() -> Self {
        Self::new()
    }
}

impl Network {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Snapshot {
            generation: 0,
            address: None,
        })))
    }
    pub fn snapshot(&self) -> Snapshot {
        *self.0.lock().unwrap()
    }
    pub fn lease(&self) -> Option<Lease> {
        let snapshot = self.snapshot();
        snapshot.address.map(|address| Lease {
            generation: snapshot.generation,
            address,
        })
    }
    pub fn is_current(&self, lease: Lease) -> bool {
        self.lease() == Some(lease)
    }
    pub fn invalidate(&self) -> Snapshot {
        let mut state = self.0.lock().unwrap();
        if let Some(generation) = state.generation.checked_add(1) {
            state.generation = generation;
        }
        state.address = None;
        *state
    }
    pub fn observe(&self, expected: Snapshot, address: Option<Ipv4Addr>) -> Snapshot {
        let mut state = self.0.lock().unwrap();
        if *state != expected || state.generation == u32::MAX {
            return *state;
        }
        let address = address.filter(|ip| {
            !ip.is_unspecified() && !ip.is_loopback() && !ip.is_broadcast() && !ip.is_multicast()
        });
        if state.address != address {
            match state.generation.checked_add(1) {
                Some(generation) if generation < u32::MAX => {
                    state.generation = generation;
                    state.address = address;
                }
                _ => {
                    state.generation = u32::MAX;
                    state.address = None;
                }
            }
        }
        *state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const IP: Ipv4Addr = Ipv4Addr::new(192, 168, 200, 10);

    #[test]
    fn ready_address_yields_a_current_lease_shared_by_clones() {
        let network = Network::new();
        assert_eq!(network.lease(), None);
        let before = network.snapshot();
        let ready = network.observe(before, Some(IP));
        assert_ne!(ready.generation, before.generation);
        let lease = network.lease().unwrap();
        assert_eq!(lease.address, IP);
        assert!(network.clone().is_current(lease));
    }

    #[test]
    fn disconnect_rejects_pending_snapshot_even_when_already_offline() {
        let network = Network::new();
        let pending = network.snapshot();
        let down = network.invalidate();
        assert_ne!(pending, down);
        assert_eq!(network.observe(pending, Some(IP)), down);
        assert_eq!(network.lease(), None);
    }

    #[test]
    fn unchanged_address_does_not_advance_but_same_ip_reconnect_does() {
        let network = Network::new();
        let ready = network.observe(network.snapshot(), Some(IP));
        assert_eq!(network.observe(ready, Some(IP)), ready);
        let lease = network.lease().unwrap();
        let down = network.invalidate();
        assert!(!network.is_current(lease));
        network.observe(down, Some(IP));
        assert!(!network.is_current(lease));
        assert_eq!(network.lease().unwrap().address, IP);
    }

    #[test]
    fn address_change_and_loss_revoke_old_leases() {
        let network = Network::new();
        network.observe(network.snapshot(), Some(IP));
        let old = network.lease().unwrap();
        network.observe(network.snapshot(), Some(Ipv4Addr::new(192, 168, 200, 11)));
        assert!(!network.is_current(old));
        let replaced = network.lease().unwrap();
        network.observe(network.snapshot(), None);
        assert!(!network.is_current(replaced));
        assert_eq!(network.lease(), None);
    }

    #[test]
    fn invalid_lan_addresses_never_become_leases() {
        for ip in [
            Ipv4Addr::UNSPECIFIED,
            Ipv4Addr::LOCALHOST,
            Ipv4Addr::BROADCAST,
            Ipv4Addr::new(224, 0, 0, 1),
        ] {
            let network = Network::new();
            network.observe(network.snapshot(), Some(IP));
            assert!(network.lease().is_some());
            assert_eq!(network.observe(network.snapshot(), Some(ip)).address, None);
            assert_eq!(network.lease(), None);
        }
    }

    #[test]
    fn exhaustion_is_permanently_unavailable_without_wrap() {
        let network = Network(Arc::new(Mutex::new(Snapshot {
            generation: u32::MAX - 1,
            address: Some(IP),
        })));
        let old = network.lease().unwrap();
        let exhausted = network.invalidate();
        assert_eq!(
            exhausted,
            Snapshot {
                generation: u32::MAX,
                address: None
            }
        );
        assert!(!network.is_current(old));
        assert_eq!(network.observe(exhausted, Some(IP)), exhausted);
        assert_eq!(network.invalidate(), exhausted);
    }
}
