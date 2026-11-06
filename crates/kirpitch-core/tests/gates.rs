// SPDX-License-Identifier: GPL-3.0-or-later

//! Review gates: approval order, errors and staleness.

use kirpitch_core::gates::{ApprovalError, Gate, GateStatus, approve, stage_findings};
use kirpitch_core::{FabProfile, Mm, PartSpec, validate};

const FIXTURE: &str = include_str!("fixtures/soic8_example.json");

fn load() -> PartSpec {
    PartSpec::from_json_str(FIXTURE).unwrap()
}

fn approve_gate(spec: &mut PartSpec, gate: Gate) -> Result<(), ApprovalError> {
    let findings = validate(spec, &FabProfile::generic());
    approve(spec, gate, &findings, None, "2026-11-06T12:00:00Z".into())
}

#[test]
fn stages_start_open() {
    let spec = load();
    for gate in Gate::ALL {
        assert_eq!(gate.status(&spec), GateStatus::Open);
    }
}

#[test]
fn the_footprint_cannot_be_approved_before_the_pinout() {
    let mut spec = load();
    assert_eq!(
        approve_gate(&mut spec, Gate::Footprint),
        Err(ApprovalError::EarlierStageOpen(Gate::Pinout))
    );
    approve_gate(&mut spec, Gate::Pinout).unwrap();
    approve_gate(&mut spec, Gate::Footprint).unwrap();
    assert_eq!(Gate::Footprint.status(&spec), GateStatus::Approved);
    let a = spec.approvals.pinout.as_ref().unwrap();
    assert!(a.approved);
    assert_eq!(a.at.as_deref(), Some("2026-11-06T12:00:00Z"));
    assert!(a.digest.is_some());
}

#[test]
fn errors_block_the_approval_and_name_the_fields() {
    let mut spec = load();
    spec.pins[1].number = "1".into(); // duplicate pin number
    match approve_gate(&mut spec, Gate::Pinout) {
        Err(ApprovalError::Errors { gate, errors }) => {
            assert_eq!(gate, Gate::Pinout);
            assert!(errors.iter().any(|e| e.path == "pins[1].number"));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(Gate::Pinout.status(&spec), GateStatus::Open);
}

#[test]
fn a_footprint_error_does_not_block_the_pinout() {
    let mut spec = load();
    spec.land_pattern.pads[0].size_y = Mm::new(1.6).unwrap(); // pads too close
    approve_gate(&mut spec, Gate::Pinout).unwrap();
    assert!(matches!(
        approve_gate(&mut spec, Gate::Footprint),
        Err(ApprovalError::Errors {
            gate: Gate::Footprint,
            ..
        })
    ));
}

#[test]
fn editing_the_pinout_reopens_both_stages() {
    let mut spec = load();
    approve_gate(&mut spec, Gate::Pinout).unwrap();
    approve_gate(&mut spec, Gate::Footprint).unwrap();
    spec.pins[0].name = "OUT_A".into();
    assert_eq!(Gate::Pinout.status(&spec), GateStatus::Stale);
    assert_eq!(Gate::Footprint.status(&spec), GateStatus::Stale);
    // Approving the footprint again needs the pinout first.
    assert_eq!(
        approve_gate(&mut spec, Gate::Footprint),
        Err(ApprovalError::EarlierStageOpen(Gate::Pinout))
    );
    approve_gate(&mut spec, Gate::Pinout).unwrap();
    assert_eq!(Gate::Pinout.status(&spec), GateStatus::Approved);
    // The footprint stays stale until it is approved again.
    assert_eq!(Gate::Footprint.status(&spec), GateStatus::Stale);
    approve_gate(&mut spec, Gate::Footprint).unwrap();
    assert_eq!(Gate::Footprint.status(&spec), GateStatus::Approved);
}

#[test]
fn editing_the_footprint_keeps_the_pinout_approved() {
    let mut spec = load();
    approve_gate(&mut spec, Gate::Pinout).unwrap();
    approve_gate(&mut spec, Gate::Footprint).unwrap();
    spec.land_pattern.pads[0].x = Mm::new(-2.5).unwrap();
    assert_eq!(Gate::Pinout.status(&spec), GateStatus::Approved);
    assert_eq!(Gate::Footprint.status(&spec), GateStatus::Stale);
}

#[test]
fn changes_that_do_not_touch_a_stage_keep_it_approved() {
    let mut spec = load();
    approve_gate(&mut spec, Gate::Pinout).unwrap();
    spec.metadata.description = "another text".into();
    assert_eq!(Gate::Pinout.status(&spec), GateStatus::Approved);
}

#[test]
fn approvals_survive_saving_and_loading() {
    let mut spec = load();
    approve_gate(&mut spec, Gate::Pinout).unwrap();
    let again = PartSpec::from_json_str(&spec.to_json_string()).unwrap();
    assert_eq!(Gate::Pinout.status(&again), GateStatus::Approved);
}

#[test]
fn findings_are_assigned_to_one_stage() {
    let mut spec = load();
    spec.package.pin_count = 9; // a pinout finding
    spec.land_pattern.pads[0].size_y = Mm::new(1.6).unwrap(); // a footprint finding
    let findings = validate(&spec, &FabProfile::generic());
    let pinout = stage_findings(Gate::Pinout, &findings);
    let footprint = stage_findings(Gate::Footprint, &findings);
    assert_eq!(pinout.len() + footprint.len(), findings.len());
    assert!(pinout.iter().any(|f| f.check.id() == "pin_count"));
    assert!(footprint.iter().any(|f| f.check.id() == "pad_clearance"));
}

#[test]
fn gate_names_round_trip() {
    for gate in Gate::ALL {
        assert_eq!(Gate::parse(gate.name()), Some(gate));
    }
    assert_eq!(Gate::parse("3d"), None);
}
