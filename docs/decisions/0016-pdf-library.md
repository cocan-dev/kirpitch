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

## Update (6 November 2026): dependency approved and added

The maintainer approved the dependency after this check:

- `pdfium-render` 0.9.5 (released 1 November 2026, 3.7 million downloads, regular
  releases since March): licence MIT OR Apache-2.0. It is built with
  `default-features = false` and the features `thread_safe` and
  `pdfium_latest`, which keeps the dependency tree small; `cargo deny check`
  passes.
- The pdfium library itself is built by the pdfium-binaries project (PDFium is
  licensed under Apache-2.0 and BSD-3-Clause). Kirpitch does not link it: the
  crate loads it at run time from a folder given by the caller.
- Tests and CI get it with `cargo xtask fetch-pdfium`, which downloads a pinned
  release (`chromium/8138`) for the platform and checks the SHA-256 of the
  archive before unpacking. The library goes to the ignored folder
  `testdata/pdfium/`. Nothing of it is committed.
- Tests run on Linux, macOS and Windows in the `PDF` workflow and need
  `KIRPITCH_PDFIUM_LIB_DIR`; without it they are skipped, unless
  `KIRPITCH_REQUIRE_PDFIUM=1`.
- How a released Kirpitch ships pdfium (bundled next to the binary or fetched at
  install) is still open and belongs to the release process.
