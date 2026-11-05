# Kirpitch

Kirpitch turns component and connector datasheets into reviewed KiCad parts:
symbol, footprint, 3D model and, where possible, a SPICE (Simulation Program
with Integrated Circuit Emphasis) simulation model. A language model (or you,
by hand) only reads the datasheet; deterministic Rust code builds every file
from one shared specification, and you approve each stage before the next one
is built. It is written in Rust and runs locally, with any LLM (large language
model) you have, or with none at all.

**Status:** early development. Nothing is usable yet.

## Documentation

- [Requirements](docs/requirements.md): the specification and source of truth
  for scope and design.
- [Contributing](CONTRIBUTING.md): how we work, including checks, commits and
  pull requests.

## License

GPL-3.0-or-later (GNU General Public License, version 3 or later). See
[LICENSE](LICENSE). The license covers Kirpitch's code, not the parts you
generate with it.

## Contact

- Questions: hello@cocan.dev
- Security issues: security@cocan.dev (please do not open a public issue)
