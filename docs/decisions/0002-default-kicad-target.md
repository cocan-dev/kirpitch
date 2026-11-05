# 0002. Default KiCad target is the installed version

- **Status:** accepted (6 November 2026)

## Context

The requirements said the default target is the installed KiCad version, but
also that "oldest wins for compatibility". The two can disagree.

## Decision

The default target stays the installed KiCad version (from `kicad-cli
version`), or the latest stable version if none is found. "Write the oldest
supported format" is **advice** for people who share files across KiCad
versions, not a rule. It is chosen with `kicad_target` or `--target`.

## Consequences

- `kirpitch gen` without `--target` uses the installed version.
- The requirements wording is changed from a rule to advice.
