# 0015. Milestone 2 is split in 2a and 2b

- **Status:** accepted (6 November 2026)

## Context

Milestone 2 in the requirements holds no-LLM mode, LLM extraction, source
tracing, cross-checks, the IPC-7351 calculator, KLC checks and `kirpitch bench`.
That is too much for one milestone with one definition of done.

## Decision

- **2a (no model needed):** IPC-7351 calculator, full cross-checks, package
  templates and more package families, pin table import, PDF rendering and
  region pick, the CLI review flow with approvals, KLC checks and a larger golden
  set. Done when a person can go from a PDF to approved KiCad files without any
  language model.
- **2b (models):** the `kirpitch-extract` crate, LLM trait and adapters,
  structured extraction, capability probe, privacy switch, cache per PDF hash,
  second read and `kirpitch bench`.
- 2a comes first; the golden set it grows is the benchmark for 2b.
- The 3D generator (milestone 1b, ADR 0001) is done after 2a.
- Provider order for 2b: an OpenAI-compatible adapter first (it covers Ollama,
  LM Studio, vLLM and OpenRouter), then Anthropic, then the other providers.
- Adapter tests use recorded responses, so CI needs no API keys.

## Consequences

- `docs/requirements.md` (Scope and milestones) lists 2a and 2b.
- New dependencies for PDF, HTTP, keychain, CSV and TOML are proposed one by one
  in the task that needs them, after checking licence and maintenance.
