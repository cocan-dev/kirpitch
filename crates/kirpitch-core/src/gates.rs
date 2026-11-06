// SPDX-License-Identifier: GPL-3.0-or-later

//! Review gates: the stages a user approves one after the other.
//!
//! Milestone 2a has two gates, **pinout** and **footprint** (the 3D and SPICE
//! gates come with their generators). Rules from the requirements:
//!
//! - nothing downstream is approved before the stage before it is;
//! - an approval is only given when the stage has no error;
//! - editing an approved stage re-opens it and every later stage.
//!
//! The last rule is implemented without bookkeeping at edit time: an approval
//! stores a fingerprint (digest) of the stage content. A stage whose content no
//! longer matches its fingerprint is *stale*, and a stage after a stale or open
//! stage counts as stale too, because its fingerprint includes the one before.

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::partspec::{Approval, PartSpec};
use crate::validate::{Finding, Level};

/// A review stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gate {
    /// Gate 1: pin numbers, names, types and grouping.
    Pinout,
    /// Gate 2: package dimensions and the land pattern.
    Footprint,
}

/// Where a stage stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    /// Never approved.
    Open,
    /// Approved and unchanged since.
    Approved,
    /// Approved once, but the stage or an earlier stage changed since: it is
    /// open again.
    Stale,
}

/// Reasons an approval is refused.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ApprovalError {
    /// An earlier stage is not approved (or is stale).
    #[error("approve the {} stage first", .0.name())]
    EarlierStageOpen(Gate),
    /// The stage has errors; the findings say which fields to fix.
    #[error("the {} stage has {} error(s)", gate.name(), errors.len())]
    Errors {
        /// The stage.
        gate: Gate,
        /// The blocking findings.
        errors: Vec<Finding>,
    },
}

impl Gate {
    /// All gates in review order.
    pub const ALL: [Gate; 2] = [Gate::Pinout, Gate::Footprint];

    /// The name used on the command line and in messages.
    pub fn name(self) -> &'static str {
        match self {
            Gate::Pinout => "pinout",
            Gate::Footprint => "footprint",
        }
    }

    /// Finds a gate by its name.
    pub fn parse(name: &str) -> Option<Gate> {
        Gate::ALL.into_iter().find(|g| g.name() == name)
    }

    /// The stage before this one.
    pub fn previous(self) -> Option<Gate> {
        match self {
            Gate::Pinout => None,
            Gate::Footprint => Some(Gate::Pinout),
        }
    }

    fn approval(self, spec: &PartSpec) -> Option<&Approval> {
        match self {
            Gate::Pinout => spec.approvals.pinout.as_ref(),
            Gate::Footprint => spec.approvals.footprint.as_ref(),
        }
    }

    fn set_approval(self, spec: &mut PartSpec, approval: Approval) {
        match self {
            Gate::Pinout => spec.approvals.pinout = Some(approval),
            Gate::Footprint => spec.approvals.footprint = Some(approval),
        }
    }

    /// The fingerprint of what this stage contains, including the stage before.
    pub fn digest(self, spec: &PartSpec) -> String {
        let mut hasher = Sha256::new();
        if let Some(prev) = self.previous() {
            hasher.update(prev.digest(spec).as_bytes());
        }
        let content = match self {
            Gate::Pinout => serde_json::to_string(&spec.pins),
            Gate::Footprint => serde_json::to_string(&(&spec.package, &spec.land_pattern)),
        };
        hasher.update(
            content
                .expect("PartSpec serialisation cannot fail")
                .as_bytes(),
        );
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Where the stage stands now.
    pub fn status(self, spec: &PartSpec) -> GateStatus {
        match self.approval(spec) {
            Some(a) if a.approved => {
                if a.digest.as_deref() == Some(self.digest(spec).as_str()) {
                    GateStatus::Approved
                } else {
                    GateStatus::Stale
                }
            }
            _ => GateStatus::Open,
        }
    }

    /// True when a finding belongs to this stage. Checks about pins belong to
    /// the pinout, everything else to the footprint.
    pub fn owns(self, finding: &Finding) -> bool {
        let pinout = matches!(
            finding.check.id(),
            "pin_count" | "pin_number" | "pin_name" | "power_pins"
        ) || finding.path.starts_with("pins");
        match self {
            Gate::Pinout => pinout,
            Gate::Footprint => !pinout,
        }
    }
}

/// The findings of one stage.
pub fn stage_findings(gate: Gate, findings: &[Finding]) -> Vec<&Finding> {
    findings.iter().filter(|f| gate.owns(f)).collect()
}

/// Approves a stage. `at` is the time text to store; `findings` come from
/// [`crate::validate`] for the same spec.
///
/// Errors block the approval; warnings do not (they are listed to the user,
/// who can accept them with an override note in the PartSpec).
pub fn approve(
    spec: &mut PartSpec,
    gate: Gate,
    findings: &[Finding],
    note: Option<String>,
    at: String,
) -> Result<(), ApprovalError> {
    if let Some(prev) = gate.previous()
        && prev.status(spec) != GateStatus::Approved
    {
        return Err(ApprovalError::EarlierStageOpen(prev));
    }
    let errors: Vec<Finding> = stage_findings(gate, findings)
        .into_iter()
        .filter(|f| f.level == Level::Error)
        .cloned()
        .collect();
    if !errors.is_empty() {
        return Err(ApprovalError::Errors { gate, errors });
    }
    let digest = gate.digest(spec);
    gate.set_approval(
        spec,
        Approval {
            approved: true,
            at: Some(at),
            note,
            digest: Some(digest),
        },
    );
    Ok(())
}
