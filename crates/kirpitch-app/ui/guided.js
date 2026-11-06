// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// The guided way to read a datasheet. Instead of letting the program search the
// whole document, you say where things are: the page with the part number, the
// pin table, the package drawing, each as a page or as a rectangle drawn on a
// page. The models then read only that text, which costs fewer tokens and does
// not depend on the program finding the right pages. A step you do not point at
// is searched for by the program as before.

const GD_STEPS = [
  { id: "file", label: "Datasheet" },
  { id: "metadata", label: "Part number", title: "Where are the part number and the description?", help: "Usually the first page: the title and the first lines of text. Draw a box around them, or use the whole page.", key: "metadata" },
  { id: "pins", label: "Pins", title: "Where is the pin table?", help: "Draw a box around the table that lists every pin: number, name and function. If it continues on the next page, go there and draw another box. Leave out long tables of alternate functions that are not needed.", key: "pins" },
  { id: "package", label: "Package", title: "Where is the package outline?", help: "Draw a box around the package outline drawing and its dimension table: the body size, the height and, when the drawing has them, the standoff, the thickness of the body and of a lead, and the lead size. These make the package and its 3D model. Include the line that names the package.", key: "package" },
  { id: "footprint", label: "Footprint", title: "Where is the recommended footprint?", help: "Most datasheets end with a recommended footprint (also called land pattern or PCB layout): the pads with their sizes and distances. Draw a box around it and its dimensions. The pads are then taken from it instead of being calculated. Skip this step if the datasheet has none.", key: "footprint" },
  { id: "build", label: "Build" },
];
const GD_LAST = GD_STEPS.length;

function gdIsPick(step) {
  return !!GD_STEPS[step - 1].key;
}

function gdEmpty() {
  return { metadata: [], pins: [], package: [], footprint: [] };
}

const gd = {
  step: 1,
  pdf: "",
  count: 0,
  page: 1,
  regions: gdEmpty(),
  suggestions: null,
  preview: null,
  visited: new Set([1]),
  drag: null,
  timer: null,
  loading: false,
};

function gdKey() {
  return GD_STEPS[gd.step - 1].key;
}

function gdGuide() {
  const g = gd.regions;
  return { metadata: g.metadata, pins: g.pins, package: g.package, footprint: g.footprint };
}

// What the build gets: null when nothing was pointed at (the program searches).
function guidedInput() {
  const g = gd.regions;
  return g.metadata.length || g.pins.length || g.package.length || g.footprint.length ? gdGuide() : null;
}

function gdReset() {
  const pdf = $("ds-pdf").value.trim();
  if (pdf !== gd.pdf) {
    gd.regions = gdEmpty();
    gd.suggestions = null;
    gd.preview = null;
  }
  gd.pdf = pdf;
  gd.visited = new Set([1]);
  gdShow(1);
}

// ---- Moving between steps ------------------------------------------------------------------

function gdError(text) {
  $("ds-error").textContent = text || "";
  $("ds-error").hidden = !text;
}

async function gdGo(step) {
  gdError("");
  if (step > 1 && gd.step === 1) {
    const pdf = $("ds-pdf").value.trim();
    if (!pdf) return gdError("Choose the datasheet PDF first.");
    if (!$("ds-out").value.trim()) return gdError("Choose where to save the part.");
    if (pdf !== gd.pdf) {
      gd.regions = gdEmpty();
      gd.suggestions = null;
      gd.preview = null;
    }
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
  gd.step = step;
  gd.visited.add(step);
  if (gdIsPick(step)) {
    // Start on the first place already chosen, else on the best suggestion.
    const key = GD_STEPS[step - 1].key;
    const chosen = gd.regions[key][0];
    const best = gd.suggestions && gd.suggestions[key][0];
    gd.page = chosen ? chosen.page : best ? best.page : 1;
  }
  gdShow(step);
  if (gdIsPick(step)) await gdLoadPage();
  if (step === GD_LAST) await gdUpdatePreview();
}

function gdShow(step) {
  gd.step = step;
  const s = GD_STEPS[step - 1];
  $("gd-p1").hidden = step !== 1;
  $("gd-pick").hidden = !gdIsPick(step);
  $("gd-p5").hidden = step !== GD_LAST;
  $("gd-back").hidden = step === 1;
  $("gd-next").hidden = step === GD_LAST;
  $("gd-skip").hidden = !gdIsPick(step);
  $("ds-run").hidden = step !== GD_LAST;
  $("gd-next").disabled = gd.loading;
  $("gd-next").textContent = gd.loading ? "Reading the datasheet…" : step === GD_LAST - 1 ? "Review" : "Next";
  if (gdIsPick(step)) {
    $("gd-pick-title").textContent = s.title;
    $("gd-pick-help").textContent = s.help;
    $("gd-skip").textContent = "Let the program search for this";
    gdPaintSuggestions();
    gdPaintRegions();
    gdPaintSize();
  }
  if (step === GD_LAST) gdPaintSummary();
  gdPaintStepper();
}

function gdPaintStepper() {
  const ol = $("gd-stepper");
  ol.replaceChildren(
    ...GD_STEPS.map((s, i) => {
      const n = i + 1;
      const done = s.key ? gd.regions[s.key].length > 0 : n < gd.step;
      const li = el("li", { class: "gd-step" + (n === gd.step ? " current" : "") + (done ? " done" : "") });
      const b = el("button", { type: "button", class: "gd-step-btn", "aria-current": n === gd.step ? "step" : "false" });
      b.append(el("span", { class: "gd-dot", text: done && n !== gd.step ? "✓" : String(n) }), el("span", { text: s.label }));
      b.disabled = gd.loading || (n > 1 && !gd.suggestions) || n === gd.step;
      b.addEventListener("click", () => gdGo(n));
      li.append(b);
      return li;
    })
  );
}

// ---- The page viewer -----------------------------------------------------------------------

async function gdLoadPage() {
  if (!gd.pdf) return;
  try {
    const image = await invoke("page_image", { pdf: gd.pdf, page: gd.page, width: 900 });
    const canvas = $("gd-canvas");
    canvas.width = image.width;
    canvas.height = image.height;
    canvas.getContext("2d").putImageData(new ImageData(decodeBase64(image.rgba_base64), image.width, image.height), 0, 0);
    gd.count = image.page_count;
    gd.page = Math.min(Math.max(gd.page, 1), image.page_count);
    $("gd-page").value = gd.page;
    $("gd-page").max = image.page_count;
    $("gd-count").textContent = "of " + image.page_count;
    gdPaintOverlay();
    gdPaintSuggestions();
  } catch (e) {
    gdError(String(e));
  }
}

function gdGoPage(page) {
  gd.page = Math.min(Math.max(Number(page) || 1, 1), gd.count || 1);
  gdLoadPage();
}

function gdPaintOverlay() {
  const o = $("gd-overlay");
  const regions = gd.regions[gdKey()] || [];
  o.replaceChildren(
    ...regions
      .filter((r) => r.page === gd.page)
      .map((r) => {
        const b = el("div", { class: "gd-box" });
        b.style.left = r.bbox[0] * 100 + "%";
        b.style.top = r.bbox[1] * 100 + "%";
        b.style.width = (r.bbox[2] - r.bbox[0]) * 100 + "%";
        b.style.height = (r.bbox[3] - r.bbox[1]) * 100 + "%";
        return b;
      })
  );
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

function gdPaintRegions() {
  const key = gdKey();
  const box = $("gd-regions");
  const regions = gd.regions[key] || [];
  if (!regions.length) {
    box.replaceChildren(el("span", { class: "muted small", text: "Nothing chosen yet. Drag on the page to draw a box, or use the whole page." }));
    return;
  }
  box.replaceChildren(
    ...regions.map((r, i) => {
      const whole = r.bbox[0] <= 0.001 && r.bbox[1] <= 0.001 && r.bbox[2] >= 0.999 && r.bbox[3] >= 0.999;
      const chip = el("span", { class: "chip chosen" });
      const go = el("button", { type: "button", class: "chip-go", text: "page " + r.page + (whole ? ", whole page" : ", box") });
      go.addEventListener("click", () => gdGoPage(r.page));
      const rm = el("button", { type: "button", class: "chip-x", "aria-label": "Remove", text: "✕" });
      rm.addEventListener("click", () => {
        regions.splice(i, 1);
        gdChanged();
        gdPaintOverlay();
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
  const p = gd.preview && gd.preview[key];
  const regions = gd.regions[key];
  if (!regions.length) {
    out.textContent = p && p.automatic ? "Nothing chosen: the program will search the datasheet for this and send about " + p.automatic.toLocaleString() + " characters." : "Nothing chosen: the program will search the datasheet for this.";
    out.className = "muted small";
    return;
  }
  if (!p) {
    out.textContent = "Counting…";
    return;
  }
  const times = p.automatic > p.chosen && p.chosen > 0 ? p.automatic / p.chosen : 0;
  out.textContent = "The model will read " + p.chosen.toLocaleString() + " characters of text." + (times >= 1.2 ? " The search would send about " + p.automatic.toLocaleString() + ", " + (times >= 10 ? Math.round(times) : times.toFixed(1)) + " times more." : "");
  out.className = "small " + (p.chosen > 60000 ? "warn-text" : "ok-text");
}

// A change in the places: recount (after a short wait) and show what the model will read.
function gdChanged() {
  gdPaintStepper();
  gdPaintRegions();
  gdPaintSize();
  gdPaintOverlay();
  clearTimeout(gd.timer);
  gd.timer = setTimeout(async () => {
    await gdUpdatePreview();
    if (gdIsPick(gd.step)) gdPaintSize();
    if (gd.step === GD_LAST) gdPaintSummary();
  }, 300);
}

async function gdUpdatePreview() {
  try {
    gd.preview = await invoke("preview_guide", { pdf: gd.pdf, guide: gdGuide(), hint: $("ds-hint").value });
    gdError("");
  } catch (e) {
    gd.preview = null;
    gdError(String(e));
  }
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

// Dragging a box on the page adds it to the places of this step.
(() => {
  const wrap = $("gd-wrap");
  const draft = el("div", { class: "gd-box draft" });
  draft.hidden = true;
  // Boxes are placed inside the page itself, not the scrolling frame around it.
  $("gd-inner").append(draft);
  const frac = (ev) => {
    const r = $("gd-canvas").getBoundingClientRect();
    return [Math.min(Math.max((ev.clientX - r.left) / r.width, 0), 1), Math.min(Math.max((ev.clientY - r.top) / r.height, 0), 1)];
  };
  const place = (a, b) => {
    const l = Math.min(a[0], b[0]), r = Math.max(a[0], b[0]), t = Math.min(a[1], b[1]), bo = Math.max(a[1], b[1]);
    draft.hidden = false;
    draft.style.left = l * 100 + "%";
    draft.style.top = t * 100 + "%";
    draft.style.width = (r - l) * 100 + "%";
    draft.style.height = (bo - t) * 100 + "%";
    return [l, t, r, bo];
  };
  wrap.addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0 || !gdKey()) return;
    gd.drag = frac(ev);
    wrap.setPointerCapture(ev.pointerId);
    place(gd.drag, gd.drag);
  });
  wrap.addEventListener("pointermove", (ev) => {
    if (gd.drag) place(gd.drag, frac(ev));
  });
  wrap.addEventListener("pointerup", async (ev) => {
    if (!gd.drag) return;
    const bbox = place(gd.drag, frac(ev));
    gd.drag = null;
    draft.hidden = true;
    if (bbox[2] - bbox[0] < 0.01 || bbox[3] - bbox[1] < 0.01) return; // a click
    const region = { page: gd.page, bbox };
    gd.regions[gdKey()].push(region);
    gdChanged();
    gdShowText(region);
  });
  $("gd-whole").addEventListener("click", () => {
    const region = { page: gd.page, bbox: [0, 0, 1, 1] };
    gd.regions[gdKey()].push(region);
    gdChanged();
  });
  $("gd-prev").addEventListener("click", () => gdGoPage(gd.page - 1));
  $("gd-nextpage").addEventListener("click", () => gdGoPage(gd.page + 1));
  $("gd-page").addEventListener("change", () => gdGoPage($("gd-page").value));
})();

// ---- The last step -------------------------------------------------------------------------

function gdDescribe(regions) {
  if (!regions.length) return "searched by the program";
  return regions
    .map((r) => {
      const whole = r.bbox[0] <= 0.001 && r.bbox[1] <= 0.001 && r.bbox[2] >= 0.999 && r.bbox[3] >= 0.999;
      return "page " + r.page + (whole ? "" : " (box)");
    })
    .join(", ");
}

function gdPaintSummary() {
  const rows = [["metadata", "Part number", 2], ["pins", "Pin table", 3], ["package", "Package outline", 4], ["footprint", "Recommended footprint", 5]].map(([key, label, step]) => {
    const p = gd.preview && gd.preview[key];
    const row = el("li", { class: "gd-sum-row" });
    const edit = el("button", { type: "button", class: "link", text: "change" });
    edit.addEventListener("click", () => gdGo(step));
    const size = p ? (gd.regions[key].length ? p.chosen.toLocaleString() + " characters" : p.automatic ? "about " + p.automatic.toLocaleString() + " characters" : "") : "";
    row.append(el("strong", { text: label }), el("span", { class: gd.regions[key].length ? "ok-text" : "muted", text: gdDescribe(gd.regions[key]) }), el("span", { class: "muted small", text: size }), edit);
    return row;
  });
  $("gd-summary").replaceChildren(...rows);
  const keys = ["metadata", "pins", "package", "footprint"];
  const total = gd.preview ? keys.reduce((a, k) => a + (gd.regions[k].length ? gd.preview[k].chosen : gd.preview[k].automatic), 0) : 0;
  const auto = gd.preview ? keys.reduce((a, k) => a + gd.preview[k].automatic, 0) : 0;
  $("gd-total").textContent = gd.preview ? "About " + total.toLocaleString() + " characters will be sent in total" + (auto > total ? " instead of about " + auto.toLocaleString() + " when the program searches everything." : ".") : "";
}

$("gd-next").addEventListener("click", () => gdGo(Math.min(gd.step + 1, GD_LAST)));
$("gd-back").addEventListener("click", () => gdGo(Math.max(gd.step - 1, 1)));
$("gd-skip").addEventListener("click", () => {
  gd.regions[gdKey()] = [];
  gdChanged();
  gdGo(Math.min(gd.step + 1, GD_LAST));
});
$("gd-auto").addEventListener("click", () => {
  // Skip the guide: the program searches everything.
  gd.regions = gdEmpty();
  gd.preview = null;
  gdGo(GD_LAST);
});
