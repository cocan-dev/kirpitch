# 0020. Golden set growth, review and test PDFs

- **Status:** accepted (6 November 2026); extends ADR 0013

## Context

The golden set has three SOIC-8 parts with identical land pattern values, drafted
by an AI assistant. Milestone 2 needs more families, and PDF parsing needs PDFs.

## Decision

- Grow the golden set to about 12 parts in milestone 2a: SOIC, TSSOP, SOT-23,
  DIP and QFN, from major manufacturers, using official datasheet URLs that were
  checked by downloading, with their SHA-256 hashes. The assistant proposes the
  list; the maintainer may change it.
- Every PartSpec has a `partspec_review` status. A part is only marked reviewed
  by a human who has compared it with the datasheet pages. Until the maintainer
  has reviewed the first three, the assistant makes a second independent pass
  and lists its doubts in the pull request.
- Tests for PDF reading use synthetic PDFs created inside the tests. No
  manufacturer PDF is ever used as a test input in the repository.

## Consequences

- Golden checks stay offline; datasheet drift is reported by `cargo xtask`.
