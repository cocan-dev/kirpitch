# 0012. What "loads in kicad-cli" means

- **Status:** accepted (6 November 2026)

## Context

The requirements say every generated part must load in `kicad-cli` without
errors. `kicad-cli` has no validate command. A spike with KiCad 10.0.5 (task 6)
found:

- `kicad-cli fp export svg <folder>.pretty` and `kicad-cli sym export svg
  <file>` parse the whole file strictly. A truncated file or an unknown token
  makes them print `Unable to load library` and exit with code 2.
- A **future format version is accepted** (version 29990101 loaded), so
  `kicad-cli` does not check that our version number is right.
- When the output folder does not exist, `fp export svg` prints
  `Failed to create file` and `Error creating svg file` but **exits with 0**.
  Exit codes alone are not enough.
- `fp upgrade` and `sym upgrade` decide by the file's version number, not by
  its text: a file in the current version is reported as `was not updated`,
  even when its whitespace differs from KiCad's own layout.
- An older version number makes `sym upgrade` rewrite the whole file in KiCad's
  own layout (it adds defaults such as `show_name`, `do_not_autoplace`, the
  empty `Footprint`, `Datasheet` and `Description` properties, and sets the
  generator name). A symbol written exactly like KiCad writes it comes back
  byte for byte identical apart from the generator name.
- `fp upgrade` of an older footprint adds **random UUIDs** to every item, so
  the result cannot be compared with our output.
- KiCad 10 library files contain no UUIDs. The official `SOIC-8` footprint and
  the `LM358` symbol have none. (Older formats may need them; see ADR 0010.)

## Decision

A generated file "loads" when all of these hold:

1. **Export:** `kicad-cli ... export svg` exits with 0, prints none of the words
   `error`, `failed` or `unable`, and produced a non-empty SVG file for the
   part. The output folder is created by Kirpitch before the call.
2. **Own check:** Kirpitch's own S-expression parser reads the file and
   prints it back to exactly the same text (parse and print round trip), and the
   version token equals the version of the chosen target. This catches what
   `kicad-cli` tolerates, such as a wrong version number.
3. **Upgrade check:** `upgrade` on a copy in the current format reports `was not
   updated`. For symbols, an additional check lowers the version number and runs
   `sym upgrade`; the result must equal our file, apart from the generator name.
   There is no such check for footprints because of the random UUIDs.
4. **Negative tests:** the test suite also proves that broken files (truncated,
   unknown token) are rejected by the same functions, so a passing check means
   something.

The wrapper lives in `kirpitch-kicad::cli`. Tests find the tool through the
`KIRPITCH_KICAD_CLI` environment variable and are skipped with a message when it
is missing, unless `KIRPITCH_REQUIRE_KICAD=1` (ADR 0007).

## Consequences

- The symbol writer should emit KiCad's full canonical form, defaults included,
  so that the upgrade check can be used.
- Messages of `kicad-cli` are read in English. If a localised KiCad is used for
  tests, the message checks may need changing.
- The behaviour was observed on KiCad 10.0.5 only; the KiCad 9 and 8 CI jobs
  (task 13) will show whether it holds there.
