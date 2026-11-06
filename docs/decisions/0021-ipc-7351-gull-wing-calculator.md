# 0021. IPC-7351 gull-wing calculator: formulas and parameters

- **Status:** accepted (6 November 2026); implements ADR 0005

## Context

The IPC-7351 standard itself is a paid, copyrighted document and is not
available to the project. The formulas for gull-wing leads are widely published,
and the official KiCad footprint generator documents the IPC-7351B parameters it
uses in its (GPL-3.0) source files `ipc_definitions.yaml` and
`ipc_pad_size_calculators.py`.

## Decision

- `kirpitch_core::ipc::gull_wing` implements the three formulas (`Zmax`, `Gmin`,
  `Xmax`) independently in Rust, with the statistical (root of sum of squares)
  tolerance for the inside span derived from the lead length.
- The parameters are facts taken from the KiCad generator's documentation:
  fillet goals for toe, heel and side at the three density levels, separate for
  pitches of 0.625 mm and above and below, rounding to 0.05 mm, fabrication
  tolerance 0.1 mm, placement tolerance 0.05 mm, and the courtyard margin per
  level (0.1, 0.25 and 0.5 mm).
- Pitches of exactly 0.625 mm use the large-pitch table, as in KiCad's
  generator, although the standard text says "greater than".
- Density defaults to nominal (level B). Other levels exist in the API; the CLI
  and the PartSpec do not offer them yet (backlog: IPC-7351 density levels).
- No text, table layout or figure of the standard is copied.

## Consequences

- Tests compare the result with KiCad's official `SOIC-8_3.9x4.9mm_P1.27mm` and
  `TSSOP-14_4.4x5mm_P0.65mm` footprints: pad length, width and position are
  identical.
- Other lead types (J-lead, no-lead, chip) need their own tables and are added
  with the families that use them.
