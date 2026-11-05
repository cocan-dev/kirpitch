# 0010. Rules for byte-identical output

- **Status:** accepted (6 November 2026)

## Context

The same PartSpec must always give byte-identical files, on every operating
system. KiCad files contain UUIDs (universally unique identifiers) and
floating-point numbers, and Windows uses different line endings.

## Decision

- Files use `\n` line endings; `.gitattributes` forces LF in the repository.
- Numbers are written in millimetres, rounded to 6 decimals, without trailing
  zeros and without exponents; `-0` is written as `0`.
- Where KiCad needs a UUID, use UUIDv5 (name-based) derived from the part
  identity and the element path. No timestamps, no random values.
- Collections are written in a defined order, never in hash-map order.

## Consequences

- Snapshot tests can compare bytes on all platforms.
- Changing any of these rules changes the output and is recorded in the
  changelog.
