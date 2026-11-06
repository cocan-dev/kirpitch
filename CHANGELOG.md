# Changelog

All notable changes to Kirpitch are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Cargo workspace with the `kirpitch-core` crate (stub).
- GitHub Actions CI (format, clippy and tests on Linux, macOS and Windows) and
  `cargo deny` licence and security checks.
- `kirpitch-core` primitives: millimetre lengths, tolerances, unit conversion,
  source tracing types and pin electrical types.
- PartSpec schema (version 1) with JSON loading that names the bad field, a
  built-in generic fab profile and structural validation (pin count, pin
  numbers and names, pin-to-pad match, tolerance order, units, pad clearance,
  positive sizes, source sanity).
- JSON schema of the PartSpec, generated from the Rust types and committed as
  `schema/partspec.schema.json` (a test fails when it is stale).
- `kirpitch-kicad` crate with a `kicad-cli` wrapper (version, SVG export,
  upgrade) and test helpers; tests use `KIRPITCH_KICAD_CLI` and skip without it.
- S-expression writer and parser in `kirpitch-kicad` that reproduces KiCad 10's
  own layout, and the `KicadTarget` type (KiCad 10 so far).
- `kirpitch-gen-footprint`: KiCad 10 footprint generator (SMD pads, courtyard,
  fab outline with pin 1 chamfer, silkscreen, pin 1 marker) with snapshot and
  `kicad-cli` tests.
- `kirpitch-gen-symbol`: KiCad 10 symbol generator (pins placed by type on the
  2.54 mm grid, stacked power pins, MPN and manufacturer fields) whose output
  equals KiCad's own canonical layout.
- Golden test set: TI LM358, TI NE555 and Microchip 24LC256 in SOIC-8 with
  datasheet URL and SHA-256 (no PDFs), hand-written PartSpecs and expected KiCad
  files; symbols place bidirectional pins on the right.
- `cargo xtask fetch-datasheets` and `verify-datasheets`: download the golden
  datasheets into the ignored `testdata/pdfs/` and report changed hashes (drift).
- `kirpitch` command-line tool with `schema`, `check` and `gen symbol|footprint|all`
  (KiCad target from `--target`, the installed KiCad, or the newest supported).
- CI job that runs the `kicad-cli` tests in the official KiCad 10 container, with
  `scripts/kicad-cli-docker.sh` as the wrapper.
- KiCad 9 writers for symbols and footprints (`--target 9`), with expected files
  per KiCad version and a KiCad 9 job in the `kicad-cli` workflow.
- KiCad 8 writers for symbols and footprints (`--target 8`) and a KiCad 8 job in the
  `kicad-cli` workflow.
- Milestone 2 decisions (ADRs 0015 to 0020): split in 2a and 2b, pdfium for PDFs,
  KLC checks in Rust, silkscreen width in the generic fab profile, exposed pads,
  golden set growth.
- The generic fab profile allows 0.12 mm silkscreen lines, as the generators draw them (ADR 0018).
- PartSpec `origin` (hand, region, llm): values read from a datasheet must carry a
  source with page and region, and a confidence when a model read them
  (`source_required` check, ADR 0004).
- More checks: `power_pins`, `pitch_fit`, `exposed_pad` (ADR 0019) and `override_unknown`.
- IPC-7351B land pattern calculator for gull-wing leads (`kirpitch_core::ipc`),
  checked against KiCad's official SOIC and TSSOP footprints (ADR 0021).
- `two_sources` check: the datasheet land pattern is compared with the IPC-7351
  result when the package dimensions E, L and b are given.
- Package template for dual-row gull-wing packages (SOIC, SOP, SSOP, TSSOP, MSOP)
  in `kirpitch_core::templates`: JEDEC letters in, package and IPC-7351 land
  pattern out, matching KiCad's official SOIC-8, SOIC-16 and TSSOP-14 pads.
- SOT-23 template (3, 5 and 6 leads) matching KiCad's pin positions and SOT-23-5 pads;
  footprints made from templates load in KiCad 8, 9 and 10.
- Through-hole pads (`kind: thru_hole`, `drill`), body centre, the `drill` check, the DIP
  template and through-hole footprints (silkscreen between the rows, pin 1 notch).
- Project documentation: requirements, contributing guidelines and
  architecture decision records.
