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

## Update (6 November 2026)

The KLC web site refuses automated access (a Cloudflare check), so the current
text could not be read and the first checks (`kirpitch_kicad::klc`) do **not**
cite rule numbers. Their values come from the KiCad footprint generator
configuration and the official libraries. Rule numbers are added when someone
reads the KLC text; the symbol checks and the remaining footprint rules are
still to do. The official `Package_SO`, `Package_DIP` and `Package_TO_SOT_SMD`
libraries are checked when `KIRPITCH_KICAD_FOOTPRINTS` is set; a few hand-drawn
official footprints deviate, so the test allows 2 percent.
