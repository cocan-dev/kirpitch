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

function table(headers, rows, onRow) {
  const head = el("tr");
  headers.forEach((h) => head.append(el("th", { scope: "col", text: h })));
  const body = rows.map((cells, i) => {
    const tr = el("tr");
    cells.forEach((c) => tr.append(c instanceof Node ? c : el("td", { text: c })));
    if (onRow) {
      tr.className = "clickable";
      tr.addEventListener("click", () => {
        document.querySelectorAll("tr.selected").forEach((r) => r.classList.remove("selected"));
        tr.classList.add("selected");
        onRow(i);
      });
    }
    return tr;
  });
  return el("table", {}, el("thead", {}, head), el("tbody", {}, ...body));
}

function renderGates(view) {
  const box = $("gates");
  box.replaceChildren();
  view.gates.forEach((g, i) => {
    const earlier = view.gates.slice(0, i).every((p) => p.status === "approved");
    const chip = el("span", {
      class: "chip " + g.status,
      text: g.status === "stale" ? "open again" : g.status,
    });
    const counts = el("p", { class: "counts" });
    counts.append(
      el("span", { class: "e", text: g.errors + " error" + (g.errors === 1 ? "" : "s") }),
      " · ",
      el("span", { class: "w", text: g.warnings + " warning" + (g.warnings === 1 ? "" : "s") })
    );
    if (g.approved_at) counts.append(" · approved " + g.approved_at + (g.note ? " (" + g.note + ")" : ""));

    const note = el("input", { type: "text", placeholder: "Note (optional)", "aria-label": "Approval note for " + g.name });
    const button = el("button", { type: "submit", class: "primary", text: g.status === "approved" ? "Approved" : "Approve" });
    if (g.status === "approved" || g.errors > 0 || !earlier) button.disabled = true;
    if (!earlier && g.status !== "approved") button.title = "Approve the stage before first";
    if (g.errors > 0) button.title = "Fix the errors first";
    const form = el("form", {}, note, button);
    form.addEventListener("submit", async (ev) => {
      ev.preventDefault();
      try {
        const next = await invoke("approve", { path: view.path, stage: g.name, note: note.value || null });
        render(next);
        show("Approved " + g.name + ".", "ok");
      } catch (e) {
        show(String(e), "error");
      }
    });
    box.append(el("div", { class: "gate" }, el("h3", {}, g.name, chip), counts, form));
  });
}

function renderPinout(view) {
  $("view-pinout").replaceChildren(
    table(
      ["Pin", "Name", "Type", "Group", "Source"],
      view.pins.map((p) => [p.number, p.name, p.electrical_type, p.group || "–", sourceCell(p.source, p.low_confidence)]),
      (i) => showSource(view.pins[i])
    )
  );
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
      (i) => showSource(view.pads[i])
    )
  );
}

function renderFindings(view) {
  const box = $("view-findings");
  $("findings-count").textContent = view.findings.length ? "(" + view.findings.length + ")" : "";
  if (!view.findings.length) {
    box.replaceChildren(el("p", { class: "muted", text: "No findings. All checks pass." }));
    return;
  }
  box.replaceChildren(
    ...view.findings.map((f) =>
      el(
        "div",
        { class: "finding" },
        el("span", { class: "lvl " + f.level, text: f.level }),
        el("strong", { text: f.check }),
        " ",
        el("span", { class: "muted", text: "(" + f.gate + ")" }),
        el("div", { text: f.message + (f.overridden_by ? " — accepted: " + f.overridden_by : "") }),
        el("div", { class: "path", text: f.path })
      )
    )
  );
}

function render(view) {
  current = view;
  $("empty").hidden = true;
  $("work").hidden = false;
  $("mpn").textContent = view.mpn;
  $("part-line").textContent = [view.manufacturer, view.description].filter(Boolean).join(" · ");
  $("package-name").textContent = view.package_name;
  renderGates(view);
  renderPinout(view);
  renderFootprint(view);
  renderFindings(view);
  // The SVG is made by our own Rust code from numbers and escaped text.
  $("svg").innerHTML = view.footprint_svg;
  const saved = savedPdf(view.path);
  $("pdf-path").value = saved;
  if (saved) showPage(1).catch((e) => show(String(e), "error"));
}

function selectTab(name) {
  for (const t of ["pinout", "footprint", "findings"]) {
    $("tab-" + t).setAttribute("aria-selected", String(t === name));
    $("view-" + t).hidden = t !== name;
  }
}

document.querySelectorAll("[data-tab]").forEach((b) => b.addEventListener("click", () => selectTab(b.dataset.tab)));

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

async function showSource(row) {
  selectSide("datasheet");
  if (!row.page) {
    show("This value has no source page.", "error");
    return;
  }
  try {
    await showPage(row.page, row.region);
  } catch (e) {
    show(String(e), "error");
  }
}

function selectSide(name) {
  for (const t of ["datasheet", "footprint"]) {
    $("side-" + t).setAttribute("aria-selected", String(t === name));
    $("pane-" + t).hidden = t !== name;
  }
}

document.querySelectorAll("[data-side]").forEach((b) => b.addEventListener("click", () => selectSide(b.dataset.side)));

const showError = (e) => show(String(e), "error");
$("pdf-form").addEventListener("submit", (ev) => {
  ev.preventDefault();
  showPage(Number($("page-no").value) || 1).catch(showError);
});
$("prev-page").addEventListener("click", () => showPage(pageState.page - 1).catch(showError));
$("next-page").addEventListener("click", () => showPage(pageState.page + 1).catch(showError));
$("page-no").addEventListener("change", () => showPage(Number($("page-no").value) || 1).catch(showError));
