# 0008. KiCad Library Convention checks start in milestone 2

- **Status:** accepted (6 November 2026)

## Context

KLC (KiCad Library Conventions) checks exist as Python scripts in
`kicad-library-utils`. Wiring them into CI adds a Python dependency.

## Decision

Milestone 1 generators follow the KLC rules directly and document which rules
they cover. Running or porting the `kicad-library-utils` checks is done in
milestone 2.

## Consequences

- No Python in milestone 1 CI.
- A list of the KLC rules covered by the generators is kept next to them.
