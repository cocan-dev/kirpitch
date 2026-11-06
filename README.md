# Kirpitch

Kirpitch turns component and connector datasheets into reviewed KiCad parts:
symbol, footprint, 3D model and, where possible, a SPICE (Simulation Program
with Integrated Circuit Emphasis) simulation model. A language model (or you,
by hand) only reads the datasheet; deterministic Rust code builds every file
from one shared specification, and you approve each stage before the next one
is built. It is written in Rust and runs locally, with any LLM (large language
model) you have, or with none at all.

**Status:** version 1 (the MVP) is built and tested; there is no release yet.
From a datasheet or by hand, Kirpitch makes a symbol, a footprint and a simple
3D model for SOIC/TSSOP-style, SOT-23, DIP and QFN packages, for KiCad 8, 9 and
10, with a review stage for each. What is and is not in version 1 is listed in
the [requirements](docs/requirements.md#scope-and-milestones); the measured
model results are in [docs/model-recommendations.md](docs/model-recommendations.md).

## Try it

You need Rust (stable). KiCad is optional (it is used to check the files).

**From a PartSpec** (the JSON description of a part):

```sh
cargo run -p kirpitch-cli -- check testdata/golden/lm358-soic8/partspec.json
cargo run -p kirpitch-cli -- gen all testdata/golden/lm358-soic8/partspec.json -o out
```

This writes `out/LM358DR.kicad_sym`, `out/kirpitch.pretty/...kicad_mod` and
`out/kirpitch.3dshapes/...step` and `...wrl` (the footprint links the STEP file
as `${KIPRJMOD}/kirpitch.3dshapes/...`). Use `--target 8`, `9` or `10` to choose the
KiCad version; the default is the installed one.

**By hand, with the review stages:**

```sh
kirpitch template spec.json --mpn LM358 --manufacturer TI --description "Dual op amp" dual-row ...
kirpitch pins spec.json --import pins.csv        # or --pdf datasheet.pdf --page 3 --region l,t,r,b
kirpitch review spec.json --stage pinout         # then footprint, then 3d
kirpitch approve spec.json --stage pinout
```

**With a language model** (copy [kirpitch.example.toml](kirpitch.example.toml) to
`kirpitch.toml` first; keys are only read from environment variables):

```sh
kirpitch models test
kirpitch build datasheet.pdf -o spec.json --hint "SOIC-8, drawing D0008A" --second-read
kirpitch bench          # needs the golden datasheets: cargo xtask fetch-datasheets
```

`build` needs a text-based PDF and `cargo xtask fetch-pdfium` once (set
`KIRPITCH_PDFIUM_LIB_DIR` to the folder it prints). With `local_only = true` no
data leaves your machine or network.

## The app

```sh
cargo run -p kirpitch-app
```

The start screen lists the example parts (opened as working copies) and your recent files, and has **New part…** (a package form with presets), **Open file…**, and **From datasheet…** (a language model reads the PDF; set up models, keys and folders under the gear icon, **Settings**). You can also drop a `.json` or a `.pdf` on the window. A part opens as four steps: **Pinout**, **Footprint**, **3D model** and **Export**. Each step shows what to check, the findings of that step (click one to jump to the pin), and a bar at the bottom with **Approve** and **Next**. The datasheet page sits beside the first two steps; selecting a pin shows it in the table, the symbol, the footprint and on the page. **Home** (the logo, the breadcrumb or Esc) goes back to the start screen. You can edit pins, drag a box on the datasheet page to read a pin table, save a copy and write the KiCad files. Shortcuts: Ctrl+O open, Ctrl+N new part, Alt+Left and Alt+Right to move between steps.
On Linux it needs the WebKitGTK development packages; on Windows the WebView2
runtime that comes with Windows 11.

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
