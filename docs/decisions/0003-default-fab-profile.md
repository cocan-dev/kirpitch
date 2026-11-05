# 0003. A built-in generic fab profile in milestone 1

- **Status:** accepted (6 November 2026)

## Context

The "pad clearance" cross-check compares pad-to-pad gaps with the minimum of
the user's PCB (printed circuit board) maker, "set in a fab profile". Fab
profiles were in the Backlog, so the check had no input in milestone 1.

## Decision

Kirpitch ships one built-in **generic** fab profile with conservative values
(listed in the requirements). Maker-specific presets (JLCPCB, PCBWay, Aisler)
stay in the Backlog. The values are chosen to be safe for typical low-cost
services, not to match any one maker.

## Consequences

- The pad clearance check can run in milestone 1.
- The values live in one documented place in `kirpitch-core` and are covered by
  tests.
