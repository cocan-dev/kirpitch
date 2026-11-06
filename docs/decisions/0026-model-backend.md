# 0026. Model backend without a unified client crate

Status: accepted

## Context

The requirements ask to evaluate the unified-client crates `siumai`, `llm` and
`multi-llm` and, if none fits, to write a thin trait with our own adapters.
Those crates bring large dependency trees and their own TLS and async stacks.
Kirpitch is a synchronous tool that already depends on the system `curl` for
downloads (ADR 0013).

## Decision

- `kirpitch-models` has a small `Client` with adapters for the Anthropic
  Messages API, the OpenAI chat API (also used for every OpenAI-compatible
  server, which covers LM Studio, llama.cpp, vLLM, LocalAI, OpenRouter, Mistral,
  Gemini's compatible endpoint and Azure OpenAI), and the native Ollama API.
  Request builders and answer readers are plain functions tested with recorded
  answers. Bedrock and Vertex AI, which need cloud-specific signing, are not
  supported in version 1.
- Requests go out through the system `curl` (`Transport` trait). The address
  and headers, which hold the key, are passed on curl's standard input, never on
  the command line. Tests use a mock transport, so no test needs a network.
- Configuration is `kirpitch.toml`. A key in the file is an error; a model names
  an environment variable (`api_key_env`). The operating system keychain is not
  used in version 1.
- JSON strategy: a request with a JSON schema is sent first (the schema format
  of OpenAI and Ollama, a forced tool for Anthropic). When the server answers
  400 or 422, the schema goes into the prompt instead. An answer that does not
  pass Kirpitch's own checks is sent back once with the problem named.
- Nothing from a model is trusted: every pin must be found on one line of the
  page text (else its confidence is 0.3), an optional second reading marks
  differences (0.4), the package is built from dimensions with the IPC-7351
  templates, and the PartSpec goes through the same validation and review
  stages as one made by hand. Every value gets page, region and confidence.
- `local_only = true` refuses any server that is not localhost, loopback, a
  private address, `.local`/`.lan` or a single-label name, before a request is
  sent.
- Answers are cached per model, prompt version and input in `.kirpitch-cache`.
- Not in version 1: reading drawings from images (vision input is supported by
  the adapters and probed by `kirpitch models test`, but the extraction tasks
  work on page text), OCR, and prices for known models (give `price_in` and
  `price_out`).

## Consequences

No async runtime or TLS library is added. Vision-only datasheets (scanned or
image-only drawings) still need the manual flow.
