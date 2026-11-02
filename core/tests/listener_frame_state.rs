// ABOUTME: Tests the production bounded listener frame-write diagnostic.
// ABOUTME: Supplies observed state values without simulating an ESP scheduler.
#[path = "../../firmware/src/listener_frame_state.rs"]
mod diagnostic;
use diagnostic::*;
#[test]
fn selected_slots_are_never_reused_and_freeze_after_two_retirements() {
    let d = Diagnostic::new();
    let mut a = d.register(100, 11, [1, 2, 3, 4], 80).unwrap();
    let b = d.register(200, 12, [5, 6, 7, 8], 81).unwrap();
    assert!(d.register(300, 13, [0; 4], 82).is_none());
    let mut s = Sampler::new(0);
    a.begin(1000);
    s.opportunity(&d, 101000, |handle| {
        assert_eq!(handle, 11);
        (1, 101000, 101001)
    });
    drop(a);
    assert!(!s.opportunity(&d, 102000, |_| unreachable!()));
    drop(b);
    assert!(s.opportunity(&d, 103000, |_| unreachable!()));
    let r = d.receipt().unwrap();
    assert_eq!(r.selected, 2);
    assert_eq!(r.unselected, 1);
    assert_eq!(r.slots[0].states[1], 1);
    assert_eq!(r.slots[0].aged_states[1], 1);
    assert_eq!(r.slots[0].exemplar.age_us, 100000);
    assert!(d.register(104000, 14, [0; 4], 83).is_none());
    assert_eq!(d.receipt().unwrap(), r);
}
#[test]
fn no_registration_and_one_live_owner_have_distinct_bounds() {
    let d = Diagnostic::new();
    let mut s = Sampler::new(100);
    assert!(!s.opportunity(&d, 30_000_099, |_| unreachable!()));
    assert!(s.opportunity(&d, 30_000_107, |_| unreachable!()));
    let r = d.receipt().unwrap();
    assert_eq!(r.selected, 0);
    assert_eq!(r.late_us, 7);
    let d = Diagnostic::new();
    let _a = d.register(500, 1, [0; 4], 9).unwrap();
    let mut s = Sampler::new(0);
    assert!(!s.opportunity(&d, 10_000_499, |_| unreachable!()));
    assert!(s.opportunity(&d, 10_000_501, |_| unreachable!()));
    let r = d.receipt().unwrap();
    assert_eq!(r.selected, 1);
    assert!(r.slots[0].live);
    assert_eq!(r.late_us, 1);
}
#[test]
fn state_rows_boundary_gaps_and_longest_exemplar_are_observations() {
    let d = Diagnostic::new();
    let mut a = d.register(1, 9, [0; 4], 1).unwrap();
    let mut s = Sampler::new(0);
    a.begin(10);
    s.opportunity(&d, 100009, |_| (2, 100009, 100010));
    s.opportunity(&d, 200010, |_| (0, 200010, 200020));
    s.opportunity(&d, 300010, |_| (99, 300010, 300030));
    a.end();
    s.opportunity(&d, 400010, |_| unreachable!());
    drop(a);
    s.opportunity(&d, 10_000_001, |_| unreachable!());
    let r = d.receipt().unwrap();
    let v = &r.slots[0];
    assert_eq!(v.states, [1, 0, 1, 0, 0, 1]);
    assert_eq!(v.aged_states, [1, 0, 0, 0, 0, 1]);
    assert_eq!(v.exemplar.sequence, 1);
    assert_eq!(v.exemplar.age_us, 300000);
    assert_eq!(v.replaced, 2);
    assert_eq!(v.inactive, 1);
    assert!(r.max_gap_us > 100000);
}

#[test]
fn changing_sequence_during_query_is_unavailable_and_keeps_no_exemplar() {
    let d = Diagnostic::new();
    let mut a = d.register(1, 1, [0; 4], 1).unwrap();
    a.begin(2);
    let mut sampler = Sampler::new(0);
    sampler.opportunity(&d, 3, |_| {
        a.end();
        a.begin(4);
        (1, 3, 5)
    });
    drop(a);
    sampler.opportunity(&d, 10_000_001, |_| unreachable!());
    let r = d.receipt().unwrap();
    assert_eq!(r.slots[0].unavailable, 1);
    assert_eq!(r.slots[0].states, [0; 6]);
    assert_eq!(r.slots[0].exemplar.sequence, 0);
}
#[test]
fn frozen_live_owner_receipt_survives_later_markers_and_retirement() {
    let d = Diagnostic::new();
    let mut a = d.register(1, 1, [0; 4], 1).unwrap();
    a.begin(2);
    let mut sampler = Sampler::new(0);
    sampler.opportunity(&d, 10_000_001, |_| unreachable!());
    let receipt = d.receipt().unwrap();
    a.end();
    a.begin(10_000_002);
    drop(a);
    assert_eq!(d.receipt().unwrap(), receipt);
    assert!(receipt.slots[0].live);
}
#[test]
fn deleted_observation_disables_further_queries() {
    let d = Diagnostic::new();
    let mut a = d.register(1, 1, [0; 4], 1).unwrap();
    a.begin(2);
    let mut sampler = Sampler::new(0);
    sampler.opportunity(&d, 3, |_| (4, 3, 4));
    sampler.opportunity(&d, 5, |_| panic!("invalid slot must not be queried"));
    drop(a);
    sampler.opportunity(&d, 10_000_001, |_| unreachable!());
    assert!(d.receipt().unwrap().slots[0].invalid);
}
#[test]
fn query_cost_excludes_work_before_the_individual_query() {
    let d = Diagnostic::new();
    let mut a = d.register(1, 1, [0; 4], 1).unwrap();
    a.begin(2);
    let mut sampler = Sampler::new(0);
    sampler.opportunity(&d, 3, |_| (1, 103, 110));
    drop(a);
    sampler.opportunity(&d, 10_000_001, |_| unreachable!());
    let r = d.receipt().unwrap();
    assert_eq!(r.slots[0].query_total_us, 7);
    assert_eq!(r.slots[0].exemplar.start_us, 103);
}
#[test]
fn acquisition_freezes_before_microsecond_clock_wrap() {
    let d = Diagnostic::new();
    let mut sampler = Sampler::new(u32::MAX - 20_000_000);
    assert!(sampler.opportunity(&d, u32::MAX - 10_000_000, |_| unreachable!()));
    assert!(d.receipt().unwrap().clock_invalid);
}
#[test]
fn unregistered_is_separate_from_selected_inactive() {
    let d = Diagnostic::new();
    let _a = d.register(1, 1, [0; 4], 1).unwrap();
    let mut sampler = Sampler::new(0);
    sampler.opportunity(&d, 2, |_| unreachable!());
    sampler.opportunity(&d, 10_000_001, |_| unreachable!());
    let r = d.receipt().unwrap();
    assert_eq!(r.slots[0].inactive, 1);
    assert_eq!(r.slots[1].unregistered, 1);
    assert_eq!(r.slots[1].inactive, 0);
}

#[test]
fn late_first_registration_cannot_revive_the_empty_acquisition() {
    let d = Diagnostic::new();
    let mut sampler = Sampler::new(0);
    assert!(!sampler.opportunity(&d, 29_950_000, |_| unreachable!()));
    let _owner = d.register(30_010_000, 1, [1, 2, 3, 4], 80).unwrap();
    assert!(sampler.opportunity(&d, 30_050_000, |_| unreachable!()));
    let receipt = d.receipt().unwrap();
    assert_eq!(receipt.selected, 0);
    assert!(!receipt.slots[0].selected);
    assert!(!receipt.slots[0].live);
    assert_eq!(receipt.slots[0].registered_us, 0);
    assert_eq!(receipt.late_us, 50_000);
    assert!(!receipt.early);
}

#[test]
fn registration_just_before_empty_cutoff_gets_its_full_window() {
    let d = Diagnostic::new();
    let mut sampler = Sampler::new(0);
    let _owner = d.register(29_999_999, 1, [1, 2, 3, 4], 80).unwrap();
    assert!(!sampler.opportunity(&d, 30_050_000, |_| unreachable!()));
    assert!(!sampler.opportunity(&d, 39_999_998, |_| unreachable!()));
    assert!(sampler.opportunity(&d, 39_999_999, |_| unreachable!()));
    let receipt = d.receipt().unwrap();
    assert_eq!(receipt.selected, 1);
    assert_eq!(receipt.late_us, 0);
    assert!(receipt.slots[0].live);
}

#[test]
fn deleted_cannot_become_an_exemplar() {
    let d = Diagnostic::new();
    let mut owner = d.register(1, 1, [0; 4], 1).unwrap();
    owner.begin(2);
    let mut sampler = Sampler::new(0);
    sampler.opportunity(&d, 100_002, |_| (4, 100_002, 100_003));
    sampler.opportunity(&d, 10_000_001, |_| unreachable!());
    let slot = &d.receipt().unwrap().slots[0];
    assert_eq!(slot.states[4], 1);
    assert_eq!(slot.aged_states[4], 1);
    assert!(slot.invalid);
    assert_eq!(slot.unavailable, 1);
    assert_eq!(slot.exemplar.sequence, 0);
}

#[test]
fn deleted_cannot_replace_a_valid_exemplar() {
    let d = Diagnostic::new();
    let mut owner = d.register(1, 1, [0; 4], 1).unwrap();
    owner.begin(2);
    let mut sampler = Sampler::new(0);
    sampler.opportunity(&d, 3, |_| (1, 3, 4));
    sampler.opportunity(&d, 100_002, |_| (4, 100_002, 100_003));
    sampler.opportunity(&d, 10_000_001, |_| unreachable!());
    let slot = &d.receipt().unwrap().slots[0];
    assert_eq!(slot.states[4], 1);
    assert!(slot.invalid);
    assert_eq!(slot.exemplar.state, 1);
    assert_eq!(slot.exemplar.age_us, 1);
    assert_eq!(slot.replaced, 0);
}

#[test]
fn two_retirements_observed_after_deadline_report_expiry() {
    let d = Diagnostic::new();
    let first = d.register(1, 1, [0; 4], 1).unwrap();
    let second = d.register(2, 2, [0; 4], 2).unwrap();
    let mut sampler = Sampler::new(0);
    assert!(!sampler.opportunity(&d, 9_900_000, |_| unreachable!()));
    // The next observation cannot establish whether retirement preceded cutoff.
    drop(first);
    drop(second);
    assert!(sampler.opportunity(&d, 10_000_101, |_| unreachable!()));
    let receipt = d.receipt().unwrap();
    assert!(!receipt.early);
    assert_eq!(receipt.late_us, 100);
    assert_eq!(receipt.selected, 2);
    assert!(receipt.slots.iter().all(|slot| !slot.live));
}

#[test]
fn two_retirements_observed_exactly_at_deadline_report_expiry() {
    let d = Diagnostic::new();
    let first = d.register(1, 1, [0; 4], 1).unwrap();
    let second = d.register(2, 2, [0; 4], 2).unwrap();
    drop(first);
    drop(second);
    let mut sampler = Sampler::new(0);
    assert!(sampler.opportunity(&d, 10_000_001, |_| unreachable!()));
    assert!(!d.receipt().unwrap().early);
    assert_eq!(d.receipt().unwrap().late_us, 0);
}

#[test]
fn two_retirements_observed_before_deadline_report_early_completion() {
    let d = Diagnostic::new();
    let first = d.register(1, 1, [0; 4], 1).unwrap();
    let second = d.register(2, 2, [0; 4], 2).unwrap();
    drop(first);
    drop(second);
    let mut sampler = Sampler::new(0);
    assert!(sampler.opportunity(&d, 10_000_000, |_| unreachable!()));
    assert!(d.receipt().unwrap().early);
    assert_eq!(d.receipt().unwrap().late_us, 0);
}
