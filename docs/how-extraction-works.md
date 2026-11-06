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

## Watching it work, and the settings

While a part is built, the datasheet dialog has a **Model output** panel (fold
it open or shut; the choice is remembered). Each call to a model is listed: how
much was sent and to which model, then what came back, how long it took, how
many tokens, why the model stopped (`stop` or `length`), and the model's own
thinking when the server returns it. An answer that was not accepted shows the
reason, and the next try is listed under it. An empty answer is explained: if the
model wrote only thinking and ran out of room, the panel says so. **Copy all**
puts the whole log on the clipboard. Answers appear when a call ends (the
servers are not asked to stream), with a counter while one is running.

Everything that shapes the extraction is in **Settings, Extraction**: the
tries for each step, the time limit for one answer, the answer limit of each
step, how much page text is sent for the pin table and the package, reading the
pin table twice by default, keeping answers, and all the prompts. They are saved
in `kirpitch.toml` (`[extraction]` and `[prompts]`), so the command line uses
the same values.

## Packages this version cannot draw

The footprint templates cover SOIC, SSOP, TSSOP, MSOP, SOT-23, DIP and QFN. For
anything else (LQFP, TQFP, BGA, SOT-223 ...) the package step answers `other`
with the package name. Kirpitch then stops that step at once instead of asking
again, says which package it is, and still saves the pins it read as a CSV file.

## The guided way (recommended)

Letting the program search a long datasheet is the weakest part: it scores pages
by keywords, and a 200 page datasheet has hundreds of pages that mention pins.
The **guided** dialog replaces the search by your knowledge:

1. **Datasheet:** the PDF, where to save the part, an optional hint.
2. **Part number:** the page or a box with the title and description.
3. **Pins:** draw a box around the pin table (one box per page when it
   continues). The pages the program would have looked at are offered as
   suggestions, best first.
4. **Package:** a box around the dimension table and the package name.
5. **Build:** a summary with how much text will be sent.

The models then read exactly the text inside the boxes (the text of the PDF
inside the rectangle, not a picture), and nothing else is sent. Under each
step the window shows how many characters that is, against what the automatic
search would have sent. A step you do not point at is searched for as before,
and **Skip the guide** on the first step does the whole build that way. The
`Model output` panel shows what each call received.

## Any package, and the footprint of the datasheet

There is no list of supported packages. The package step reads the outline
drawing (body, height, standoff A1, body thickness A2, lead thickness c, lead
size) and names the shape: a dual-row or quad gull-wing shape, a no-lead shape,
a ball grid (BGA, LGA, CSP), or `custom` for anything else. A new step then reads
the **recommended footprint** (land pattern) of the datasheet and its pad sizes
and distances replace the calculated ones; for a shape with no template its
pads are drawn as they are. Without a recommended footprint the pads are
calculated by the IPC-7351B formulas. The 3D model is built from the same
drawing dimensions. See [ADR 0029](decisions/0029-any-package-from-the-datasheet.md).

## Step by step, with confirmation

The guided dialog reads one thing at a time. You point at the place (a page, a
box, or several pages with "Continue the table to…"), a model reads it, and you
see the result as values you can change: three fields for the part number, a
table for the pins (add, remove, paste a table), a form for the package that
follows the shape (the letters of the drawing), and the pad values of the
recommended footprint with a drawing of the pads that will be made. If nothing
could be read, the same form comes up empty and you type the values in.
**Confirm** checks them by the same rules a model's answer goes through, and a
pin that is not in the text you chose is marked and asked about once. After the
last step the part is made from the confirmed values alone, with no call to a
model, and checked first: nothing is written while the checks find errors.
See [ADR 0030](decisions/0030-step-by-step-with-confirmation.md).

## How each model runs

Under **Settings, Models**, each model has **Thinking and context**:

- **Thinking effort** (none, minimal, low, medium, high): sent the way each kind
  of server takes it: `reasoning_effort` for OpenAI-style servers (OpenAI's own
  reasoning models also get `max_completion_tokens`), `think` for Ollama (a level
  for models that take one, off for none), and an extended-thinking budget for
  Anthropic (1,024, 2,048, 8,192 or 24,000 tokens, with the thinking counted
  in the answer limit; a forced tool is not allowed with it, so the example in
  the prompt carries the shape). A model that thinks can spend its whole answer
  budget on thinking and answer nothing: turn the thinking off, lower the
  effort, or raise the answer limit under **Extraction**.
- **Context length**: for Ollama (`num_ctx`). Left empty, Kirpitch asks for what
  each question needs (in steps from 4,096 to 131,072 tokens), because Ollama's
  default is small and cuts the page text off without saying so.
- **Temperature**: 0 by default. Not sent when an effort is set, because models
  that think refuse it, unless you set one.
- **Extra request fields**: a JSON object added to the request body. It wins over
  what Kirpitch sets, so any option of a server can be used (it cannot replace
  the model name, the messages or `stream`).
