# 0019. Exposed pads get one paste pad with a warning

- **Status:** accepted (6 November 2026)

## Context

Parts such as QFN and SOIC-8 with exposed pad have a large thermal pad. Printing
one big solder paste area can float the part, so makers split the paste into
smaller windows. The backlog lists "exposed-pad helpers" for this.

## Decision

In milestone 2 an exposed pad is an ordinary pad of the land pattern that gets
paste on one single area, and the checks report a warning that the paste area
of a large exposed pad should be reviewed. Split paste windows and thermal vias
stay in the backlog.

## Consequences

- Pads sharing a number (for example several paste windows entered by hand) are
  already allowed by the clearance check.
- The warning is overridable with a note (as every warning).
