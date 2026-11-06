# 0018. The generic fab profile allows 0.12 mm silkscreen

- **Status:** accepted (6 November 2026); amends ADR 0003

## Context

The generators draw silkscreen lines 0.12 mm wide, as KiCad's own libraries do.
The generic fab profile required at least 0.15 mm, so a generated footprint
would fail the profile's own limit.

## Decision

Change the generic profile's minimum silkscreen line width to 0.12 mm and keep
the generators at 0.12 mm. A maker-specific profile (backlog) may set a larger
minimum, and the generator will then widen its lines to that minimum.

## Consequences

- `FabProfile::generic()`, its test and the table in `docs/requirements.md`
  change in a follow-up pull request.
- A check that generated silkscreen respects the profile can be added.
