# 0022. Approvals store a fingerprint of the stage

- **Status:** accepted (6 November 2026)

## Context

The gate rules say that editing an approved stage re-opens every later stage,
and that approvals are saved in the PartSpec so work can stop and resume. A
PartSpec is a plain JSON file that people may also edit by hand or with other
tools, so the program cannot rely on seeing every edit.

## Decision

- Each approval stores `digest`, a SHA-256 fingerprint of the stage content: the
  pins for the pinout, the package and land pattern for the footprint. The
  footprint fingerprint includes the pinout fingerprint, so a change of the
  pinout also changes it.
- A stage is **approved** when it has an approval whose fingerprint matches its
  current content, **stale** (open again) when it has one that does not match,
  and **open** when it never had one.
- A stage can only be approved when the stage before it is approved, and when it
  has no error finding. Warnings never block; they are listed at approval time
  and can be accepted with an override note in the PartSpec.
- Findings belong to the pinout when their check is about pins (`pin_count`,
  `pin_number`, `pin_name`, `power_pins`) or their path starts with `pins`, and to
  the footprint otherwise.
- Times are stored as RFC 3339 text in UTC. They are data written by the user's
  actions, not part of generated KiCad files, so they do not affect the byte
  identity of those.

## Consequences

- No bookkeeping when a value changes, and hand edits are handled correctly.
- Metadata (name, description, keywords) is not part of a stage, so changing it
  does not re-open anything.
- The 3D and SPICE gates will add their own contents to this scheme.
