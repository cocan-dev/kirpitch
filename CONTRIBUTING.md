# Contributing to Kirpitch

Thanks for your interest in Kirpitch! This guide explains how we work, for
human contributors and AI-assisted ones alike.

Kirpitch turns component and connector datasheets into reviewed KiCad parts.
The full specification is in [docs/requirements.md](docs/requirements.md).
Please read it before proposing larger changes; it is the source of truth for
scope and design decisions.

## Before you start

- **Bugs and ideas:** open a [GitHub issue](https://github.com/cocan-dev/kirpitch/issues).
  For larger changes, open an issue first so we can agree on the approach
  before you write code.
- **Security issues:** do not open a public issue. Email
  **security@cocan.dev** instead.
- **Questions:** hello@cocan.dev.

## Development setup

- Rust, stable toolchain (install with [rustup](https://rustup.rs)).
- KiCad 8, 9 or 10, for tests that load generated files with `kicad-cli`
  (optional). Set `KIRPITCH_KICAD_CLI` to the path of `kicad-cli`; without it
  these tests are skipped with a message. On Linux, `scripts/kicad-cli-docker.sh`
  runs `kicad-cli` of any version from the official container image. See
  [testdata/README.md](testdata/README.md).
- Git, with your commit name and email configured.

## Checks before every commit

Run these and fix everything they report. Code that fails them is not merged.

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

If a test cannot pass yet (for example, for unfinished work), mark it
`#[ignore]` with a reason and say so in the commit body.

## Commit messages

We use **Conventional Commits**, a standard message format that tools can read
to build changelogs.

```
<type>(<optional scope>): <short summary in the imperative>

<body: what changed and why, wrapped at 72 characters>

<footer: issue references, breaking changes, co-author trailers>
```

- **Types:** `feat` (new feature), `fix` (bug fix), `docs`, `test`, `refactor`
  (no behaviour change), `perf`, `build` (Cargo, dependencies), `ci`, `chore`.
- **Scope:** the crate or area, e.g. `feat(core): add PartSpec pin schema` or
  `fix(gen-footprint): correct pad rotation for SOT-23`.
- **Summary:** imperative mood ("add", not "added"), at most 72 characters, no
  trailing period.
- **Body:** explain *why*, not just *what*. Mention trade-offs and anything a
  reviewer should check.
- **Breaking changes:** add `!` after the type (`feat(core)!: ...`) and a
  `BREAKING CHANGE:` line in the footer.
- **Issues:** reference them with `Refs #12` or `Closes #12`.

## Commit size and frequency

- One logical change per commit. Each commit should be easy to review, revert
  or cherry-pick on its own. Don't mix a refactor, a feature and formatting
  changes in one commit.
- Commit whenever a coherent step is complete and the checks pass. Don't batch
  unrelated work into one commit, and don't commit broken states "to save
  progress".
- Move or rename files in their own commit, separate from content changes.

## Branches and pull requests

- `main` must always build and pass all checks.
- Work on short-lived branches named `<type>/<short-description>`, e.g.
  `feat/partspec-schema`, `fix/qfn-exposed-pad`, `docs/requirements-cleanup`.
- Keep pull requests small and focused on one topic. Use a Conventional Commits
  style title and describe what changed, why, and how you tested it.
- Pull requests are merged with **squash and merge** unless the individual
  commits are worth keeping.
- Never force-push to `main` or rewrite history that others may have pulled.

## Never commit

- **Secrets:** API keys, tokens, `.env` files, or `kirpitch.toml` files that
  contain real keys. Keys belong in environment variables or the OS keychain.
- **Datasheet PDFs.** Manufacturer datasheets are copyrighted and must not be
  redistributed in this repository. For test data, commit the datasheet URL,
  its SHA-256 hash and the expected PartSpec, plus a script that downloads the
  PDFs into an ignored folder such as `testdata/pdfs/`.
- Vendor 3D models or footprints whose licence does not allow redistribution.
- Build output (`target/`), editor files, large binaries and generated test
  output.

## Code guidelines

- Every source file starts with `// SPDX-License-Identifier: GPL-3.0-or-later`.
- New dependencies must be GPL-3.0-compatible and actively maintained. Explain
  why you added a dependency in the commit body. Licence and security checks run
  with `cargo deny check`.
- LLMs (large language models) only extract data. All geometry, IPC-7351 land
  pattern calculations and file writing are deterministic Rust code with tests.
- Units are millimetres everywhere internally.
- Public functions and types get doc comments. Spell out abbreviations the
  first time they appear in docs and user-facing text.

## Documentation and decisions

- When a design decision changes, update `docs/requirements.md` in the same
  pull request.
- Record significant decisions as short ADRs (architecture decision records) in
  `docs/decisions/NNNN-title.md`, covering context, decision and consequences.
- Keep `CHANGELOG.md` up to date in the [Keep a Changelog](https://keepachangelog.com)
  format, with an `Unreleased` section at the top.

## AI-assisted contributions

AI-assisted contributions are welcome. You are responsible for every line you
submit: review it, understand it and test it as if you had written it yourself.
Mark substantially AI-generated commits with a `Co-Authored-By:` trailer, or
mention it in the pull request description.

## Releases

Releases follow SemVer (semantic versioning: MAJOR.MINOR.PATCH; while below
1.0, a MINOR release may contain breaking changes). Publishing to crates.io,
tagging and GitHub releases are done by the maintainers.

## Licence of contributions

Kirpitch is licensed under GPL-3.0-or-later. By submitting a contribution, you
agree that it is licensed under the same terms.
