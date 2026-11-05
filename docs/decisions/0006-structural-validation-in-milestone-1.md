# 0006. Structural validation in milestone 1, full cross-checks in milestone 2

- **Status:** accepted (6 November 2026)

## Context

`kirpitch-core` is described as "schema and validation", but the cross-check
table is listed under milestone 2.

## Decision

Milestone 1 implements the checks that protect the generators from bad input:
pin count, unique pin numbers, pin-to-pad match, tolerance order, units, and
pad clearance against the generic fab profile (ADR 0003). The remaining checks
(power pins, pitch fit, two sources agree, pin 1 consistency, 3D fit, SPICE
nodes, second read) arrive with the stages that need them.

## Consequences

- Validation returns findings with a check id, a level (error or warning) and
  the JSON path of the field.
