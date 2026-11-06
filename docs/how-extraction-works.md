# How Kirpitch reads a datasheet

This describes what happens when you press **Build part** (or run
`kirpitch build`). The code is in `crates/kirpitch-models/src/tasks.rs` and
`build.rs`.

## The steps

1. **Read the PDF.** pdfium extracts the text of every page, line by line, with
   each line's position. Scanned PDFs have no text and are refused.
2. **Metadata** (part number, maker, one-line description). The text of the
   first two pages goes to the model assigned to the `metadata` task.
3. **Pin table.** Pages are scored by keywords ("pin functions", "pin
   description", "pinout", ... and negative words such as "tape and reel").
   Your hint (for example "pinout on page 43") adds weight to the pages it
   names. The best pages, up to about 24,000 characters, go to the `pin_tables`
   model. With "Read the pin table twice" a second differently worded request
   is made and any pin the two readings disagree on is flagged.
4. **Package.** The same page selection with other keywords ("package
   outline", "mechanical data", "millimeters", ...) for the `drawings` model.
   It returns the JEDEC letters (e, A, D, E, E1, L, b, and D2/E2 for QFN) and
   a package family.
5. **Assemble and check.** The three answers become a PartSpec. Every pin is
   looked up in the page text (a pin that is on no line gets low confidence and
   a note); the package family picks a template (SOIC, SSOP, TSSOP, MSOP,
   SOT-23, DIP, QFN) that builds the land pattern by the IPC-7351 formulas.
6. **Save**, then you review the pinout, footprint and 3D gates by hand.

## What is sent to a model

Each request has three parts:

- a **system prompt** (`SYSTEM` in `tasks.rs`): report only what the text says,
  never guess, never use remembered knowledge, leave a value out when the text
  does not give it;
- a **task instruction** (what to read, which letters, which types);
- the **page text**, with `[page N]` markers, and the **hint**.

The prompt ends with an **example answer** whose values only show the shape.
Models follow an example far better than a schema. Servers that can enforce a
JSON schema (OpenAI, Ollama's `format`, Anthropic tools) also get the schema as
a constraint; servers that cannot simply rely on the example.

## What happens when an answer is bad

- JSON is taken out of the answer wherever it sits (code fence, sentences).
- If the answer is wrapped (`{"properties": {...}}`, `{"answer": {...}}`), the
  inside is read.
- If the check fails, the model is asked again with the problem named, up to
  two more times.
- If the answer was **cut off** (hit the token limit) the limit is raised.
- If the answer was **empty**, as happens with models that think and use their
  whole budget on thinking, the next request turns thinking off (Ollama) and
  allows more tokens.
- Answers are cached on disk by request, so a repeat build costs nothing.

## Where it still goes wrong

- A model that does not follow instructions at all. Use the **Test** button in
  Settings: a model must pass Reachable and Structured JSON.
- The wrong pages chosen. Give a hint with page numbers.
- Pin tables printed as images or drawn rather than typed.

For Ollama (local or cloud) choose the **Ollama** kind, not "OpenAI-compatible":
the native API lets Kirpitch turn thinking off and enforce the answer's shape.

## Changing the prompts

Every prompt above can be edited: in the app under **Settings, Extraction
prompts**, or in the `[prompts]` section of `kirpitch.toml` (`system`,
`metadata`, `pins`, `pins_second`, `package`). A prompt that is left out, or
equal to the default, is the default. In the instructions, `{pages}` marks where
the page text goes (it is added at the end if missing), `{hint}` your hint and
`{types}` the electrical types.

The settings screen shows a short **prompt set code**. It changes whenever any
prompt changes, so a score can be tied to the prompts that produced it.

## Measuring changes

**Settings, Evaluate** runs the saved models and prompts on *cases*: a datasheet
PDF plus the part it should produce. A case is either a golden part of the
repository (point to its PDF once; the PDFs are not shipped) or a part you have
reviewed, with its datasheet. For each case the run counts the pins, package
dimensions and pads that come out right, with the tokens and time used, and
keeps the run (prompts, models, scores) in the `evals` folder next to your
settings file. The History shows each run against the one before it, and
**Use these prompts** puts the prompts of any run back. The command line has the
same measurement for the golden parts: `kirpitch models bench`.
