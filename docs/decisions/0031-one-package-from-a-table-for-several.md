# 0031. One package from a pin table that covers several

Status: accepted

## Context

A pin table in a family datasheet has a column of pin numbers for each package.
The pin step read all of the rows, so a part for the 64-pin package got 176
pins and the build stopped. The first answer was to ask the model to read the
table again for one package. That was slow, and a model that thinks too long
(or ignores "thinking off") could take minutes, run out of room and time out, and
the screen jumped back to the pins step without saying why.

## Decision

- Cutting the table is done by the program, not a model: each pin is found in
  the page text, the cells before its name are the pin numbers of each package
  (`leading_cells`), and the column is the one that holds the numbers 1 to the
  pin count of the package, each once, with only a few missing
  (`narrow_to_package`). The headers are sideways text and are not used.
- A column that fits more than one way, or not at all, is an error with a
  message; the model re-read stays as the fallback.
- Pins that only smaller packages have (extra VSS and VDD) are read from the
  rows of the table, marked with low confidence and listed as notes; pin numbers
  that cannot be found are named, never invented.
- The button says that it goes to the pins step, the result is shown for
  checking there, and confirming returns to the build screen.
- When an answer is cut off and asked again with more room, the time limit
  grows in proportion (up to one hour). The output panel says when thinking was
  asked to be off but the model thought anyway.

## Consequences

It is instant and works without a model, and the person still confirms the
pins. It relies on the text layer of the PDF: a table drawn as an image is not
cut (the message says so). A table with two columns that both look like pin
numbers of the same size is refused rather than guessed.
