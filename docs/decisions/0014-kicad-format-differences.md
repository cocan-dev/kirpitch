# 0014. How the KiCad file formats differ between versions

- **Status:** accepted (6 November 2026)

## Context

Kirpitch writes one KiCad file per supported KiCad major version (requirements,
"KiCad versions"). The differences were read from the official libraries of KiCad
10.0.5, 9.0.9 and 8.0.9 (the `SOIC-8_3.9x4.9mm_P1.27mm` footprint and the
`LM2904` symbol) and confirmed with `kicad-cli` of each version, using the load
check of ADR 0012.

## Differences used by the writers

| Item | KiCad 10 | KiCad 9 | KiCad 8 |
| --- | --- | --- | --- |
| Footprint format version | 20260206 | 20241229 | 20240108 |
| Symbol format version | 20251024 | 20241209 | 20231120 |
| `generator_version` of symbols | `10.0` | `9.0` | `8.0` |
| Footprint `duplicate_pad_numbers_are_jumpers` | yes | no | no |
| `(embedded_fonts no)` in footprints and symbols | yes | yes | no |
| Symbol `in_pos_files`, `duplicate_pin_numbers_are_jumpers` | yes | no | no |
| Symbol property `show_name`, `do_not_autoplace`, own `hide` | yes | no | no |
| `hide` of a symbol field | in the property | in `effects` | in `effects` |
| Order of pins in a symbol unit | by pin number | left to right, then top to bottom | by pin number |

(Only the columns of versions that have a writer are implemented; the KiCad 8
column is filled in when its writer is added.)

## Not different

- No UUIDs are written in library files of any of the three versions, so the
  UUIDv5 helper of ADR 0010 is not needed yet. It stays reserved for formats that
  need it.
- Pad, line, polygon and text syntax used by the generators is the same, except
  as listed.

## Decision

Every difference is a method of `KicadTarget`; generators never compare version
numbers. A new KiCad major version is added by reading its official libraries
the same way and extending this table. A feature that a target does not have is
dropped, and a warning is added to the report once reports exist (milestone 2).

## Consequences

- Each target has its own snapshot files; the CI job for each KiCad version
  checks the files written for that version.
