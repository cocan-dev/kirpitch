# 0005. The IPC-7351 calculator moves to milestone 2

- **Status:** accepted (6 November 2026)

## Context

Footprints use the datasheet's recommended land pattern, or else a pattern
calculated with IPC-7351 (the IPC standard for land patterns). The calculator is
only needed when no land pattern is given, and for the "two sources agree"
cross-check.

## Decision

Milestone 1 footprints use the explicit land pattern in the hand-written
PartSpec. The IPC-7351 calculator (nominal density first) is built in
milestone 2, together with extraction and that cross-check.

## Consequences

- Milestone 1 PartSpecs must contain a land pattern.
- No IPC-7351 code or tables in milestone 1.
