# 0032. Fab rules: a JLCPCB preset and rules read from KiCad and Eagle files

Status: accepted

## Context

Footprints have to respect what the PCB maker can build, and every maker's
limits differ. Until now a fab profile had four limits, was a JSON file typed by
hand and was used by one check. The wish was to import the design rules that
makers publish, JLCPCB first, from a settings screen, and to use them while the
footprint is made.

There is no universal format for design rules. The files that exist are KiCad's
custom rules (`.kicad_dru`), Eagle's design rules (`.dru`) and Altium's `.RUL`.
Makers publish their limits as a web page or a PDF (JLCPCB's page is
"PCB Manufacturing & Assembly Capabilities"). JLCPCB has no official file for
KiCad; community files exist, and they hold many rules that apply only to some
objects (inner layers, non-plated holes, castellated holes, slots).

## Decision

- A fab profile has nine limits: copper-to-copper gap, smallest SMD pad,
  smallest drill, annular ring, hole to hole, solder mask web, silkscreen line
  width, silkscreen text height and silkscreen to copper. Values left out keep
  the generic ones.
- A setting names the profile in use: empty (generic), `preset:<name>` (built
  in) or the path of a JSON file. `FabProfile::resolve` reads it. The settings,
  the environment variable and `--fab` all accept all three.
- Built in: `generic` and `jlcpcb`. The JLCPCB numbers are those of the
  capabilities page as read on 7 November 2026, for two-layer 1 oz boards
  (pad to pad 0.15, smallest pad 0.25, drill 0.15, ring 0.18, hole to hole 0.45,
  mask web 0.10, silkscreen 0.15 and 1.0 high, silkscreen to copper 0.15). Other
  makers are reached by import, not by numbers that were not checked.
- Import reads `.kicad_dru` and `.dru` (and the JSON profile). It maps what
  corresponds to a limit, skips rules that are only for inner layers,
  non-plated holes, slots, castellated holes, vias or net classes (and
  clearances for tracks or zones), uses the smallest minimum of the rest, and
  reports what it used, what it skipped and what stayed at the generic value.
  The result is shown as numbers the user can correct before saving; it is never
  saved silently. Altium `.RUL` is not read.
- Saved profiles are JSON files in a `fab` folder next to the settings. The
  settings screen has a **Fab rules** tab to choose, import, copy and edit,
  and delete them.
- The rules are used in two places. A new check, `fab_limits`, tests every pad
  and hole (SMD pad size, drill, annular ring, distance between holes: errors;
  a solder mask web below the maker's smallest: warning, because the maker may
  remove the web). The footprint generator keeps silkscreen lines and text at
  least as large as the profile asks for, and the silkscreen at least its
  distance from copper; with the generic profile the output is unchanged.

## Consequences

- A footprint that the chosen maker cannot build is an error against the pad or
  drill that is the cause, and switching to a finer profile is a visible choice.
- Existing profile files keep working; the new fields take the generic values.
  A profile that lowers `min_copper_gap` alone now gets mask web warnings for
  gaps under 0.10 mm, so it should lower `min_mask_web` too.
- Track width and board-edge clearance are in rules files but are about the
  board, not the footprint, so they are listed as not used.
- The numbers of a maker change; the preset carries its date and source, and
  the user should compare with the maker's page before ordering.
