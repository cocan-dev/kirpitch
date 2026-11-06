# Measured model recommendations

This table is made by `kirpitch bench` on the golden set (`testdata/golden`)
and is meant to grow with every release. Numbers are measured, not claimed.
Read the limits below before relying on them.

## How it is measured

`kirpitch bench` reads the text of each golden datasheet, runs `kirpitch build`
with the configured models and compares the result with the golden PartSpec:

- **pinout**: pins whose number, name and electrical type are all right, out of
  the golden pins.
- **dimensions**: package dimensions (minimum and maximum) that match, out of
  the dimensions that both PartSpecs record.
- **pads**: pads at the same place and size as the golden land pattern. They come
  from the IPC-7351 formulas, while a golden part may carry the maker's own
  land pattern, so a low number here is expected and is not an error of the model.

The package hint is the package text of the golden `datasheet.json`, for example
`8-Lead SOIC (SN), 3.90 mm body, Microchip drawing C04-057-SN`. That is what a
person would type with `--hint` for a datasheet that covers several packages.

## Results

| Date | Model | Run by | Pinout | Dimensions | Pads | Time | Tokens (in / out) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-11-06 | `glm-5.3-flash:cloud` through a local Ollama (0.40), `--second-read` | `kirpitch bench` | 100% (3 of 3 parts) | 100% | 0% | 74 s for 3 parts | 28,372 / 18,473 |

Data of this run left the machine: the `:cloud` models of Ollama run at
ollama.com (Kirpitch knows this and counts them as not local for `local_only`).

## Limits

- Three parts, all 8-pin SOIC (LM358, NE555, 24LC256). This says that the method
  works on text-based datasheets of this kind; it does not say how a model does
  on QFN, connectors or scanned datasheets.
- The golden PartSpecs had an AI second pass, not a human review.
- One model, one run. Models answer differently each time even at temperature 0;
  repeat runs before choosing a model.
- A model that thinks before it answers can use its whole answer budget on that
  and answer nothing. Kirpitch allows four times more tokens on the retry, but
  such models are slow (this one needed up to 76 s for a part).
- Reading drawings from images is not part of version 1.
