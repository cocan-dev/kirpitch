// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// Reading a datasheet step by step. For each thing in turn (part number, pins,
// package outline, recommended footprint) you point at where it is, a model
// reads it, and you see the result as values you can change. If nothing could
// be read you type the values in. Only confirmed values go on, and after the
// last step the part is made from them alone, with no further call to a model.

const GD_STEPS = [
  { id: "file", label: "Datasheet" },
  { id: "metadata", label: "Part number", key: "metadata", title: "Where are the part number and the description?", help: "Usually the first page: the title and the first lines of text. Draw a box around them, or use the whole page.", review: "Check the part number" },
  { id: "pins", label: "Pins", key: "pins", title: "Where is the pin table?", help: "Draw a box around the table that lists every pin: number, name and function. If it continues on the next page, draw another box, or use “Continue the table to…”.", review: "Check the pins" },
  { id: "package", label: "Package", key: "package", title: "Where is the package outline?", help: "Draw a box around the package outline drawing and its dimension table: body size, height, and when the drawing has them the standoff, the thickness of the body and of a lead, and the lead size. Include the line that names the package.", review: "Check the package" },
  { id: "footprint", label: "Footprint", key: "footprint", title: "Where is the recommended footprint?", help: "Most datasheets end with a recommended footprint (land pattern): the pads with their sizes and distances. Draw a box around it and its dimensions. The pads are then taken from it. If there is none, the pads are calculated from the package.", review: "Check the footprint" },
  { id: "build", label: "Build" },
];
const GD_LAST = GD_STEPS.length;

const PIN_TYPES = ["input", "output", "bidirectional", "tri_state", "passive", "power_in", "power_out", "open_collector", "open_emitter", "no_connect", "free", "unspecified"];

function gdIsPick(step) {
  return !!GD_STEPS[step - 1].key;
}

function gdEmpty() {
  return { metadata: [], pins: [], package: [], footprint: [] };
}

const gd = {
  step: 1,
  mode: "steps", // "steps", or "auto": the program searches everything
  pdf: "",
  count: 0,
  page: 1,
  ratio: 1.414,
  regions: gdEmpty(),
  suggestions: null,
  phase: {}, // key -> "point" | "reading" | "review"
  draft: {}, // key -> the values on the screen
  values: {}, // key -> confirmed values
  confirmed: {},
  readError: {},
  notes: {},
  ack: {},
  ready: null,
  useFootprint: true,
  loading: false,
  drag: null,
  extending: false,
};

function gdKey() {
  return GD_STEPS[gd.step - 1].key;
}

function gdPhase(key) {
  return gd.phase[key] || "point";
}

function guidedInput() {
  return null; // the steps carry what was confirmed; the automatic build takes no places
}

function gdConfirmedMode() {
  return gd.mode === "steps";
}

function gdForget() {
  gd.regions = gdEmpty();
  gd.suggestions = null;
  gd.phase = {};
  gd.draft = {};
  gd.values = {};
  gd.confirmed = {};
  gd.readError = {};
  gd.notes = {};
  gd.ack = {};
  gd.ready = null;
}

function gdReset() {
  const pdf = $("ds-pdf").value.trim();
  if (pdf !== gd.pdf) gdForget();
  gd.pdf = pdf;
  gd.mode = "steps";
  gd.extending = false;
  gdShow(1);
}

// ---- Moving between steps ------------------------------------------------------------------

function gdError(text) {
  $("ds-error").textContent = text || "";
  $("ds-error").hidden = !text;
}

async function gdGo(step) {
  gdError("");
  gd.returnToFinal = false;
  if (step > 1 && gd.step === 1) {
    const pdf = $("ds-pdf").value.trim();
    if (!pdf) return gdError("Choose the datasheet PDF first.");
    if (!$("ds-out").value.trim()) return gdError("Choose where to save the part.");
    if (pdf !== gd.pdf) gdForget();
    gd.pdf = pdf;
    if (!gd.suggestions) {
      gd.loading = true;
      gdShow(gd.step);
      try {
        gd.suggestions = await invoke("suggest_pages", { pdf, hint: $("ds-hint").value });
        gd.count = gd.suggestions.page_count;
      } catch (e) {
        gd.loading = false;
        gdShow(1);
        return gdError(String(e));
      }
      gd.loading = false;
    }
  }
  try {
    if (gdIsPick(step)) {
      // The viewer starts on the first place already chosen, else on the best suggestion.
      const key = GD_STEPS[step - 1].key;
      const chosen = gd.regions[key][0];
      const best = ((gd.suggestions && gd.suggestions[key]) || [])[0];
      gd.page = chosen ? chosen.page : best ? best.page : 1;
    }
    gd.step = step;
    gd.extending = false;
    gdShow(step);
    if (gdIsPick(step) && gdPhase(GD_STEPS[step - 1].key) === "point") await gdLoadPage();
    if (step === GD_LAST) await gdEnterFinal();
  } catch (e) {
    gdError(String(e));
    gdShow(gd.step);
  }
}

function gdShow(step) {
  gd.step = step;
  const s = GD_STEPS[step - 1];
  const key = s.key;
  const phase = key ? gdPhase(key) : "";
  $("gd-p1").hidden = step !== 1;
  $("gd-pick").hidden = !(key && phase === "point");
  $("gd-reading").hidden = !(key && phase === "reading");
  $("gd-review").hidden = !(key && phase === "review");
  $("gd-p5").hidden = step !== GD_LAST;
  // The page is shown to point at it, and next to the values to check them against it.
  $("gd-split").hidden = !(key && (phase === "point" || phase === "review"));
  $("gd-split").classList.toggle("review", phase === "review");
  $("gd-viewer").classList.toggle("review-mode", phase === "review");
  $("ds-dialog").classList.toggle("wide", phase === "review");
  const busy = gd.loading || phase === "reading";
  $("gd-back").hidden = step === 1;
  $("gd-back").disabled = busy;
  $("gd-skip").hidden = !(key && phase === "point");
  $("gd-nofp").hidden = !(key === "footprint" && phase === "point");
  $("gd-again").hidden = !(key && phase === "review");
  $("gd-reread").hidden = !(key && phase === "review");
  $("gd-next").hidden = step === GD_LAST;
  $("gd-next").disabled = busy;
  $("gd-next").textContent = gd.loading ? "Reading the datasheet…" : phase === "reading" ? "Reading…" : phase === "review" ? (gd.ack[key] ? "Confirm anyway" : "Confirm and continue") : key ? "Read it" : "Next";
  $("ds-run").hidden = step !== GD_LAST;
  $("gd-skip").textContent = "Let the program search for this";
  if (key && phase === "point") {
    $("gd-pick-title").textContent = s.title;
    $("gd-pick-help").textContent = s.help;
    gdPaintSuggestions();
    gdPaintRegions();
    gdPaintSize();
  }
  if (key && phase === "review") {
    gdPaintReview(key);
    gdShowSource(key);
  }
  if (step === GD_LAST) gdPaintFinal();
  gdPaintStepper();
}

function gdPaintStepper() {
  const ol = $("gd-stepper");
  const reading = gd.step >= 2 && gd.step < GD_LAST && gdPhase(gdKey()) === "reading";
  ol.replaceChildren(
    ...GD_STEPS.map((s, i) => {
      const n = i + 1;
      const done = s.key ? !!gd.confirmed[s.key] : n < gd.step;
      const li = el("li", { class: "gd-step" + (n === gd.step ? " current" : "") + (done ? " done" : "") });
      const b = el("button", { type: "button", class: "gd-step-btn", "aria-current": n === gd.step ? "step" : "false" });
      b.append(el("span", { class: "gd-dot", text: done && n !== gd.step ? "✓" : String(n) }), el("span", { text: s.label }));
      b.disabled = gd.loading || reading || (n > 1 && !gd.suggestions) || n === gd.step;
      b.addEventListener("click", () => gdGo(n));
      li.append(b);
      return li;
    })
  );
}

// ---- Reading a step ------------------------------------------------------------------------------

let gdPoll = null;
let gdSeen = 0;
let gdRunStart = 0;

function gdPollStart() {
  gdSeen = 0;
  gdRunStart = Date.now();
  logRunId++;
  $("pg-verbose").hidden = false;
  clearInterval(gdPoll);
  gdPoll = setInterval(async () => {
    $("gd-reading-time").textContent = Math.floor((Date.now() - gdRunStart) / 1000) + " s";
    try {
      const events = await invoke("build_progress", { since: gdSeen });
      gdSeen += events.length;
      events.forEach((e) => {
        if (e.state === "call" && e.call) logCall(e.call);
      });
    } catch (e) {
      // The next poll tries again.
    }
  }, 300);
}

function gdPollStop() {
  clearInterval(gdPoll);
  gdPoll = null;
}

const GD_MANUAL = {
  metadata: () => ({ mpn: "", manufacturer: "", description: "" }),
  pins: () => ({ pins: [] }),
  package: () => ({ family: "soic" }),
  footprint: () => ({}),
};

async function gdRead(key, withPlaces, extra) {
  gdError("");
  gd.phase[key] = "reading";
  gd.ack[key] = false;
  $("gd-reading-title").textContent = "Reading: " + GD_STEPS.find((s) => s.key === key).label.toLowerCase();
  $("gd-reading-where").textContent = withPlaces && gd.regions[key].length ? "from the places you pointed at" : "searching the whole datasheet";
  $("gd-reading-time").textContent = "0 s";
  gdShow(gd.step);
  gdPollStart();
  try {
    const run = await invoke("read_step", { input: { pdf: gd.pdf, step: key, regions: withPlaces ? gd.regions[key] : [], hint: [$("ds-hint").value.trim(), extra && extra.hint].filter(Boolean).join(". "), second_read: $("ds-second").checked } });
    gd.draft[key] = run.value;
    gd.notes[key] = run.notes || [];
    gd.readError[key] = "";
  } catch (e) {
    // Nothing usable came back: the values are typed in (or, when a read was only
    // meant to narrow down what is there, what was there stays).
    if (!(extra && extra.keep)) {
      gd.draft[key] = GD_MANUAL[key]();
      gd.notes[key] = [];
    }
    gd.readError[key] = String(e);
  }
  // The last calls of the step.
  try {
    const events = await invoke("build_progress", { since: gdSeen });
    events.forEach((e) => e.state === "call" && e.call && logCall(e.call));
  } catch (e) {
    // Nothing more to show.
  }
  gdPollStop();
  gd.phase[key] = "review";
  if (gdKey() === key) gdShow(gd.step);
}

// What is on the screen is checked by the same rules a model's answer goes through.
async function gdConfirm(key) {
  gdError("");
  const backToFinal = key === "pins" && gd.returnToFinal;
  let value = gd.draft[key];
  if (key === "footprint" && !gd.useFootprint) value = null;
  if (value === null) {
    gd.values[key] = null;
    gd.confirmed[key] = true;
    return gdGo(gd.step + 1);
  }
  try {
    const run = await invoke("check_step_values", { pdf: gd.pdf, step: key, value, regions: gd.regions.pins });
    gd.draft[key] = run.value;
    // Things worth a look are shown once; pressing again goes on.
    if (run.notes.length && !gd.ack[key]) {
      gd.notes[key] = run.notes;
      gd.ack[key] = true;
      gdShow(gd.step);
      return;
    }
    gd.values[key] = run.value;
    gd.confirmed[key] = true;
    gd.ack[key] = false;
    return gdGo(backToFinal ? GD_LAST : gd.step + 1);
  } catch (e) {
    gd.ack[key] = false;
    gdShow(gd.step);
    gdShowCheckError(String(e));
  }
}

function gdShowCheckError(text) {
  const box = $("gd-review-error");
  box.textContent = text;
  box.hidden = !text;
}

// ---- Review: the values of a step, to check and change ---------------------------------------------

function gdLabel(text, input, help) {
  const wrap = el("label", { class: "gd-f" }, el("span", { class: "gd-lab", text }), input);
  if (help) wrap.append(el("span", { class: "muted small", text: help }));
  return wrap;
}

function gdText(obj, key, placeholder) {
  const i = el("input", { type: "text", spellcheck: "false", autocomplete: "off" });
  i.value = obj[key] == null ? "" : String(obj[key]);
  if (placeholder) i.placeholder = placeholder;
  i.addEventListener("input", () => {
    const v = i.value.trim();
    if (v === "") delete obj[key];
    else obj[key] = v;
    gdTouched();
  });
  return i;
}

// A number: empty means it is not given; something that is not a number is marked.
function gdNum(obj, key, placeholder, integer) {
  const i = el("input", { type: "text", inputmode: "decimal", spellcheck: "false", autocomplete: "off" });
  i.value = obj[key] == null ? "" : String(obj[key]);
  if (placeholder) i.placeholder = placeholder;
  i.addEventListener("input", () => {
    const text = i.value.trim().replace(",", ".");
    if (text === "") {
      delete obj[key];
      i.classList.remove("invalid");
    } else {
      const n = Number(text);
      const bad = !Number.isFinite(n) || n < 0 || (integer && !Number.isInteger(n));
      i.classList.toggle("invalid", bad);
      if (!bad) obj[key] = n;
    }
    gdTouched();
  });
  return i;
}

// A range from the drawing: the smallest and the largest value.
function gdRange(obj, key) {
  const cur = obj[key] && typeof obj[key] === "object" ? obj[key] : {};
  const row = el("span", { class: "gd-range" });
  const part = (k, ph) => {
    const i = el("input", { type: "text", inputmode: "decimal", spellcheck: "false", autocomplete: "off", placeholder: ph });
    i.value = cur[k] == null ? "" : String(cur[k]);
    i.addEventListener("input", () => {
      const text = i.value.trim().replace(",", ".");
      const next = { ...(obj[key] && typeof obj[key] === "object" ? obj[key] : {}) };
      if (text === "") {
        delete next[k];
        i.classList.remove("invalid");
      } else {
        const n = Number(text);
        i.classList.toggle("invalid", !Number.isFinite(n) || n <= 0);
        if (Number.isFinite(n) && n > 0) next[k] = n;
      }
      if (next.min != null || next.max != null) obj[key] = next;
      else delete obj[key];
      gdTouched();
    });
    return i;
  };
  row.append(part("min", "min"), part("max", "max"));
  return row;
}

let gdTouchTimer = null;
// Something was changed on the screen: the footprint preview follows.
function gdTouched() {
  gdShowCheckError("");
  if (gdKey() === "footprint") {
    clearTimeout(gdTouchTimer);
    gdTouchTimer = setTimeout(() => {
      if (gdKey() === "footprint") gdPreview();
    }, 350);
  }
}

function gdPaintReview(key) {
  const s = GD_STEPS.find((x) => x.key === key);
  $("gd-review-title").textContent = s.review;
  const err = gd.readError[key];
  $("gd-review-help").textContent = err ? "" : "This is what the model read. Correct anything that is wrong; it is checked when you confirm.";
  const banner = $("gd-review-banner");
  banner.hidden = !err;
  const kept = key === "pins" && ((gd.draft.pins || {}).pins || []).length > 0;
  banner.textContent = !err ? "" : kept ? "The model could not read the table again (" + err + "). The pins read before are still here: delete the ones of other packages by hand, or press Read again." : "Nothing could be read here (" + err + "). Type the values in by hand from the datasheet.";
  gdShowCheckError("");
  const body = $("gd-review-body");
  // For pins the warnings are the links above the table.
  const shown = (gd.notes[key] || []).filter((n) => !(key === "pins" && /next to its number/.test(n)));
  $("gd-review-notes").replaceChildren(...shown.map((n) => el("p", { class: "warn-text small", text: "⚠ " + n })));
  if (key === "metadata") body.replaceChildren(gdFormMetadata());
  else if (key === "pins") body.replaceChildren(gdFormPins());
  else if (key === "package") body.replaceChildren(gdFormPackage());
  else {
    body.replaceChildren(gdFormFootprint());
    gdPreview();
  }
}

// Next to the values: the datasheet at the place they were read from, with the
// places you pointed at outlined.
async function gdShowSource(key) {
  if (!gd.pdf || !gd.count) return;
  const d = gd.draft[key] || {};
  const named = key === "pins" ? d.pins && d.pins[0] && d.pins[0].page : d.page;
  const first = gd.regions[key][0];
  gd.page = first ? first.page : named || (key === "metadata" ? 1 : gd.page);
  gdBuildPages();
  gdGoPage(gd.page);
  gdPaintOverlay();
  gdPaintRegions();
}

// Outlines a place on a page (a pin's line, for example) and brings it into view.
function gdHighlight(page, bbox) {
  document.querySelectorAll("#gd-pages .gd-box.src").forEach((b) => b.remove());
  const pg = gdPageEl(page);
  if (!pg) return;
  // A place without a known line (a pin that was not found) is only outlined.
  const b = el("div", { class: "gd-box src" + (bbox ? "" : " whole") });
  const box = bbox || [0, 0, 1, 1];
  // A line is thin: give it room so it can be seen.
  const pad = 0.004;
  b.style.left = Math.max(box[0] - pad, 0) * 100 + "%";
  b.style.top = Math.max(box[1] - pad, 0) * 100 + "%";
  b.style.width = (Math.min(box[2] + pad, 1) - Math.max(box[0] - pad, 0)) * 100 + "%";
  b.style.height = (Math.min(box[3] + pad, 1) - Math.max(box[1] - pad, 0)) * 100 + "%";
  pg.append(b);
  gd.page = page;
  gdWant(page);
  const wrap = gdWrap();
  wrap.scrollTop = pg.offsetTop + pg.offsetHeight * box[1] - wrap.clientHeight / 3;
  gdShowPage();
}

// A box that is the whole page means the place was not found.
function gdIsWhole(b) {
  return !b || (b[0] <= 0.001 && b[1] <= 0.001 && b[2] >= 0.999 && b[3] >= 0.999);
}

function gdFormMetadata() {
  const d = gd.draft.metadata;
  const grid = el("div", { class: "gd-grid" });
  grid.append(gdLabel("Part number", gdText(d, "mpn", "LM358DR"), "The orderable part number of this device."), gdLabel("Manufacturer", gdText(d, "manufacturer", "Texas Instruments")), gdLabel("Description", gdText(d, "description", "Dual operational amplifier"), "One line."));
  return grid;
}

// ---- Pins --------------------------------------------------------------------------------------

function gdFormPins() {
  const d = gd.draft.pins;
  if (!Array.isArray(d.pins)) d.pins = [];
  const wrap = el("div", { class: "gd-pins-wrap" });
  const count = el("p", { class: "muted small gd-pin-count" });
  const table = el("table", { class: "gd-pins" });
  table.append(el("thead", {}, el("tr", {}, ...["Pin", "Name", "Type", "", ""].map((h) => el("th", { text: h })))));
  const body = el("tbody");
  table.append(body);
  const warnings = el("div", { class: "gd-pin-warnings" });
  const rows = new Map(); // pin -> its row
  const focusPin = (p) => {
    const tr = rows.get(p);
    if (tr) {
      tr.scrollIntoView({ block: "center" });
      body.querySelectorAll("tr.on").forEach((r) => r.classList.remove("on"));
      tr.classList.add("on");
    }
    if (p.page) gdHighlight(p.page, gdIsWhole(p.bbox) ? null : p.bbox);
  };
  const paintCount = () => {
    const n = d.pins.length;
    const odd = d.pins.filter((p) => p.grounded === false);
    count.textContent = n + " pin" + (n === 1 ? "" : "s") + (odd.length ? ", " + odd.length + " not found next to their number (marked ⚠)" : "");
    // Each warning is a link: it opens the datasheet at the place to look.
    warnings.replaceChildren(
      ...odd.map((p) => {
        const b = el("button", { type: "button", class: "gd-warn-link" });
        const where = p.page ? (gdIsWhole(p.bbox) ? "page " + p.page : "where its name is written, page " + p.page) : "";
        b.append(el("span", { text: "⚠ pin " + p.number + " " + p.name + ": not found next to its number" }), el("span", { class: "muted small", text: where ? " · show me " + where : "" }));
        b.addEventListener("click", () => focusPin(p));
        return b;
      })
    );
  };
  const row = (p) => {
    const tr = el("tr");
    const num = el("input", { type: "text", spellcheck: "false", class: "gd-pin-num" });
    num.value = p.number == null ? "" : String(p.number);
    num.addEventListener("input", () => {
      p.number = num.value.trim();
      gdTouched();
    });
    const name = el("input", { type: "text", spellcheck: "false" });
    name.value = p.name == null ? "" : String(p.name);
    name.addEventListener("input", () => {
      p.name = name.value.trim();
      gdTouched();
    });
    const type = el("select");
    PIN_TYPES.forEach((t) => {
      const o = el("option", { value: t, text: t });
      if (t === (p.type || "unspecified")) o.selected = true;
      type.append(o);
    });
    type.addEventListener("change", () => {
      p.type = type.value;
      gdTouched();
    });
    const flag = el("span", { class: p.grounded === false ? "gd-flag bad" : "gd-flag", title: p.grounded === false ? "Not found next to its number: click to see where to look" : "", text: p.grounded === false ? "⚠" : "" });
    const del = el("button", { type: "button", class: "ghost small-btn", "aria-label": "Remove this pin", text: "✕" });
    del.addEventListener("click", () => {
      d.pins.splice(d.pins.indexOf(p), 1);
      tr.remove();
      paintCount();
    });
    tr.append(el("td", {}, num), el("td", {}, name), el("td", {}, type), el("td", {}, flag), el("td", {}, del));
    // Clicking a row shows the place in the datasheet where the pin was read.
    rows.set(p, tr);
    const show = () => {
      if (p.page) gdHighlight(p.page, gdIsWhole(p.bbox) ? null : p.bbox);
      body.querySelectorAll("tr.on").forEach((r) => r.classList.remove("on"));
      tr.classList.add("on");
    };
    tr.addEventListener("focusin", show);
    tr.addEventListener("click", show);
    return tr;
  };
  d.pins.forEach((p) => body.append(row(p)));
  paintCount();
  const bar = el("div", { class: "gd-pin-bar" });
  const add = el("button", { type: "button", class: "ghost", text: "+ Add a pin" });
  add.addEventListener("click", () => {
    const last = d.pins[d.pins.length - 1];
    const next = last && /^\d+$/.test(String(last.number)) ? String(Number(last.number) + 1) : "";
    const p = { number: next, name: "", type: "unspecified" };
    d.pins.push(p);
    const tr = row(p);
    body.append(tr);
    tr.querySelector("input").focus();
    paintCount();
  });
  const paste = el("button", { type: "button", class: "ghost", text: "Paste a table…" });
  const box = el("div", { class: "gd-paste" });
  box.hidden = true;
  const area = el("textarea", { rows: "6", spellcheck: "false", placeholder: "One pin per line: number, name and optionally the type, separated by tabs, commas or two spaces." });
  const use = el("button", { type: "button", class: "primary small-btn", text: "Add these pins" });
  use.addEventListener("click", () => {
    for (const line of area.value.split(/\r?\n/)) {
      const cells = line.trim().split(/\t|,|\s{2,}/).map((c) => c.trim()).filter(Boolean);
      if (cells.length < 2) continue;
      const type = PIN_TYPES.includes((cells[2] || "").toLowerCase()) ? cells[2].toLowerCase() : "unspecified";
      const p = { number: cells[0], name: cells[1], type };
      d.pins.push(p);
      body.append(row(p));
    }
    area.value = "";
    box.hidden = true;
    paintCount();
  });
  box.append(area, use);
  paste.addEventListener("click", () => (box.hidden = !box.hidden));
  bar.append(add, paste, count);
  wrap.append(warnings, table, bar, box);
  return wrap;
}

// ---- Package -----------------------------------------------------------------------------------

const GD_FAMILIES = [
  ["soic", "SOIC (two rows, gull-wing)"], ["ssop", "SSOP"], ["tssop", "TSSOP"], ["msop", "MSOP"], ["sot23", "SOT-23"], ["dip", "DIP (through-hole)"],
  ["qfn", "QFN (four sides, no leads)"], ["qfp", "QFP (four sides, gull-wing)"], ["lqfp", "LQFP"], ["tqfp", "TQFP"],
  ["bga", "BGA (balls)"], ["lga", "LGA (lands)"], ["csp", "CSP / WLCSP"], ["custom", "Other (pads from the footprint)"],
];

// The letters of the drawing that each kind of package needs, with what they mean.
function gdPackageFields(family) {
  const quad = ["qfp", "lqfp", "tqfp", "pqfp"].includes(family);
  const grid = ["bga", "lga", "csp", "wlcsp"].includes(family);
  const f = [];
  if (family === "custom") {
    f.push(["package_name", "text", "Package name", "As the drawing calls it, for example SOT-223."], ["D", "range", "Body length (D)", "Along y."], ["E1", "range", "Body width (E1)", "Along x."]);
  } else if (grid) {
    f.push(["rows", "int", "Rows", "The letters A, B, C …"], ["cols", "int", "Columns", "The numbers 1, 2, 3 …"], ["D", "range", "Body size D", "Along x."], ["E", "range", "Body size E", "Along y."], ["b", "range", "Ball diameter (b)", ""], ["e", "num", "Pitch (e)", "Distance between ball centres."]);
  } else if (quad || family === "qfn") {
    f.push(["pin_count", "int", "Pins in all", "Or fill in the two below."], ["pins_x", "int", "Pins on top and bottom", "Each of those sides."], ["pins_y", "int", "Pins on left and right", "Each of those sides."]);
    if (quad) f.push(["D", "range", "Size over the lead tips, x (D)", ""], ["E", "range", "Size over the lead tips, y (E)", ""], ["D1", "range", "Body size x (D1)", ""], ["E1", "range", "Body size y (E1)", ""]);
    else f.push(["D", "range", "Body size x (D)", ""], ["E", "range", "Body size y (E)", ""], ["D2", "num", "Exposed pad width (D2)", "If there is one."], ["E2", "num", "Exposed pad height (E2)", ""]);
    f.push(["L", "range", "Lead length (L)", ""], ["b", "range", "Lead width (b)", ""], ["e", "num", "Pitch (e)", ""]);
  } else if (family === "dip") {
    f.push(["pin_count", "int", "Pins", ""], ["D", "range", "Body length (D)", ""], ["E1", "range", "Body width (E1)", ""], ["row_spacing", "num", "Distance between the rows", "Centre to centre."], ["e", "num", "Pitch (e)", ""]);
  } else {
    f.push(["pin_count", "int", "Pins", ""], ["D", "range", "Body length (D)", "Along the pin rows."], ["E1", "range", "Body width (E1)", "Across the rows, without leads."], ["E", "range", "Lead span (E)", "Over the lead tips."], ["L", "range", "Lead length (L)", ""], ["b", "range", "Lead width (b)", ""], ["e", "num", "Pitch (e)", ""]);
  }
  f.push(["A", "num", "Height (A)", "The largest."], ["A1", "num", "Standoff (A1)", "Body to board; for the 3D model."], ["A2", "num", "Body thickness (A2)", "For the 3D model."], ["c", "num", "Lead thickness (c)", "For the 3D model."]);
  if (family !== "custom") f.unshift(["package_name", "text", "Name on the drawing", "Optional."]);
  return f;
}

function gdFormPackage() {
  const d = gd.draft.package;
  if (!d.family) d.family = "soic";
  const wrap = el("div", { class: "gd-package" });
  const fam = el("select");
  const norm = String(d.family).toLowerCase().replace(/[-\s]/g, "");
  if (!GD_FAMILIES.some(([v]) => v === norm)) fam.append(el("option", { value: norm, text: norm }));
  GD_FAMILIES.forEach(([v, label]) => fam.append(el("option", { value: v, text: label })));
  fam.value = norm;
  d.family = norm;
  fam.addEventListener("change", () => {
    d.family = fam.value;
    gdPaintReview("package");
  });
  wrap.append(gdLabel("Kind of package", fam, "The shape decides which letters of the drawing are needed."));
  const grid = el("div", { class: "gd-grid" });
  for (const [key, kind, label, help] of gdPackageFields(norm)) {
    const input = kind === "range" ? gdRange(d, key) : kind === "text" ? gdText(d, key) : gdNum(d, key, "", kind === "int");
    grid.append(gdLabel(label, input, help));
  }
  wrap.append(grid);
  wrap.append(el("p", { class: "muted small", text: "All sizes in millimetres. The letters follow JEDEC, which most datasheets use. A range is the smallest and the largest value of the drawing; type one value in both if only one is given." }));
  return wrap;
}

// ---- Footprint ---------------------------------------------------------------------------------

function gdFormFootprint() {
  const d = gd.draft.footprint;
  const wrap = el("div", { class: "gd-footprint" });
  const pkg = gd.values.package || gd.draft.package || {};
  const custom = String(pkg.family || "").toLowerCase() === "custom";
  if (custom) gd.useFootprint = true;
  const choice = el("div", { class: "gd-choice" });
  const radio = (val, label, help, disabled) => {
    const input = el("input", { type: "radio", name: "gd-fp" });
    input.checked = gd.useFootprint === val;
    input.disabled = !!disabled;
    input.addEventListener("change", () => {
      gd.useFootprint = val;
      gdPaintReview("footprint");
    });
    return el("label", { class: "check big" }, input, el("span", {}, el("strong", { text: label }), el("span", { class: "help", text: " " + help })));
  };
  choice.append(radio(true, "Use the recommended footprint of the datasheet", "The pad sizes and the distance between the rows come from it."), radio(false, "Calculate the pads from the package", "By the IPC-7351B formulas. Use this when the datasheet has no recommended footprint.", custom));
  wrap.append(choice);
  if (gd.useFootprint) {
    const grid = el("div", { class: "gd-grid" });
    grid.append(
      gdLabel("Pad length", gdNum(d, "pad_length"), "Along the lead, pointing away from the body."),
      gdLabel("Pad width", gdNum(d, "pad_width"), "Across the lead."),
      gdLabel("Pitch", gdNum(d, "pitch"), "Between pad centres."),
      gdLabel("Row distance x (centres)", gdNum(d, "center_x"), "Between the centres of the left and right rows."),
      gdLabel("Row distance y (centres)", gdNum(d, "center_y"), "Between the top and bottom rows."),
      gdLabel("Row distance x (outer edges)", gdNum(d, "outer_x"), "Use this if the drawing gives outer edges."),
      gdLabel("Row distance y (outer edges)", gdNum(d, "outer_y"), ""),
      gdLabel("Land diameter", gdNum(d, "pad_diameter"), "Ball grids: the round pads.")
    );
    const ep = d.exposed_pad && typeof d.exposed_pad === "object" ? d.exposed_pad : (d.exposed_pad = {});
    grid.append(gdLabel("Pad under the body, width", gdNum(ep, "width"), "If there is one."), gdLabel("Pad under the body, height", gdNum(ep, "height"), ""));
    wrap.append(grid);
    if (Array.isArray(d.groups)) {
      const area = el("textarea", { rows: "6", spellcheck: "false", class: "gd-groups" });
      area.value = JSON.stringify(d.groups, null, 1);
      area.addEventListener("input", () => {
        try {
          d.groups = JSON.parse(area.value);
          area.classList.remove("invalid");
          gdTouched();
        } catch (e) {
          area.classList.add("invalid");
        }
      });
      wrap.append(gdLabel("Pads that are not rows or a grid (advanced)", area, "Each run: first_number, count, x, y, dx, dy, size_x, size_y, shape. From the centre; x right, y down."));
    }
  }
  wrap.append(el("div", { id: "gd-fp-preview", class: "gd-fp-preview" }, el("p", { class: "muted small", text: "The footprint that will be made:" }), el("div", { id: "gd-fp-svg" }), el("div", { id: "gd-fp-numbers", class: "gd-fp-numbers" }), el("p", { id: "gd-fp-info", class: "muted small" })));
  return wrap;
}

// The numbers of the footprint, measured on the pads that will be made, so they can be
// compared with the datasheet. [label, value, optional warning].
function gdFootprintNumbers(p, pkg) {
  const mm = (v) => String(Math.round(v * 1000) / 1000);
  const rows = [];
  if (!p.pads.length) return rows;
  const area = (a) => a.size_x * a.size_y;
  const sorted = p.pads.map(area).sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  const centre = p.pads.filter((a) => area(a) > median * 6); // an exposed (centre) pad
  const regular = p.pads.filter((a) => !centre.includes(a));
  rows.push(["Body", mm(p.body_width) + " × " + mm(p.body_length) + " mm"]);
  rows.push(["Pads", p.pads.length + (centre.length ? " (" + regular.length + " around the body, " + centre.length + " in the centre)" : "")]);

  // Pad sizes, long side first, so the pads of the four sides count as one kind.
  const kinds = new Map();
  for (const a of regular) {
    const w = Math.max(a.size_x, a.size_y), h = Math.min(a.size_x, a.size_y);
    const k = mm(w) + " × " + mm(h) + " mm" + (a.shape === "circle" ? " (round)" : "");
    kinds.set(k, (kinds.get(k) || 0) + 1);
  }
  const kindText = [...kinds].map(([k, n]) => (kinds.size > 1 ? k + " ×" + n : k));
  rows.push([kinds.size > 1 ? "Pad sizes" : "Pad size", kindText.join(" · ")]);
  for (const a of centre) rows.push(["Centre pad " + a.number, mm(a.size_x) + " × " + mm(a.size_y) + " mm"]);

  // The pitch: the most common distance from a pad to the pad next to it.
  const near = new Map();
  for (const a of regular) {
    let best = Infinity;
    for (const b of regular) if (a !== b && Math.hypot(a.x - b.x, a.y - b.y) > 1e-6) best = Math.min(best, Math.hypot(a.x - b.x, a.y - b.y));
    if (best < Infinity) near.set(mm(best), (near.get(mm(best)) || 0) + 1);
  }
  let pitch = null, most = 0;
  for (const [k, n] of near) if (n > most) { pitch = Number(k); most = n; }
  if (pitch != null) {
    const wanted = pkg && Number(pkg.pitch) > 0 ? Number(pkg.pitch) : null;
    rows.push(["Pitch", mm(pitch) + " mm", wanted && Math.abs(wanted - pitch) > 0.005 ? "the package says " + mm(wanted) + " mm" : ""]);
  }

  // The rows: how far apart the pad centres are, and how far the pads reach.
  const xs = regular.map((a) => a.x), ys = regular.map((a) => a.y);
  const cx = Math.max(...xs) - Math.min(...xs), cy = Math.max(...ys) - Math.min(...ys);
  const distinctX = new Set(xs.map(mm)).size, distinctY = new Set(ys.map(mm)).size;
  if (distinctX <= 2 && distinctY > 2) rows.push(["Row to row", mm(cx) + " mm", "pad centre to pad centre"]);
  else if (distinctY <= 2 && distinctX > 2) rows.push(["Row to row", mm(cy) + " mm", "pad centre to pad centre"]);
  else rows.push(["Pad centres", mm(cx) + " × " + mm(cy) + " mm", "opposite sides, centre to centre"]);
  const ex = Math.max(...p.pads.map((a) => a.x + a.size_x / 2)) - Math.min(...p.pads.map((a) => a.x - a.size_x / 2));
  const ey = Math.max(...p.pads.map((a) => a.y + a.size_y / 2)) - Math.min(...p.pads.map((a) => a.y - a.size_y / 2));
  rows.push(["Land pattern", mm(ex) + " × " + mm(ey) + " mm", "outer edge to outer edge of the pads"]);
  return rows;
}

function gdPaintNumbers(p, pkg) {
  const box = $("gd-fp-numbers");
  if (!box) return;
  const list = el("dl", { class: "fp-numbers" });
  for (const [label, value, note] of gdFootprintNumbers(p, pkg)) {
    list.append(el("dt", { text: label }), el("dd", {}, el("span", { text: value }), note ? el("span", { class: "muted small", text: " · " + note }) : ""));
  }
  box.replaceChildren(list);
}

// The footprint as it will be made, so a wrong pad is seen before the part exists.
async function gdPreview() {
  const svgBox = $("gd-fp-svg");
  if (!svgBox) return;
  const info = $("gd-fp-info");
  const pkg = gd.values.package || gd.draft.package;
  const fp = gd.useFootprint ? cleanFootprint(gd.draft.footprint) : null;
  try {
    const p = await invoke("preview_footprint", { package: pkg, footprint: fp && Object.keys(fp).length ? fp : null });
    svgBox.replaceChildren(gdDrawPads(p));
    gdPaintNumbers(p, pkg);
    info.className = "muted small";
    info.textContent = p.name + " · " + p.pads.length + " pads" + (p.notes.length ? " · " + p.notes.join(" · ") : "");
  } catch (e) {
    svgBox.replaceChildren();
    if ($("gd-fp-numbers")) $("gd-fp-numbers").replaceChildren();
    info.className = "small err-text";
    info.textContent = String(e);
  }
}

function cleanFootprint(d) {
  const out = {};
  for (const [k, v] of Object.entries(d || {})) {
    if (k === "exposed_pad") {
      if (v && v.width > 0 && v.height > 0) out.exposed_pad = v;
    } else if (k === "groups") {
      if (Array.isArray(v) && v.length) out.groups = v;
    } else if (v != null && v !== "") out[k] = v;
  }
  return out;
}

const SVGNS = "http://www.w3.org/2000/svg";
function svgEl(tag, attrs) {
  const e = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs || {})) e.setAttribute(k, String(v));
  return e;
}

function gdDrawPads(p) {
  let x0 = -p.body_width / 2, x1 = p.body_width / 2, y0 = -p.body_length / 2, y1 = p.body_length / 2;
  for (const pad of p.pads) {
    x0 = Math.min(x0, pad.x - pad.size_x / 2);
    x1 = Math.max(x1, pad.x + pad.size_x / 2);
    y0 = Math.min(y0, pad.y - pad.size_y / 2);
    y1 = Math.max(y1, pad.y + pad.size_y / 2);
  }
  const m = Math.max(x1 - x0, y1 - y0) * 0.06;
  const svg = svgEl("svg", { viewBox: [x0 - m, y0 - m, x1 - x0 + 2 * m, y1 - y0 + 2 * m].join(" "), class: "gd-fp-svg", role: "img", "aria-label": "Footprint " + p.name });
  svg.append(svgEl("rect", { x: -p.body_width / 2, y: -p.body_length / 2, width: p.body_width, height: p.body_length, class: "fp-body" }));
  const unit = Math.max(x1 - x0, y1 - y0) / 60;
  for (const pad of p.pads) {
    const cls = "fp-pad" + (pad.number === "1" || pad.number === "A1" ? " first" : "");
    if (pad.shape === "circle" || pad.shape === "oval") {
      svg.append(svgEl("ellipse", { cx: pad.x, cy: pad.y, rx: pad.size_x / 2, ry: pad.size_y / 2, class: cls }));
    } else {
      svg.append(svgEl("rect", { x: pad.x - pad.size_x / 2, y: pad.y - pad.size_y / 2, width: pad.size_x, height: pad.size_y, rx: Math.min(pad.size_x, pad.size_y) * 0.2, class: cls }));
    }
  }
  if (p.pads.length <= 40) {
    for (const pad of p.pads) {
      const t = svgEl("text", { x: pad.x, y: pad.y, class: "fp-num", "font-size": Math.min(unit * 2.4, Math.min(pad.size_x, pad.size_y) * 0.9 + unit), "text-anchor": "middle", "dominant-baseline": "central" });
      t.textContent = pad.number;
      svg.append(t);
    }
  }
  return svg;
}

// ---- The last step ------------------------------------------------------------------------------------

function gdFinalInput(dry) {
  return {
    pdf: gd.pdf,
    out: $("ds-out").value.trim(),
    hint: $("ds-hint").value,
    metadata: gd.values.metadata,
    pins: gd.values.pins,
    package: gd.values.package,
    footprint: gd.useFootprint && gd.values.footprint ? cleanFootprint(gd.values.footprint) : null,
    pin_regions: gd.regions.pins,
    dry_run: dry,
  };
}

async function gdEnterFinal() {
  gd.ready = null;
  if (gd.mode !== "steps") return gdPaintFinal();
  if (["metadata", "pins", "package"].some((k) => !gd.confirmed[k])) return gdPaintFinal();
  $("gd-ready").replaceChildren(el("p", { class: "muted small", text: "Checking everything together…" }));
  try {
    gd.ready = await invoke("build_confirmed", { input: gdFinalInput(true) });
  } catch (e) {
    gd.ready = { error: String(e), failures: [], notes: [] };
  }
  gdPaintFinal();
}

function gdPaintFinal() {
  const rows = $("gd-summary");
  const ready = $("gd-ready");
  if (gd.mode === "auto") {
    rows.replaceChildren(el("li", { class: "gd-sum-row" }, el("strong", { text: "Automatic" }), el("span", { text: "The program searches the whole datasheet for everything and the model reads what it finds. You review the part afterwards." }), el("span"), el("span")));
    ready.replaceChildren();
    $("gd-total").textContent = "";
    $("ds-run").disabled = false;
    return;
  }
  const m = gd.values.metadata;
  const pins = gd.values.pins && gd.values.pins.pins;
  const line = (key, label, text, step, ok) => {
    const li = el("li", { class: "gd-sum-row" });
    const edit = el("button", { type: "button", class: "link", text: gd.confirmed[key] ? "change" : "do this step" });
    edit.addEventListener("click", () => gdGo(step));
    li.append(el("strong", { text: label }), el("span", { class: ok ? "ok-text" : "err-text", text }), el("span"), edit);
    return li;
  };
  rows.replaceChildren(
    line("metadata", "Part number", m ? [m.mpn, m.manufacturer].join(" · ") : "not confirmed yet", 2, !!m),
    line("pins", "Pins", pins ? pins.length + " pins" : "not confirmed yet", 3, !!pins),
    line("package", "Package", gd.values.package ? String(gd.values.package.family).toUpperCase() + (gd.values.package.package_name ? " " + gd.values.package.package_name : "") : "not confirmed yet", 4, !!gd.values.package),
    line("footprint", "Footprint", gd.confirmed.footprint ? (gd.useFootprint && gd.values.footprint ? "from the recommended footprint of the datasheet" : "calculated from the package") : "not confirmed yet (it will be calculated)", 5, true)
  );
  const parts = [];
  const r = gd.ready;
  let canBuild = false;
  if (!r) {
    parts.push(el("p", { class: "err-text", text: "Confirm the part number, the pins and the package first." }));
  } else if (r.error) {
    parts.push(el("p", { class: "err-text", text: "These values do not fit together: " + r.error }));
    const mismatch = /pin table has (\d+) pins but the package has (\d+)/.exec(r.error);
    if (mismatch && Number(mismatch[1]) > Number(mismatch[2])) parts.push(gdNarrowBox(Number(mismatch[1]), Number(mismatch[2])));
    else parts.push(el("p", { class: "muted small", text: "Go back to the step that is wrong (click it above) and correct it." }));
  } else {
    if (r.failures.length) {
      parts.push(el("p", { class: "err-text", text: "Problems found before building:" }), el("ul", { class: "err-list" }, ...r.failures.map((f) => el("li", { text: f }))), el("p", { class: "muted small", text: "Go back to the step that is wrong and correct it." }));
    } else {
      parts.push(el("p", { class: "ok-text", text: "Everything fits together and passes the checks. Nothing more is asked of a model." }));
      canBuild = true;
    }
    if (r.notes.length) parts.push(el("details", {}, el("summary", { text: r.notes.length + " thing" + (r.notes.length === 1 ? "" : "s") + " to look at in review" }), el("ul", {}, ...r.notes.map((n) => el("li", { class: "small muted", text: n })))));
  }
  ready.replaceChildren(...parts);
  $("gd-total").textContent = "";
  $("ds-run").disabled = !canBuild;
}

// A datasheet for several packages has one table with a column of pin numbers for each of
// them. The table was read for all of them; the package says which one is wanted.
function gdPackageLabel(want) {
  const family = String((gd.values.package || {}).family || "").toUpperCase();
  return family && family !== "CUSTOM" ? family + want : want + "-pin package";
}

function gdNarrowBox(have, want) {
  const label = gdPackageLabel(want);
  const box = el("div", { class: "gd-narrow" });
  box.append(
    el("p", { text: "The pin table has " + have + " pins, but this package (" + label + ") has " + want + ". Datasheets for several packages list them in one table, with a column of pin numbers for each package." }),
    el("p", { class: "muted small", text: "The model can read the table again and keep only the pins that have a number in the " + label + " column. You check the result before it is used." })
  );
  const go = el("button", { type: "button", class: "primary", text: "Keep only the " + want + " pins of " + label });
  go.addEventListener("click", () => gdNarrowPins(want));
  box.append(go);
  return box;
}

async function gdNarrowPins(want) {
  const label = gdPackageLabel(want);
  const name = (gd.values.package || {}).package_name;
  const hint =
    "This datasheet lists several packages in one pin table, with a column of pin numbers for each package (the header names the columns, in order). Read only the " +
    label + " package" + (name ? " (the drawing calls it " + name + ")" : "") + ", which has " + want +
    " pins: use the pin numbers of its column, leave out the rows where that column is empty or a dash, and give every pin once. The answer must have exactly " + want + " pins";
  gd.confirmed.pins = false;
  gd.ack.pins = false;
  gd.step = GD_STEPS.findIndex((s) => s.key === "pins") + 1;
  await gdRead("pins", gd.regions.pins.length > 0, { hint, keep: true });
  gd.returnToFinal = true;
  const have = ((gd.draft.pins || {}).pins || []).length;
  if (have !== want && !gd.readError.pins) {
    gd.notes.pins = ["The model gave " + have + " pins, but the package has " + want + ". Check the list, or draw a box around the pin table again."].concat(gd.notes.pins || []);
  }
  if (gdKey() === "pins") gdShow(gd.step);
}

// The part, from the confirmed values. Called by the dialog's Build button.
async function gdBuildConfirmed() {
  $("ds-error").hidden = true;
  $("ds-result").hidden = true;
  $("ds-run").disabled = true;
  $("ds-cancel").disabled = true;
  $("ds-run").textContent = "Building…";
  try {
    const outcome = await invoke("build_confirmed", { input: gdFinalInput(false) });
    showBuildResult(outcome, gd.pdf);
  } catch (e) {
    $("ds-error").textContent = String(e);
    $("ds-error").hidden = false;
  } finally {
    $("ds-run").textContent = "Build part";
    $("ds-run").disabled = false;
    $("ds-cancel").disabled = false;
  }
}

// ---- The page viewer: the whole document, scrolling ---------------------------------------------

const GD_WIDTH = 900;
const GD_KEEP = 14; // pages kept drawn at the same time
let gdObserver = null;
let gdQueue = [];
let gdBusy = 0;
const gdDrawn = new Set();

function gdWrap() {
  return $("gd-wrap");
}

function gdPageEl(p) {
  return $("gd-pages").children[p - 1];
}

function gdBuildPages() {
  const box = $("gd-pages");
  if (box.dataset.pdf === gd.pdf && box.children.length === gd.count) return;
  box.dataset.pdf = gd.pdf;
  if (gdObserver) gdObserver.disconnect();
  gdDrawn.clear();
  gdQueue = [];
  box.replaceChildren();
  gd.ratio = 1.414;
  for (let p = 1; p <= gd.count; p++) {
    const d = el("div", { class: "gd-pg", "data-page": String(p) });
    d.style.aspectRatio = "1 / " + gd.ratio;
    d.append(el("canvas"), el("span", { class: "gd-pg-no", text: "page " + p }));
    box.append(d);
  }
  gdObserver = new IntersectionObserver(
    (entries) => {
      for (const e of entries) if (e.isIntersecting) gdWant(Number(e.target.dataset.page));
    },
    { root: gdWrap(), rootMargin: "900px 0px" }
  );
  [...box.children].forEach((c) => gdObserver.observe(c));
}

function gdWant(p) {
  if (gdDrawn.has(p) || gdQueue.includes(p)) return;
  gdQueue.push(p);
  gdPump();
}

function gdPump() {
  while (gdBusy < 2 && gdQueue.length) {
    gdQueue.sort((x, y) => Math.abs(x - gd.page) - Math.abs(y - gd.page));
    const p = gdQueue.shift();
    gdBusy++;
    gdDraw(p).finally(() => {
      gdBusy--;
      gdPump();
    });
  }
}

async function gdDraw(p) {
  const pdf = gd.pdf;
  try {
    const image = await invoke("page_image", { pdf, page: p, width: GD_WIDTH });
    const el1 = gdPageEl(p);
    if (pdf !== gd.pdf || !el1) return;
    const canvas = el1.querySelector("canvas");
    canvas.width = image.width;
    canvas.height = image.height;
    canvas.getContext("2d").putImageData(new ImageData(decodeBase64(image.rgba_base64), image.width, image.height), 0, 0);
    el1.style.aspectRatio = image.width + " / " + image.height;
    el1.classList.add("ready");
    gdDrawn.add(p);
    if (gdDrawn.size > GD_KEEP) {
      const far = [...gdDrawn].sort((x, y) => Math.abs(y - gd.page) - Math.abs(x - gd.page))[0];
      const old = gdPageEl(far);
      if (old && far !== p) {
        const c = old.querySelector("canvas");
        c.width = 0;
        c.height = 0;
        old.classList.remove("ready");
        gdDrawn.delete(far);
      }
    }
  } catch (e) {
    gdError(String(e));
  }
}

async function gdLoadPage() {
  if (!gd.pdf) return;
  gdBuildPages();
  gdGoPage(gd.page);
  gdPaintOverlay();
}

function gdGoPage(page) {
  gd.page = Math.min(Math.max(Number(page) || 1, 1), gd.count || 1);
  const e = gdPageEl(gd.page);
  if (e) gdWrap().scrollTop = e.offsetTop - 6;
  gdWant(gd.page);
  gdShowPage();
}

function gdShowPage() {
  $("gd-page").value = gd.page;
  $("gd-page").max = gd.count;
  $("gd-count").textContent = "of " + gd.count;
  gdPaintSuggestions();
}

let gdScrollTimer = 0;
$("gd-wrap").addEventListener("scroll", () => {
  if (gdScrollTimer) return;
  gdScrollTimer = setTimeout(() => {
    gdScrollTimer = 0;
    const r = gdWrap().getBoundingClientRect();
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    const pg = hit && hit.closest && hit.closest(".gd-pg");
    if (pg && Number(pg.dataset.page) !== gd.page) {
      gd.page = Number(pg.dataset.page);
      gdShowPage();
    }
  }, 60);
});

function gdPaintOverlay() {
  document.querySelectorAll("#gd-pages .gd-box:not(.draft)").forEach((b) => b.remove());
  const key = gdKey();
  if (!key) return;
  for (const r of gd.regions[key]) {
    const pg = gdPageEl(r.page);
    if (!pg) continue;
    const b = el("div", { class: "gd-box" });
    b.style.left = r.bbox[0] * 100 + "%";
    b.style.top = r.bbox[1] * 100 + "%";
    b.style.width = (r.bbox[2] - r.bbox[0]) * 100 + "%";
    b.style.height = (r.bbox[3] - r.bbox[1]) * 100 + "%";
    pg.append(b);
  }
}

function gdPaintSuggestions() {
  const box = $("gd-suggest");
  const key = gdKey();
  const list = (gd.suggestions && key && gd.suggestions[key]) || [];
  box.replaceChildren();
  if (!list.length) return;
  box.append(el("span", { class: "muted small", text: "The program would look at:" }));
  list.forEach((s, i) => {
    const b = el("button", { type: "button", class: "chip" + (s.page === gd.page ? " on" : ""), text: "page " + s.page + (i === 0 ? " (best match)" : "") });
    b.addEventListener("click", () => gdGoPage(s.page));
    box.append(b);
  });
}

// Runs of boxes on following pages with the same columns are shown as one.
function gdGroups(regions) {
  const groups = [];
  for (const r of regions) {
    const last = groups[groups.length - 1];
    const prev = last && last[last.length - 1];
    if (prev && r.page === prev.page + 1 && Math.abs(r.bbox[0] - prev.bbox[0]) < 1e-6 && Math.abs(r.bbox[2] - prev.bbox[2]) < 1e-6) last.push(r);
    else groups.push([r]);
  }
  return groups;
}

function gdPaintRegions() {
  const key = gdKey();
  const box = $("gd-regions");
  const regions = gd.regions[key] || [];
  $("gd-extend").disabled = !regions.length;
  if (!regions.length) {
    box.replaceChildren(el("span", { class: "muted small", text: "Nothing chosen yet. Drag on a page to draw a box, or use the whole page." }));
    return;
  }
  box.replaceChildren(
    ...gdGroups(regions).map((group) => {
      const first = group[0];
      const last = group[group.length - 1];
      const whole = group.length === 1 && first.bbox[0] <= 0.001 && first.bbox[1] <= 0.001 && first.bbox[2] >= 0.999 && first.bbox[3] >= 0.999;
      const label = group.length > 1 ? "pages " + first.page + "–" + last.page : "page " + first.page + (whole ? ", whole page" : ", box");
      const chip = el("span", { class: "chip chosen" });
      const go = el("button", { type: "button", class: "chip-go", text: label });
      go.addEventListener("click", () => gdGoPage(first.page));
      const rm = el("button", { type: "button", class: "chip-x", "aria-label": "Remove", text: "✕" });
      rm.addEventListener("click", () => {
        regions.splice(regions.indexOf(first), group.length);
        gdChanged();
      });
      chip.append(go, rm);
      return chip;
    })
  );
}

function gdPaintSize() {
  const key = gdKey();
  const out = $("gd-size");
  if (!key) return;
  out.className = "muted small";
  out.textContent = gd.regions[key].length ? "The model will read only the text inside the places you chose." : "Nothing chosen: choose a place, or let the program search the datasheet for this.";
}

// A change in the places.
function gdChanged() {
  gdPaintStepper();
  gdPaintRegions();
  gdPaintSize();
  gdPaintOverlay();
}

async function gdShowText(region) {
  const det = $("gd-text");
  try {
    const table = await invoke("region_table", { pdf: gd.pdf, page: region.page, region: region.bbox });
    $("gd-text-pre").textContent = table.rows.map((r) => r.join("  |  ")).join("\n");
    det.hidden = false;
  } catch (e) {
    $("gd-text-pre").textContent = String(e);
    det.hidden = false;
  }
}

// ---- Drawing a box, and continuing a table over several pages ------------------------------------

function gdSetExtending(on) {
  gd.extending = on;
  $("gd-wrap").classList.toggle("extending", on);
  $("gd-hint").hidden = !on;
  $("gd-extend").textContent = on ? "Cancel" : "Continue the table to…";
  if (on) $("gd-hint").textContent = "Scroll to the page where the table ends and click on its last row. Every page in between is added with the same columns.";
}

function gdExtendTo(page, y) {
  const list = gd.regions[gdKey()];
  const last = list[list.length - 1];
  if (!last) return;
  if (page < last.page || (page === last.page && y <= last.bbox[1] + 0.01)) {
    gdError("Click further down than the box: on a later page, or lower on the same page.");
    return;
  }
  gdError("");
  const [x0, , x1] = last.bbox;
  if (page === last.page) {
    last.bbox[3] = Math.max(y, last.bbox[3]);
  } else {
    last.bbox[3] = 1;
    for (let p = last.page + 1; p < page; p++) list.push({ page: p, bbox: [x0, 0, x1, 1] });
    list.push({ page, bbox: [x0, 0, x1, Math.min(Math.max(y, 0.02), 1)] });
  }
  gdSetExtending(false);
  gdChanged();
}

(() => {
  const wrap = $("gd-wrap");
  const frac = (pg, ev) => {
    const r = pg.getBoundingClientRect();
    return [Math.min(Math.max((ev.clientX - r.left) / r.width, 0), 1), Math.min(Math.max((ev.clientY - r.top) / r.height, 0), 1)];
  };
  const place = (box, a, b) => {
    const l = Math.min(a[0], b[0]), r = Math.max(a[0], b[0]), t = Math.min(a[1], b[1]), bo = Math.max(a[1], b[1]);
    box.style.left = l * 100 + "%";
    box.style.top = t * 100 + "%";
    box.style.width = (r - l) * 100 + "%";
    box.style.height = (bo - t) * 100 + "%";
    return [l, t, r, bo];
  };
  wrap.addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0 || !gdKey() || gdPhase(gdKey()) !== "point") return;
    const pg = ev.target.closest(".gd-pg");
    if (!pg) return;
    const draft = el("div", { class: "gd-box draft" });
    pg.append(draft);
    gd.drag = { pg, draft, page: Number(pg.dataset.page), start: frac(pg, ev) };
    place(draft, gd.drag.start, gd.drag.start);
    wrap.setPointerCapture(ev.pointerId);
  });
  wrap.addEventListener("pointermove", (ev) => {
    const d = gd.drag;
    if (d) place(d.draft, d.start, frac(d.pg, ev));
  });
  wrap.addEventListener("pointerup", (ev) => {
    const d = gd.drag;
    if (!d) return;
    gd.drag = null;
    const end = frac(d.pg, ev);
    const bbox = place(d.draft, d.start, end);
    d.draft.remove();
    if (bbox[2] - bbox[0] < 0.01 || bbox[3] - bbox[1] < 0.01) {
      if (gd.extending) gdExtendTo(d.page, end[1]);
      return;
    }
    const region = { page: d.page, bbox };
    gd.regions[gdKey()].push(region);
    gdChanged();
    gdShowText(region);
  });
  $("gd-whole").addEventListener("click", () => {
    gd.regions[gdKey()].push({ page: gd.page, bbox: [0, 0, 1, 1] });
    gdChanged();
  });
  $("gd-extend").addEventListener("click", () => gdSetExtending(!gd.extending));
  $("gd-prev").addEventListener("click", () => gdGoPage(gd.page - 1));
  $("gd-nextpage").addEventListener("click", () => gdGoPage(gd.page + 1));
  $("gd-page").addEventListener("change", () => gdGoPage($("gd-page").value));
  document.addEventListener("keydown", (ev) => {
    if (ev.key === "Escape" && gd.extending) {
      ev.stopPropagation();
      gdSetExtending(false);
    }
  });
})();

// ---- The buttons ------------------------------------------------------------------------------------

$("gd-next").addEventListener("click", () => {
  const key = gdKey();
  if (!key) return gdGo(2);
  if (gdPhase(key) === "point") return gdRead(key, true);
  if (gdPhase(key) === "review") return gdConfirm(key);
});
$("gd-back").addEventListener("click", () => {
  const key = gdKey();
  // From the review of a step, back is its places; otherwise the step before.
  if (key && gdPhase(key) === "review") {
    gd.phase[key] = "point";
    return gdGo(gd.step);
  }
  return gdGo(Math.max(gd.step - 1, 1));
});
$("gd-skip").addEventListener("click", () => gdRead(gdKey(), false));
$("gd-again").addEventListener("click", () => {
  gd.phase[gdKey()] = "point";
  gdGo(gd.step);
});
$("gd-reread").addEventListener("click", () => gdRead(gdKey(), gd.regions[gdKey()].length > 0));
$("gd-nofp").addEventListener("click", () => {
  gd.useFootprint = false;
  gd.draft.footprint = {};
  gd.readError.footprint = "";
  gd.notes.footprint = [];
  gd.phase.footprint = "review";
  gdShow(gd.step);
});
$("gd-auto").addEventListener("click", async () => {
  // Skip the guide: the program searches everything.
  gd.mode = "auto";
  await gdGo(GD_LAST);
});
