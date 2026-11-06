# 0025. Parametric 3D model written by Kirpitch itself

Status: accepted

## Context

The requirements ask for a parametric 3D model (STEP and VRML) and say to
evaluate `truck` and `opencascade-rs`, with a CadQuery sidecar as the fallback.
CadQuery needs Python and a large CAD kernel; `opencascade-rs` needs a C++
toolchain. Both would make a plain `cargo install` much harder.

## Decision

- The model is a union of axis-aligned boxes: body, pin 1 mark, and terminals
  (gull-wing leg of three boxes, flat no-lead terminal, through-hole pin,
  exposed pad). It has the right outline, height, terminal positions and pin 1
  corner. It is a model for review and for the board's 3D view, not a copy of
  a maker's exact lead shape; a maker's STEP file can replace it.
- Kirpitch writes both formats itself, without new dependencies: VRML 2.0
  (lengths divided by 2.54 because KiCad reads one VRML unit as 0.1 inch) and
  STEP AP214 with one manifold solid per box and colours. Output is
  deterministic (fixed time stamp).
- Gate 3 ("3d") has no values of its own. Its fingerprint follows the
  footprint, so any footprint change re-opens it. The check `model_fit` blocks
  approval when the height is above the maximum, the body leaves the
  courtyard, an SMD pad has no terminal, or pin 1 is in another corner.
- `gen all` writes `kirpitch.3dshapes/<package>.step` and `.wrl` and links the
  STEP file from the footprint as `${KIPRJMOD}/kirpitch.3dshapes/...`.
- Tests load the files through KiCad: a board with the footprint is exported
  with `kicad-cli pcb export step`, for STEP and VRML links, for each KiCad
  version that is installed.

## Consequences

No Python or C++ toolchain is needed. Curved leads, rounded bodies and exact
lead profiles are not modelled; they can be added later behind the same
`Model` type.
