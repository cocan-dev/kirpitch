# 0009. Dependabot and minimum supported Rust version

- **Status:** accepted (6 November 2026)

## Context

Dependencies and CI actions need regular updates, and a minimum supported Rust
version (MSRV) must be stated.

## Decision

Use Dependabot (GitHub's dependency updater) for Cargo and GitHub Actions,
weekly. Until 1.0 the MSRV is the latest stable Rust, and `rust-toolchain.toml`
tracks `stable`.

## Consequences

- Updates arrive as small pull requests that must pass the same CI.
- No MSRV test job.
