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
- Project documentation: requirements, contributing guidelines and
  architecture decision records.
