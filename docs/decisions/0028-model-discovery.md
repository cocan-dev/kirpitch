# 0028. Finding models and prices

Status: accepted

## Context

Typing a model name and its prices by hand is slow and error prone, and a
wrong name only shows up as an error on the first datasheet.

## Decision

- Each kind of server is asked for its list: `/api/tags` (Ollama, local or
  ollama.com), `/v1/models` (Anthropic, OpenAI, OpenAI-compatible servers,
  Gemini's `/v1beta/openai/models`). Only a GET with the model's own key is
  sent, to the model's own server. No datasheet content is involved.
- Prices, context size and image support are used only when a server publishes
  them (OpenRouter does; Mistral publishes image support and context size).
  Anthropic, OpenAI, Gemini and the others do not publish prices through their
  APIs, so for remote servers the public OpenRouter list (no key, no account)
  is searched by normalised model name (vendor prefix, dates and `-latest`
  removed). A price found that way is marked "OpenRouter list" on the screen,
  because it is a reference and not the user's own plan. Local servers and
  Ollama are never asked for prices, and a failed lookup only means no prices.
- Prices are never written without the user choosing the model in the list;
  they stay editable under More options.
- Prices are not built into the program: they change, and a stale table would
  be wrong silently.

## Consequences

- A request to openrouter.ai happens when listing models of a remote server
  other than OpenRouter itself. It carries no key and no user data.
- A model without a match simply has no price and the cost estimate stays
  empty.
