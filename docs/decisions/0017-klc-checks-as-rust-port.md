# 0017. KLC checks are ported to Rust

- **Status:** accepted (6 November 2026); refines ADR 0008

## Context

ADR 0008 moved the KiCad Library Convention (KLC) checks to milestone 2. They
exist as Python scripts in `kicad-library-utils`. Calling them adds a Python
dependency for CI and for users.

## Decision

Port the rules that matter for the supported package families (SOIC, TSSOP, SOT,
DIP, QFN) to Rust, as checks on the generated files. Before porting, read the
current KLC text and cite rule numbers in the code and in the report. Test the
port against KiCad's official libraries: every official part of those families
must pass, and deliberately broken copies must fail.

## Consequences

- No Python at runtime or in CI.
- The port covers only the listed families at first; the report says which rules
  are checked and which are not.
