# 0001. Milestone 1 covers symbol and footprint only

- **Status:** accepted (6 November 2026)

## Context

The Scope section listed milestone 1 as "schema and all generators", but
SPICE (Simulation Program with Integrated Circuit Emphasis) models are planned
for milestone 5, and the 3D generator needs a prior evaluation of `truck`,
`opencascade-rs` and a CadQuery sidecar.

## Decision

Milestone 1 delivers the PartSpec schema plus the **symbol and footprint**
generators, driven by hand-written PartSpecs. The 3D generator becomes its own
step (milestone 1b) after the STEP-engine evaluation. SPICE stays in
milestone 5.

## Consequences

- `kirpitch-gen-3d` and `kirpitch-gen-spice` are not created in milestone 1.
- The PartSpec reserves, but does not define, `body_3d` and `electrical`.
- `docs/requirements.md` (Scope and milestones) is updated accordingly.
