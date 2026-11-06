# Test data

## `golden/`

The golden test set: one folder per part with

- `datasheet.json`: manufacturer, part, datasheet **URL** and **SHA-256 hash**,
  revision, the pages used and a review status;
- `partspec.json`: the hand-checked expected PartSpec;
- `expected/`: the KiCad files that must be generated from it.

See [ADR 0013](../docs/decisions/0013-golden-test-set.md).

## `pdfs/` (not in git)

Datasheet PDFs are copyrighted and must never be committed. Download them for
re-checking a PartSpec by hand:

```sh
cargo xtask fetch-datasheets     # download missing PDFs, check their hashes
cargo xtask verify-datasheets    # download again and look for changed PDFs
```

Both need `curl`. A changed hash is reported as **drift** (exit code 3) and
does not fail `cargo test`: re-read the pages named in `datasheet.json`, update
`partspec.json` and the hash, then refresh the expected files with
`KIRPITCH_UPDATE_SNAPSHOTS=1 cargo test -p kirpitch-golden`.

## Running the `kicad-cli` tests

Tests that load generated files with KiCad need the path of `kicad-cli` in
`KIRPITCH_KICAD_CLI`; without it they are skipped with a message. On Windows with
KiCad 10 installed in the default place:

```powershell
$env:KIRPITCH_KICAD_CLI = "C:\Program Files\KiCad\10.0\bin\kicad-cli.exe"
cargo test --all
```
