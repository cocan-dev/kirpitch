# 0013. Golden test set layout

- **Status:** accepted (6 November 2026)

## Context

The requirements call for a golden set of datasheets, each with a hand-checked
expected PartSpec, and forbid committing datasheet PDFs (copyright). Tests need
to run offline and on every platform.

## Decision

- Each part has a folder `testdata/golden/<part>-<package>/` with:
  - `datasheet.json`: manufacturer, part, title, **URL**, **SHA-256**, size,
    revision, date checked, package, the page numbers used (`pinout`,
    `package_outline`, `land_pattern`, ...), notes and a `partspec_review`
    status.
  - `partspec.json`: the expected PartSpec, in canonical form (the exact output
    of `PartSpec::to_json_string`), with a source page on every pin and pad.
  - `expected/`: the KiCad files that must be generated from it.
- PDFs are downloaded into `testdata/pdfs/`, which is ignored by git.
- Offline tests (`crates/kirpitch-golden`, a test-only crate) check the manifest,
  that the PartSpec agrees with it, that it has no validation findings, and that
  the generated files equal `expected/` byte for byte, are deterministic and take
  under a second. With `KIRPITCH_KICAD_CLI` set they also run the `kicad-cli`
  load check of ADR 0012.
- `KIRPITCH_UPDATE_SNAPSHOTS=1` rewrites `partspec.json` and `expected/` after a
  deliberate change; the diff is reviewed like code.
- A changed datasheet hash is **drift**, reported by a separate tool (`cargo
  xtask verify-datasheets`), and never fails `cargo test`: manufacturers
  re-issue PDFs under the same URL, and the test must not break without a code
  change. A person re-reads the datasheet and updates PartSpec and hash.
- A PartSpec drafted by an AI assistant is marked in `partspec_review` until a
  maintainer has checked it against the datasheet pages.

## Consequences

- The first three parts are SOIC-8: TI LM358 (op-amp), TI NE555 (timer) and
  Microchip 24LC256 (I2C EEPROM). All three use the same land pattern values,
  so they test pin tables and metadata more than geometry. More packages come
  with later milestones.
- Two parts share the footprint name `SOIC-8_3.9x4.9mm_P1.27mm` with slightly
  different pad shapes. How a library handles such cases is a milestone 3
  question (reusing one footprint for parts that share a package).
- `kirpitch-golden` is not in the crate list of the requirements; it only holds
  tests and is never published.
