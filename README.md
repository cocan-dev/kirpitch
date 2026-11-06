# Kirpitch

Kirpitch turns component and connector datasheets into reviewed KiCad parts:
symbol, footprint, 3D model and, where possible, a SPICE (Simulation Program
with Integrated Circuit Emphasis) simulation model. A language model (or you,
by hand) only reads the datasheet; deterministic Rust code builds every file
from one shared specification, and you approve each stage before the next one
is built. It is written in Rust and runs locally, with any LLM (large language
model) you have, or with none at all.

**Status:** early development. Milestone 1 is done: from a hand-written
PartSpec (Kirpitch's JSON description of a part) the command line tool writes a
symbol and a footprint for SOIC-8 packages that load in KiCad 8, 9 and 10.
Milestone 2a adds a review flow without a language model (package templates for SOIC/TSSOP/SOT-23/DIP, pin table import, gates and approvals in the CLI). PDF reading, extraction with language models, the desktop app and 3D models are not built yet.

## Try it

You need Rust (stable). KiCad is optional.

```sh
cargo run -p kirpitch-cli -- check testdata/golden/lm358-soic8/partspec.json
cargo run -p kirpitch-cli -- gen all testdata/golden/lm358-soic8/partspec.json -o out
```

This writes `out/LM358DR.kicad_sym` and `out/kirpitch.pretty/SOIC-8_3.9x4.9mm_P1.27mm.kicad_mod`.
Use `--target 8`, `9` or `10` to choose the KiCad version; the default is the
installed one. `kirpitch schema` prints the JSON schema of a PartSpec.

## The app

There is a first desktop window (Tauri). Start it with

```sh
cargo run -p kirpitch-app
```

Type the path of a PartSpec file (for example `testdatagoldenlm358-soic8partspec.json`) and press Open. It shows the review stages, the pins and pads with their datasheet pages, the findings and a drawing of the footprint, lets you approve each stage and writes the KiCad files. On Linux it needs the WebKitGTK development packages; on Windows the WebView2 runtime that comes with Windows 11.

## Documentation

- [Requirements](docs/requirements.md): the specification and source of truth
  for scope and design.
- [Contributing](CONTRIBUTING.md): how we work, including checks, commits and
  pull requests.
- [Decisions](docs/decisions): short records of design decisions.
- [Changelog](CHANGELOG.md)

## License

GPL-3.0-or-later (GNU General Public License, version 3 or later). See
[LICENSE](LICENSE). The license covers Kirpitch's code, not the parts you
generate with it.

## Contact

- Questions: hello@cocan.dev
- Security issues: security@cocan.dev (please do not open a public issue)
