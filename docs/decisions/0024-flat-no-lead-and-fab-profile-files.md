# 0024. Flat no-lead pads and fab profile files

Status: accepted

## Context

Milestone 2 needs QFN packages, and makers differ in how fine a process they
offer. At 0.4 mm pitch the corner pads of neighbouring sides are about 0.07 mm
apart, even in KiCad's official footprints, so the conservative generic fab
profile rejects them.

## Decision

- `ipc::flat_no_lead` reuses the gull-wing pad calculation with the fillet
  goals for no-lead terminals (toe 0.2/0.3/0.4 mm, heel 0, side -0.04 mm) and
  courtyard excess 0.1/0.25/0.5 mm. It is checked against the official
  QFN-32-1EP_4x4mm_P0.4mm footprint.
- `quad_flat_no_lead` numbers pins as JEDEC and KiCad do (pin 1 top of the left
  side, counter-clockwise) and puts the exposed pad after the last terminal.
- A fab profile can be loaded from a JSON file (`--fab FILE` or
  `KIRPITCH_FAB_PROFILE`). Fields left out keep the generic values, unknown
  fields and non-positive lengths are errors.

## Consequences

Fine-pitch parts need an explicit, visible choice of a finer fab profile
instead of a silent relaxation of the checks.
