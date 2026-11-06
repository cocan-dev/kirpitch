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
  try {
    if (gdIsPick(step)) {
      // Start on the first place already chosen, else on the best suggestion.
      const key = GD_STEPS[step - 1].key;
      const chosen = gd.regions[key][0];
      const best = ((gd.suggestions && gd.suggestions[key]) || [])[0];
      gd.page = chosen ? chosen.page : best ? best.page : 1;
    }
    gd.step = step;
    gd.visited.add(step);
    gd.extending = false;
    gdShow(step);
    if (gdIsPick(step)) await gdLoadPage();
    if (step === GD_LAST) await gdUpdatePreview();
  } catch (e) {
    // Whatever went wrong, the screen shows the step that is current.
    gdError(String(e));
    gdShow(gd.step);
  }
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

// ---- The page viewer: the whole document, scrolling ---------------------------------------------
//
// Every page has its place in one long column; a page is drawn when it comes
// near the window and let go when it is far away, so a 200 page datasheet costs
// no more than a few pages. Boxes are drawn on the page under the pointer.

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
    // The pages nearest to the one in view first.
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
    // The real shape of the page, so what is drawn and what is clicked agree.
    el1.style.aspectRatio = image.width + " / " + image.height;
    el1.classList.add("ready");
    gdDrawn.add(p);
    // Let go of the pages that are far away.
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

// The page in the middle of the window is the current page.
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

// ---- Drawing a box, and continuing a table over several pages ------------------------------------

function gdSetExtending(on) {
  gd.extending = on;
  $("gd-wrap").classList.toggle("extending", on);
  $("gd-hint").hidden = !on;
  $("gd-extend").textContent = on ? "Cancel" : "Continue the table to…";
  if (on) $("gd-hint").textContent = "Scroll to the page where the table ends and click on its last row. Every page in between is added with the same columns.";
}

// Adds the pages from the last box down to the point that was clicked.
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
    // On its own page the table runs to the bottom, then whole pages, then down to the click.
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
    if (ev.button !== 0 || !gdKey()) return;
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
      // A click, not a drag.
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
