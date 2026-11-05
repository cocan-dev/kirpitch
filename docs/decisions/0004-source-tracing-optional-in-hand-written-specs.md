# 0004. Source tracing is optional in hand-written PartSpecs

- **Status:** accepted (6 November 2026)

## Context

Every extracted value should record `{page, bbox, confidence}`. Requiring that
in hand-written files makes them long and error-prone while no extractor
exists.

## Decision

In milestone 1 the `source` of a value is optional. From milestone 2 on, values
produced by extraction must carry a source, and validation enforces it for
them. Values typed by hand may still omit it and are shown as "no source".

## Consequences

- The schema models values as `Sourced<T>` with an optional `source`.
- No schema break is needed when extraction arrives.
