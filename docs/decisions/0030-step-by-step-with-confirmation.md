# 0030. Step by step, with the person confirming each result

Status: accepted

## Context

The guided dialog collected places for every step and then built the part in
one go. When something was wrong (a pin read badly, a missing height) it showed
only at the end, and the cure was to start again. A model sometimes reads
nothing, and then there was no way forward except another try.

## Decision

- The dialog goes one step at a time: part number, pins, package outline,
  recommended footprint. For each step the person points at the place, a model
  reads it, and the result is shown as values that can be changed. When
  nothing could be read, the same values are shown empty to be typed in. The
  step is confirmed or corrected before the next one starts.
- The values travel as JSON in the shape the models answer in. A value a model
  wrote and a value a person typed go through the same parsers, so every rule
  that protects a build protects a typed value too (\`check_step\`).
- Pins keep their trace: each is looked up in the text the person chose (a pin
  not found there is marked and shown as a note), and confirmed pins have full
  confidence because a person has looked at them.
- The footprint step shows the pads that will be made (drawn from the confirmed
  package and, when used, the recommended footprint) while the values are
  edited. The person can choose to calculate the pads instead.
- After the last step the part is assembled from the confirmed values alone, with
  no call to a model (\`assemble_confirmed\`). Before anything is written, the
  validation and the 3D fit checks run (a dry run) and their errors are shown on
  the last screen; the Build button stays off while there are errors, and nothing
  is written then. A file that exists is never replaced.
- Automatic reading stays for people who want it: "skip the guide" builds in one
  go as before.

## Consequences

- A build from confirmed values cannot fail for a reason that was not shown.
- A model that reads nothing is no longer a dead end.
- More clicks than the automatic way, by design: each is a check.
