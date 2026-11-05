# 0007. kicad-cli tests run on Linux CI only

- **Status:** accepted (6 November 2026)

## Context

The requirements call for CI (continuous integration) containers with each
supported KiCad version. Containers only work on Linux runners, while Kirpitch
supports Linux, macOS and Windows.

## Decision

All three operating systems run format, lint and unit/snapshot tests. Tests
that need `kicad-cli` run in a Linux job, once per supported KiCad version.
They read the path from `KIRPITCH_KICAD_CLI` and are skipped with a clear
message when it is not set, unless `KIRPITCH_REQUIRE_KICAD=1` (set in that
job), in which case a missing tool is a failure.

## Consequences

- Generated files are still compared byte for byte on all three systems
  through snapshots.
- Local runs on Windows or macOS can opt in by setting the variable.
