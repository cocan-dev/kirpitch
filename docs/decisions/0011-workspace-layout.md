# 0011. Cargo workspace layout

- **Status:** accepted (6 November 2026)

## Context

The requirements list the crates of the workspace. Milestone 1 needs only some
of them, and a shared S-expression writer was missing from the list.

## Decision

- Crates live in `crates/`, one folder per crate, and the workspace member list
  is `crates/*`. A development helper crate `xtask` (not published) is added
  later at the top level.
- Shared package metadata (version, edition 2024, licence, repository) and lints
  are set once in the workspace manifest. `unsafe_code` is forbidden.
- Crates are created when they get code: `kirpitch-core`, `kirpitch-kicad`,
  `kirpitch-gen-symbol`, `kirpitch-gen-footprint` and `kirpitch-cli` in
  milestone 1. Empty crates are not created.
- `publish = false` everywhere until the maintainers decide otherwise.
- Every `.rs` file starts with the SPDX licence line; a test enforces it.

## Consequences

- `cargo test --all` covers the whole workspace.
- Adding a crate means adding a folder, with no change to the member list.
