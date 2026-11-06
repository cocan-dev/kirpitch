# 0029. Any package, drawn from what the datasheet says

Status: accepted

## Context

Version 1 listed seven package shapes (SOIC, SSOP, TSSOP, MSOP, SOT-23, DIP,
QFN). A datasheet for anything else, such as an LQFP microcontroller, ended in
"not supported". But the footprint writer and the 3D model never depended on a
shape: they work from the pads and the body of the PartSpec. Only the step that
turned dimension letters into pads knew the shapes. And every datasheet has the
data: the package outline gives every dimension, and most end with a
recommended footprint with the pads drawn and dimensioned.

## Decision

- **No list of supported footprints.** Three building blocks make the pads, and
  the PartSpec can hold anything:
  - quad gull-wing (QFP, LQFP, TQFP and relatives) with the IPC-7351B gull-wing
    formulas, numbered like KiCad (pin 1 at the top of the left side,
    counter-clockwise);
  - grid arrays (BGA, LGA, CSP, WLCSP) with rows A, B, C ... (JEDEC letters, no
    I, O, Q, S, X, Z) and columns 1, 2, 3 ...; a pad for every position, then
    only the balls the pin table lists are kept;
  - custom pads: runs of pads (first number, count, first position, step, size,
    shape, hole) for anything else, such as a SOT-223 with its tab.
- **The recommended footprint wins over calculation.** A new step reads it:
  pad length and width, the distance of the rows from centres or from outer
  edges, the pitch, the round lands of a grid and the pad under the body, or
  runs of pads for odd layouts. The model reports named values from the drawing
  and Kirpitch computes the coordinates, because arithmetic on a drawing is
  where models go wrong. The values replace the pad sizes and row positions of
  the calculated pattern; numbering stays. The note says how far they are from
  the calculated pads, and warns above 0.25 mm. Without a footprint page, or
  when it cannot be read, the pads are calculated as before and a note says so.
  A package with no template needs the footprint and fails with that reason.
- **The 3D model uses the outline drawing.** The package step also reads A1
  (standoff), A2 (thickness of the body) and c (thickness of a lead), with the
  body, the lead size and the ball diameter it already read. The model uses
  them where they exist, and has balls under the body for grid packages.
  Golden 3D files were regenerated for the three parts that have A1 and c.
- The guided dialog has a Footprint step to point at the footprint.

## Consequences

- Nothing is "unsupported": an unusual package needs its footprint read, and
  the review stages (pinout, footprint, 3D) and the cross-checks stay in
  charge.
- The footprint step costs one more model call when a footprint page exists.
- Text only for now: the dimensions of a drawing are read from the text of the
  PDF inside the box. Reading the drawing as an image for models that can see
  would be a next step.
- The manual flow (`kirpitch template`, New part) still offers only the old
  shapes; the new ones are reached through extraction.
