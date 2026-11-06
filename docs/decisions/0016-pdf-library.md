# 0016. PDF rendering with pdfium, text-based PDFs first

- **Status:** accepted in principle (6 November 2026); the dependency itself
  still needs its licence and maintenance check in the task that adds it

## Context

Region pick needs page images and text with positions. The requirements name
`pdfium-render` as a candidate. It needs the pdfium shared library on every
platform, which weakens the "single static binary" goal. Pure-Rust PDF readers
exist but render worse.

## Decision

- Use `pdfium-render` with a bundled pdfium library, for text-based PDFs first.
- OCR (`ocrs` or Tesseract) comes later in milestone 2a, after text-based PDFs
  work end to end.
- The PDF code sits behind a small trait in its own module, so the library can be
  replaced without touching the rest.
- Before adding the dependency: check licences of the crate and of the library
  binaries, how binaries are fetched or bundled for Linux, macOS and Windows, and
  that `cargo deny` passes. Ask the maintainer if any answer is unclear.

## Consequences

- The release process must ship or download pdfium per platform.
- Tests use synthetic PDFs created in the tests (ADR 0020).
