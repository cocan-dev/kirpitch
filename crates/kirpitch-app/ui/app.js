// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// Talks to the Rust side through Tauri. When the page is opened outside the app
// (for example in a browser while designing it) the calls fail with a clear
// message instead of breaking the page.
const invoke = (cmd, args) => {
  const tauri = window.__TAURI__;
  if (!tauri || !tauri.core) {
    return Promise.reject("This page runs outside the Kirpitch app, so it cannot read files.");
  }
  return tauri.core.invoke(cmd, args);
};

const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...children) => {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else node.setAttribute(k, v);
  }
  for (const c of children) node.append(c);
  return node;
};

let current = null; // the SpecView that is shown

function show(message, kind) {
  const m = $("message");
  m.textContent = message;
  m.className = "message" + (kind ? " " + kind : "");
  m.hidden = !message;
  clearTimeout(show.timer);
  if (message && kind === "ok") show.timer = setTimeout(() => (m.hidden = true), 6000);
}

function recentPaths() {
  try {
    return JSON.parse(localStorage.getItem("kirpitch.recent") || "[]");
  } catch {
    return [];
  }
}

function remember(path) {
  try {
    const list = [path, ...recentPaths().filter((p) => p !== path)].slice(0, 8);
    localStorage.setItem("kirpitch.recent", JSON.stringify(list));
  } catch {
    // Storage may be unavailable; the list of recent files is only a convenience.
  }
  renderRecent();
}

function renderRecent() {
  const list = $("recent");
  list.replaceChildren(...recentPaths().map((p) => el("option", { value: p })));
}

function sourceCell(text, low) {
  const cls = low ? "low" : text === "no source" ? "nosrc" : "";
  return el("td", { class: cls, text: low ? text + " (low confidence)" : text });
}

function table(headers, rows, pinNumbers) {
  const head = el("tr");
  headers.forEach((h) => head.append(el("th", { scope: "col", text: h })));
  const body = rows.map((cells, i) => {
    const tr = el("tr");
    cells.forEach((c) => tr.append(c instanceof Node ? c : el("td", { text: c })));
    if (pinNumbers) {
      tr.className = "clickable";
      tr.dataset.pin = pinNumbers[i];
      tr.addEventListener("click", () => selectPin(pinNumbers[i], true));
    }
    return tr;
  });
  return el("table", {}, el("thead", {}, head), el("tbody", {}, ...body));
}

function renderPinout(view) {
  const box = $("view-pinout");
  const head = el("tr");
  ["Pin", "Name", "Type", "Group", "Source", ""].forEach((h) => head.append(el("th", { scope: "col", text: h })));
  const rows = view.pins.map((p, i) => pinRow(view, p, i));
  const add = el("button", { type: "button", class: "add-pin", text: "Add pin" });
  add.addEventListener("click", () => {
    const draft = draftPins(view);
    const numbers = draft.map((d) => Number(d.number)).filter((n) => Number.isFinite(n));
    draft.push({ number: String((numbers.length ? Math.max(...numbers) : 0) + 1), name: "", electrical_type: "unspecified", group: null, page: null, region: null });
    savePins(view, draft);
  });
  box.replaceChildren(
    el("table", {}, el("thead", {}, head), el("tbody", {}, ...rows)),
    add,
    el("p", { class: "muted small edit-hint", text: "Edit a cell and press Enter or click away to save. Changing the pinout opens the approval again." })
  );
}

// The pins as the window has them, ready to be changed and sent back.
function draftPins(view) {
  return view.pins.map((p) => ({
    number: p.number,
    name: p.name,
    electrical_type: p.electrical_type,
    group: p.group,
    page: p.page,
    region: p.region,
  }));
}

async function savePins(view, draft, keepNumber) {
  try {
    const next = await invoke("save_pins", { path: view.path, pins: draft });
    const keep = keepNumber === undefined ? selected : keepNumber;
    render(next, true);
    if (keep) selectPin(keep, false);
  } catch (e) {
    show(String(e), "error");
    render(view, true);
  }
}

function pinRow(view, p, i) {
  const tr = el("tr", { class: "clickable" });
  tr.dataset.pin = p.number;
  const field = (key, cls) => {
    const input = el("input", { type: "text", value: p[key] || "", spellcheck: "false", "aria-label": key + " of pin " + p.number });
    input.addEventListener("change", () => {
      const draft = draftPins(view);
      draft[i][key] = key === "group" && !input.value.trim() ? null : input.value;
      savePins(view, draft, key === "number" ? input.value.trim() : p.number);
    });
    return el("td", { class: "edit " + (cls || "") }, input);
  };
  const type = el("select", { "aria-label": "type of pin " + p.number });
  view.electrical_types.forEach((t) => {
    const o = el("option", { value: t, text: t });
    if (t === p.electrical_type) o.selected = true;
    type.append(o);
  });
  type.addEventListener("change", () => {
    const draft = draftPins(view);
    draft[i].electrical_type = type.value;
    savePins(view, draft, p.number);
  });
  const action = (label, title, fn, disabled) => {
    const b = el("button", { type: "button", text: label, title, "aria-label": title });
    b.disabled = !!disabled;
    b.addEventListener("click", (ev) => {
      ev.stopPropagation();
      const draft = draftPins(view);
      fn(draft);
      savePins(view, draft, p.number);
    });
    return b;
  };
  const actions = el(
    "td",
    { class: "pin-actions" },
    action("↑", "Move up", (d) => d.splice(i - 1, 0, d.splice(i, 1)[0]), i === 0),
    action("↓", "Move down", (d) => d.splice(i + 1, 0, d.splice(i, 1)[0]), i === view.pins.length - 1),
    action("×", "Delete pin " + p.number, (d) => d.splice(i, 1))
  );
  tr.append(field("number", "num-in"), field("name"), el("td", { class: "edit" }, type), field("group"), sourceCell(p.source, p.low_confidence), actions);
  // Clicking the row (not a control) selects the pin and shows its datasheet page;
  // focusing a control only selects it.
  tr.addEventListener("click", (ev) => {
    if (!ev.target.closest("input, select, button")) selectPin(p.number, true);
  });
  tr.addEventListener("focusin", () => selectPin(p.number, false));
  return tr;
}

function renderFootprint(view) {
  $("view-footprint").replaceChildren(
    table(
      ["Pad", "Shape", "x", "y", "Size x", "Size y", "Drill", "Source"],
      view.pads.map((p) => [
        p.number,
        p.shape,
        el("td", { class: "num", text: p.x }),
        el("td", { class: "num", text: p.y }),
        el("td", { class: "num", text: p.size_x }),
        el("td", { class: "num", text: p.size_y }),
        el("td", { class: "num", text: p.drill || "–" }),
        sourceCell(p.source, false),
      ]),
      view.pads.map((p) => p.number)
    )
  );
}

$("open-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const path = $("path").value.trim();
  if (!path) return show("Type the path of a PartSpec file first.", "error");
  try {
    const view = await invoke("open", { path });
    render(view);
    remember(path);
    show("", "");
  } catch (e) {
    show(String(e), "error");
  }
});

$("export-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  if (!current) return;
  const outDir = $("out-dir").value.trim();
  if (!outDir) return show("Type the folder to write into.", "error");
  const target = $("target").value;
  try {
    const written = await invoke("export", {
      path: current.path,
      outDir,
      target: target ? Number(target) : null,
    });
    $("written").replaceChildren(...written.map((w) => el("li", { text: w })));
    show("Wrote " + written.length + " files.", "ok");
  } catch (e) {
    show(String(e), "error");
  }
});

renderRecent();

// ---- Datasheet pages -------------------------------------------------------

function savedPdf(specPath) {
  try {
    return localStorage.getItem("kirpitch.pdf:" + specPath) || "";
  } catch {
    return "";
  }
}

function rememberPdf(specPath, pdfPath) {
  try {
    localStorage.setItem("kirpitch.pdf:" + specPath, pdfPath);
  } catch {
    // Only a convenience.
  }
}

let pageState = { path: "", page: 1, count: 0 };

function decodeBase64(text) {
  const bin = atob(text);
  const bytes = new Uint8ClampedArray(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

// Draws a page of the datasheet and, when given, a box around a region
// (fractions from the top-left corner).
async function showPage(page, region) {
  const pdf = $("pdf-path").value.trim();
  if (!pdf) {
    $("pdf-hint").textContent = "Type the path of the datasheet PDF first.";
    return;
  }
  const image = await invoke("page_image", { pdf, page, width: 900 });
  const canvas = $("page-canvas");
  canvas.width = image.width;
  canvas.height = image.height;
  canvas.getContext("2d").putImageData(new ImageData(decodeBase64(image.rgba_base64), image.width, image.height), 0, 0);
  pageState = { path: pdf, page: Math.min(Math.max(page, 1), image.page_count), count: image.page_count };
  $("page-no").value = pageState.page;
  $("page-no").max = image.page_count;
  $("page-count").textContent = "of " + image.page_count;
  $("page-wrap").hidden = false;
  const box = $("region");
  if (region) {
    box.hidden = false;
    box.style.left = region[0] * 100 + "%";
    box.style.top = region[1] * 100 + "%";
    box.style.width = (region[2] - region[0]) * 100 + "%";
    box.style.height = (region[3] - region[1]) * 100 + "%";
  } else {
    box.hidden = true;
  }
  if (current) rememberPdf(current.path, pdf);
  show("", "");
}

const showError = (e) => show(String(e), "error");
$("pdf-form").addEventListener("submit", (ev) => {
  ev.preventDefault();
  showPage(Number($("page-no").value) || 1).catch(showError);
});
$("prev-page").addEventListener("click", () => showPage(pageState.page - 1).catch(showError));
$("next-page").addEventListener("click", () => showPage(pageState.page + 1).catch(showError));
$("page-no").addEventListener("change", () => showPage(Number($("page-no").value) || 1).catch(showError));

// ---- Selecting a pin: highlighted in every view ----------------------------

let selected = null; // pin number

const esc = (s) => (window.CSS && CSS.escape ? CSS.escape(s) : s.replace(/["\\]/g, "\\$&"));

function selectPin(number, jump) {
  selected = selected === number && !jump ? null : number;
  document.querySelectorAll(".sel").forEach((n) => n.classList.remove("sel"));
  if (selected === null) return;
  document.querySelectorAll('[data-pin="' + esc(selected) + '"]').forEach((n) => n.classList.add("sel"));
  if (!jump || !current) return;
  // The datasheet follows: the page of the pin, else of its pad.
  const row = current.pins.find((p) => p.number === selected && p.page) || current.pads.find((p) => p.number === selected && p.page);
  if (row) {
    showPage(row.page, row.region).catch(showError);
  } else {
    $("pdf-hint").textContent = "Pin " + selected + " has no source page.";
  }
}

// Clicking a pin in a drawing selects it too.
for (const id of ["symbol", "svg"]) {
  $(id).addEventListener("click", (ev) => {
    const target = ev.target.closest("[data-pin]");
    if (target) selectPin(target.dataset.pin, true);
  });
}
document.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape") selectPin(selected, false);
});

// ---- Selecting a region of the page by dragging ----------------------------

let pick = null; // { page, region: [l, t, r, b] } after a drag

function fraction(ev) {
  const r = $("page-canvas").getBoundingClientRect();
  return [Math.min(Math.max((ev.clientX - r.left) / r.width, 0), 1), Math.min(Math.max((ev.clientY - r.top) / r.height, 0), 1)];
}

function placeBox(box, a, b) {
  const [l, r] = [Math.min(a[0], b[0]), Math.max(a[0], b[0])];
  const [t, bo] = [Math.min(a[1], b[1]), Math.max(a[1], b[1])];
  box.hidden = false;
  box.style.left = l * 100 + "%";
  box.style.top = t * 100 + "%";
  box.style.width = (r - l) * 100 + "%";
  box.style.height = (bo - t) * 100 + "%";
  return [l, t, r, bo];
}

(() => {
  const wrap = $("page-wrap");
  const draft = el("div", { class: "region draft" });
  draft.hidden = true;
  // Boxes are placed inside the page itself, not the scrolling frame around it.
  $("page-inner").append(draft);
  let start = null;

  wrap.addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0) return;
    start = fraction(ev);
    wrap.setPointerCapture(ev.pointerId);
    placeBox(draft, start, start);
  });
  wrap.addEventListener("pointermove", (ev) => {
    if (start) placeBox(draft, start, fraction(ev));
  });
  wrap.addEventListener("pointerup", async (ev) => {
    if (!start) return;
    const region = placeBox(draft, start, fraction(ev));
    start = null;
    if (region[2] - region[0] < 0.01 || region[3] - region[1] < 0.01) {
      draft.hidden = true; // a click, not a drag
      return;
    }
    try {
      const table = await invoke("region_table", { pdf: pageState.path, page: pageState.page, region });
      pick = { page: pageState.page, region };
      $("pick-where").textContent = "page " + pageState.page;
      $("pick-text").value = table.tsv;
      $("pick-warnings").replaceChildren();
      $("pick").hidden = false;
      show("", "");
    } catch (e) {
      draft.hidden = true;
      show(String(e), "error");
    }
  });

  const close = () => {
    pick = null;
    draft.hidden = true;
    $("pick").hidden = true;
  };
  $("pick-cancel").addEventListener("click", close);
  $("pick-use").addEventListener("click", async () => {
    if (!pick || !current) return;
    try {
      const out = await invoke("pins_from_table", {
        path: current.path,
        tsv: $("pick-text").value,
        page: pick.page,
        region: pick.region,
      });
      const kept = pick;
      render(out.view, true);
      show("Read " + out.view.pins.length + " pins from page " + kept.page + ". Check them, then approve the pinout.", "ok");
      $("pick").hidden = false;
      $("pick-where").textContent = "page " + kept.page;
      $("pick-warnings").replaceChildren(...out.warnings.map((w) => el("li", { text: w })));
    } catch (e) {
      show(String(e), "error");
    }
  });
})();
