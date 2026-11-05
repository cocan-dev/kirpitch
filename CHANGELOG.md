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
- Project documentation: requirements, contributing guidelines and
  architecture decision records.
