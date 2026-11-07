# Changelog

All notable changes to Kirpitch are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Fixed: an Ollama model whose address still ended in /v1 (left over from its OpenAI-compatible address) sent requests to ./v1/api/chat and got 404. The Ollama kind now ignores a stray /v1 or /api in the address, in requests, the model list and the details, and changing the kind to Ollama in Settings cleans the address.
- A model card asks the server about the model and shows its context window, answer limit, whether it thinks and whether it reads images (Ollama `/api/show`, also behind an OpenAI-compatible Ollama address; LM Studio; Anthropic; OpenRouter and Mistral lists), with "Use N as the context length" for Ollama. An Ollama server set up as OpenAI-compatible is explained and can be switched to the Ollama kind in one click.
- Models have Thinking and context settings: thinking effort (sent as reasoning_effort, Ollama think, or an Anthropic thinking budget), context length (Ollama num_ctx; automatic when empty, so Ollama no longer cuts page text off at its small default), temperature and extra request fields as JSON (`effort`, `context_length`, `temperature`, `extra_body` in `kirpitch.toml`).
- In the review of a step the datasheet is shown next to the values, scrolled to the place you pointed at with the boxes outlined; clicking (or tabbing to) a pin row jumps to the line where that pin was read, and a pin that was not found only outlines its page. The dialog widens for it.
- Reading a datasheet step by step with confirmation: for each of part number, pins, package outline and recommended footprint you point at the place, a model reads it, and you check and correct the result as an editable form (a table for pins, a form for the package, pad values with a drawing of the footprint). Nothing that cannot be read is a dead end: the form comes up empty to type in. After the last step the part is made from the confirmed values alone, with no call to a model, and a dry run of the validation and 3D fit checks shows any error on the last screen before anything is written (ADR 0030). Reading one step is `kirpitch-models::steps`.
- The guided steps show the whole datasheet as one scrolling column (pages are drawn as they come into view), the page box and arrows jump to a page, boxes are drawn on the page under the pointer, and "Continue the table to…" adds every page from the last box down to a clicked point with the same columns (shown as one "pages 48–51" chip). Fixed the Footprint step that could not be opened (the backend did not send footprint page suggestions) and added a test that pins the names the window reads.
- Fixed boxes drawn on a datasheet page (guided steps and the review sheet) landing above the cursor and not scrolling with the page: they are now placed inside the page image, not the scrolling frame.
- Any package: LQFP/TQFP/QFP, ball grids (BGA, LGA, CSP) and anything else (custom pads) are drawn from the datasheet. A new step reads the recommended footprint and its pad sizes and distances replace the calculated ones (a note says how far apart they are); the 3D model uses the drawing's standoff, body thickness and lead thickness and has balls under grid packages; the guided dialog has a Footprint step; Settings has the two new limits (ADR 0029). Golden 3D files were regenerated.
- Guided reading of a datasheet: the dialog is now a sequence of steps (datasheet, part number, pins, package, build) where you point at the page or draw a box over the part number, the pin table and the package table; the models read only the text inside the boxes. Suggested pages come from the same keywords as the automatic search, and each step shows how many characters will be sent against the automatic search. Steps you skip are searched for as before (`kirpitch-review::guide`, `GivenText` in `kirpitch-models`).
- The datasheet dialog has a collapsible Model output panel that lists every call to a model: what is sent, what comes back, the thinking, tokens, time, why it stopped and why an answer was rejected. Settings has an Extraction tab with the tries, time limit, answer limits, page text sizes, read-twice default and answer cache (`[extraction]`), next to the prompts. A package that cannot be drawn (LQFP, BGA ...) is named and ends the step at once; the pins are still saved. The retry for an empty answer also works for Ollama servers used as OpenAI-compatible. The old Model settings path field of the dialog is gone: Settings is the only source.
- Evaluate tab in Settings: run the saved models and prompts on datasheets with known answers (golden parts once you point to their PDF, or your own reviewed parts), see pins, dimensions and pads right per part, and keep every run with its exact prompts, models and a prompt set code; compare runs and bring a good prompt set back (`kirpitch-review::eval`).
- Extraction prompts can be changed: Settings has an Extraction prompts tab (system prompt, part number, pin table, second reading, package) with the default shown, a changed badge, placeholder checks and Reset; they are saved in `[prompts]` of `kirpitch.toml` and a short prompt set code names each version (`kirpitch-models::prompts`).
- Extraction is more reliable: prompts end with an example answer instead of the JSON schema (models were filling in the schema), answers inside a wrapper (`properties`, `answer`, ...) are read, an empty answer is asked again with thinking turned off and more room, and up to two repairs are tried. `docs/how-extraction-works.md` explains the steps. Settings warns when an Ollama server is set up as OpenAI-compatible.
- Reading a datasheet shows real progress: each step (PDF text, part number, pin table, package, check, save) in order, the one being worked on spinning, finished ones with a green check and their time, and a clock that counts up. The window asks the build for its log a few times a second (`build_progress`), so it needs no Tauri permissions. The made-up rotating messages are gone. Reports come from `kirpitch-models::build_with_progress`.
- Settings screen (gear icon): add models from presets (Ollama, Claude, OpenAI, LM Studio, OpenRouter, Mistral, Gemini, any OpenAI-compatible server) with address, key and options; keys go to the system keychain (`kirpitch-models::keys`, ADR 0027); test a model; choose which model does which task and the local-only switch; default folders for parts, KiCad output and datasheets, KiCad version, fab profile and pdfium folder. Dialogs start in those folders and the Export step is pre-filled. The command line reads the same `kirpitch.toml` (now also from the user's settings folder) and the keychain keys.
- Settings: **Find models** asks the server for its list of models (Ollama local and ollama.com, Anthropic, OpenAI, OpenRouter, Mistral, Gemini, LM Studio, any OpenAI-compatible server) and fills in prices, context size and image support where they are known; the list loads by itself after a key is saved or a keyless preset is added. New preset for Ollama cloud. Prices come from the server when it publishes them (OpenRouter), else from OpenRouter's public list by model name, marked as such (`kirpitch-models::discover`, ADR 0028).
- Settings, better to use: the model field is a searchable list that opens with every model showing (keyboard: arrows, Enter, Esc) and accepts a name that is not listed; model lists load by themselves when a card opens and are cached for ten minutes; adding a model starts from cards for each provider; model cards fold to one line; the key box can show the key and says where to get one; names are checked as you type.
- Settings polish: a styled checkbox, no layout shifts (the model list updates only its own field, lines that come and go keep their room, scroll position is kept), Remove asks inside the card, price boxes refuse non-numbers, the screen re-reads the saved settings when the window regains focus.
- App: a new layout for the part view. A top bar with Home, a breadcrumb and a menu (save a copy, close, open by path); a guided stepper (Pinout, Footprint, 3D model, Export) with the findings of the current step, a fixed bar with Approve and Next, a progress bar, the datasheet beside the steps that need it, status badges for recent parts, and keyboard shortcuts (Esc for Home, Ctrl+O, Ctrl+N, Alt+Left and Alt+Right). The editing, drawings and datasheet pane are unchanged.
- App: a livelier look. A hero with an animated chip and the mascot Pinny, example parts as cards, rise-in and hover motion, a pulse on the selected pin, pops on stage chips, a confetti burst and a "Part ready to ship" banner when all three stages are approved, friendlier empty states and loading lines. Everything that moves stops with the system setting for reduced motion (`ui/fun.css`, `ui/fun.js`).
- App: easier ways to start. A start screen with the example parts (as working copies, so the originals never change) and recent files; native file dialogs (Open file, Browse for the datasheet and the output folder, Save copy); New part from a package form with presets (SOIC, TSSOP, SOT-23, DIP, QFN); New part from a datasheet with a language model; dropping a .json or .pdf on the window; remembered folders. The logic is in `kirpitch-review::parts` and tested; file dialogs use `rfd` (MIT) without its portal features.
- First measured model results (`docs/model-recommendations.md`): on the three golden SOIC-8 datasheets a real model read the pinout and the package dimensions 100% right. Fixes found by the real runs: schema always in the prompt, JSON taken from prose, retry with more tokens when an answer is cut off, hint words and weighted page choice, plain hyphens in names, JEDEC letters for the package, `ollama` `:cloud` models count as not local, `KIRPITCH_DEBUG_MODELS`, `think` option.
- CI: the app is split into `kirpitch-review` (logic and tests) and `kirpitch-app` (the Tauri window). Only a separate `Window` job installs the web view libraries; all other jobs run with `--exclude kirpitch-app`.
- Symbol checks against the KiCad library conventions (`klc_symbol::check_symbol`): fields, grid, pin length, pins touching the body, text sizes, overlapping pins. Tested on the golden symbols and on symbols of SOIC, DIP and QFN parts for KiCad 8, 9 and 10. Rule numbers are still not cited (the KLC site refuses automated access).
- Language model backend (`kirpitch-models`): `kirpitch.toml` with per-task routing and `local_only`, adapters for Anthropic, OpenAI-compatible servers and Ollama over the system `curl`, schema-checked answers with one repair step, cache. Commands `kirpitch build`, `kirpitch models test` and `kirpitch bench` (ADR 0026, `kirpitch.example.toml`).
- 3D model: `kirpitch-gen-3d` builds a parametric model of body, pin 1 mark and terminals and writes STEP and VRML; `gen 3d`, `gen all` writes `kirpitch.3dshapes` and links it from the footprint; review stage `3d` with the `model_fit` check; flat views in the window (ADR 0025).
- QFN support: `ipc::flat_no_lead`, `quad_flat_no_lead`, `kirpitch template ... qfn`, checked against KiCad's QFN-32 footprint.
- Fab profiles from a JSON file (`--fab FILE`, `KIRPITCH_FAB_PROFILE`); see ADR 0024.
- Cargo workspace with the `kirpitch-core` crate (stub).
- GitHub Actions CI (format, clippy and tests on Linux, macOS and Windows) and
  `cargo deny` licence and security checks.
- `kirpitch-core` primitives: millimetre lengths, tolerances, unit conversion,
  source tracing types and pin electrical types.
- PartSpec schema (version 1) with JSON loading that names the bad field, a
  built-in generic fab profile and structural validation (pin count, pin
  numbers and names, pin-to-pad match, tolerance order, units, pad clearance,
  positive sizes, source sanity).
- JSON schema of the PartSpec, generated from the Rust types and committed as
  `schema/partspec.schema.json` (a test fails when it is stale).
- `kirpitch-kicad` crate with a `kicad-cli` wrapper (version, SVG export,
  upgrade) and test helpers; tests use `KIRPITCH_KICAD_CLI` and skip without it.
- S-expression writer and parser in `kirpitch-kicad` that reproduces KiCad 10's
  own layout, and the `KicadTarget` type (KiCad 10 so far).
- `kirpitch-gen-footprint`: KiCad 10 footprint generator (SMD pads, courtyard,
  fab outline with pin 1 chamfer, silkscreen, pin 1 marker) with snapshot and
  `kicad-cli` tests.
- `kirpitch-gen-symbol`: KiCad 10 symbol generator (pins placed by type on the
  2.54 mm grid, stacked power pins, MPN and manufacturer fields) whose output
  equals KiCad's own canonical layout.
- Golden test set: TI LM358, TI NE555 and Microchip 24LC256 in SOIC-8 with
  datasheet URL and SHA-256 (no PDFs), hand-written PartSpecs and expected KiCad
  files; symbols place bidirectional pins on the right.
- `cargo xtask fetch-datasheets` and `verify-datasheets`: download the golden
  datasheets into the ignored `testdata/pdfs/` and report changed hashes (drift).
- `kirpitch` command-line tool with `schema`, `check` and `gen symbol|footprint|all`
  (KiCad target from `--target`, the installed KiCad, or the newest supported).
- CI job that runs the `kicad-cli` tests in the official KiCad 10 container, with
  `scripts/kicad-cli-docker.sh` as the wrapper.
- KiCad 9 writers for symbols and footprints (`--target 9`), with expected files
  per KiCad version and a KiCad 9 job in the `kicad-cli` workflow.
- KiCad 8 writers for symbols and footprints (`--target 8`) and a KiCad 8 job in the
  `kicad-cli` workflow.
- Milestone 2 decisions (ADRs 0015 to 0020): split in 2a and 2b, pdfium for PDFs,
  KLC checks in Rust, silkscreen width in the generic fab profile, exposed pads,
  golden set growth.
- The generic fab profile allows 0.12 mm silkscreen lines, as the generators draw them (ADR 0018).
- PartSpec `origin` (hand, region, llm): values read from a datasheet must carry a
  source with page and region, and a confidence when a model read them
  (`source_required` check, ADR 0004).
- More checks: `power_pins`, `pitch_fit`, `exposed_pad` (ADR 0019) and `override_unknown`.
- IPC-7351B land pattern calculator for gull-wing leads (`kirpitch_core::ipc`),
  checked against KiCad's official SOIC and TSSOP footprints (ADR 0021).
- `two_sources` check: the datasheet land pattern is compared with the IPC-7351
  result when the package dimensions E, L and b are given.
- Package template for dual-row gull-wing packages (SOIC, SOP, SSOP, TSSOP, MSOP)
  in `kirpitch_core::templates`: JEDEC letters in, package and IPC-7351 land
  pattern out, matching KiCad's official SOIC-8, SOIC-16 and TSSOP-14 pads.
- SOT-23 template (3, 5 and 6 leads) matching KiCad's pin positions and SOT-23-5 pads;
  footprints made from templates load in KiCad 8, 9 and 10.
- Through-hole pads (`kind: thru_hole`, `drill`), body centre, the `drill` check, the DIP
  template and through-hole footprints (silkscreen between the rows, pin 1 notch).
- Pin table import (`kirpitch_core::pintable`): CSV, tab, semicolon, bar or space
  separated text with or without header; guessed types are reported.
- Review flow without a model: `kirpitch template`, `pins`, `status`, `review`
  (with `--edit`) and `approve`; approvals store a fingerprint so editing an
  approved stage re-opens it and later stages (ADR 0022).
- First KLC footprint checks (`kirpitch_kicad::klc`): courtyard, fab outline,
  silkscreen width, text layers; generated footprints pass (ADR 0017 update).
- `kirpitch-extract`: PDF page rendering, characters with positions and text of a
  region (pdfium via `pdfium-render`, ADR 0016); `cargo xtask fetch-pdfium`; `PDF`
  workflow on Linux, macOS and Windows.
- Region pick: `kirpitch pins --pdf FILE --page N --region l,t,r,b` reads a pin table from
  a region of a datasheet page (column detection from text positions); the page
  and region are saved as the source of every pin.
- Desktop app (`kirpitch-app`, Tauri 2): open a PartSpec, see both review gates, pin and pad
  tables with source pages, findings and a footprint drawing, approve stages and
  write KiCad files. Start it with `cargo run -p kirpitch-app` (ADR 0023).
- App: the datasheet page next to the pin table. Click a pin or pad row to jump to the
  page it was read from, with a box around the region when the source has one;
  pages are rendered with pdfium.
- App: pin table, symbol, footprint and datasheet are always visible side by side; selecting
  a pin in any of them highlights it in all of them and shows its datasheet page.
- App: drag on the datasheet page to select a table; the text is split into columns and
  shown for correction, then "Use as pin table" imports the pins with page and region
  as their source (guessed types are listed).
- App: edit the pins in the table (number, name, type, group), move, add and delete pins;
  changes are saved at once, keep each pin's datasheet source, and reopen an approved pinout.
- Project documentation: requirements, contributing guidelines and
  architecture decision records.
