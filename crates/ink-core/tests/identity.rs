//! T001 / NFR-004: the domain crate must be testable with no display or GPU.
//! These tests cover only the identity and logical-unit vocabulary created in
//! T001. The document model, commands, and history belong to T009/T010.

use ink_core::{IdSource, LogicalPoint, LogicalSize, ObjectId, OutputId};

#[test]
fn object_ids_are_injected_and_deterministic() {
    let mut ids = IdSource::starting_at(1);
    let first: ObjectId = ids.next_id();
    let second = ids.next_id();

    assert_ne!(first, second);
    assert_eq!(
        vec![first, second],
        IdSource::starting_at(1).take(2),
        "a fresh source with the same seed must produce the same ids"
    );
}

#[test]
fn output_ids_compare_by_value_not_by_monitor_index() {
    let a = OutputId::new("DP-1");
    let b = OutputId::new("DP-1");
    let c = OutputId::new("HDMI-A-1");

    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(a.as_str(), "DP-1");
}

#[test]
fn logical_size_rejects_nonfinite_and_negative_values() {
    assert!(LogicalSize::new(1920.0, 1080.0).is_some());
    assert!(LogicalSize::new(f64::NAN, 1080.0).is_none());
    assert!(LogicalSize::new(f64::INFINITY, 1080.0).is_none());
    assert!(LogicalSize::new(-1.0, 1080.0).is_none());
    assert!(LogicalSize::new(0.0, 1080.0).is_none());
}

#[test]
fn logical_points_are_output_local_and_finite() {
    assert!(LogicalPoint::new(0.0, 0.0).is_some());
    assert!(
        LogicalPoint::new(-12.5, 4.0).is_some(),
        "negative is in-range; clamping is a backend policy"
    );
    assert!(LogicalPoint::new(f64::NAN, 0.0).is_none());
}
