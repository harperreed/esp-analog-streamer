// ABOUTME: Public HTTP controller contract rejects invalid identity before network access.
// ABOUTME: Protocol and real loopback framing cases live with the private transport helpers.
use esp_shazam_core::{sonos::DiscoveryGroup, sonos_upnp::Client};
use std::{net::Ipv4Addr, time::Instant};
#[test]
fn expired_connect_has_no_network_side_effect() {
    let group = DiscoveryGroup {
        player_id: "RINCON_TEST".into(),
        group_id: "group".into(),
        household_id: "house".into(),
        name: "test".into(),
        host: Ipv4Addr::LOCALHOST,
        is_leader: true,
    };
    assert!(Client::connect(&group, Instant::now()).is_err());
}
