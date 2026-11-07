// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// The ways to start: examples, recent files, a new part, a part built from a
// datasheet, copies, file pickers and dropping files on the window. It uses the
// helpers of app.js ($, el, invoke, show, render, remember, ...).

// ---- Remembered folders ----------------------------------------------------

function lastDir(kind) {
  try {
    return localStorage.getItem("kirpitch.dir:" + kind) || null;
  } catch {
    return null;
  }
}

function rememberDir(kind, path) {
  const dir = path.replace(/[\\/][^\\/]*$/, "");
  if (!dir || dir === path) return;
  try {
    localStorage.setItem("kirpitch.dir:" + kind, dir);
  } catch {
    // Only a convenience.
  }
}

async function pickFile(kind) {
  const path = await invoke("pick_file", { kind, startDir: lastDir(kind) });
  if (path) rememberDir(kind, path);
  return path;
}

async function pickSave(kind, name) {
  const path = await invoke("pick_save", { kind, name, startDir: lastDir(kind) });
  if (path) rememberDir(kind, path);
  return path;
}

async function pickFolder() {
  const path = await invoke("pick_folder", { startDir: lastDir("out") });
  if (path) rememberDir("out", path + "\\x");
  return path;
}

// ---- Opening ---------------------------------------------------------------

async function openPath(path) {
  try {
    const view = await invoke("open", { path });
    $("path").value = path;
    render(view);
    remember(path);
    show("", "");
  } catch (e) {
    show(String(e), "error");
  }
}

// Remember the datasheet of a part before it is shown, so the page appears.
function rememberPdf(specPath, pdf) {
  try {
    localStorage.setItem("kirpitch.pdf:" + specPath, pdf);
  } catch {
    // Only a convenience.
  }
}

async function openFileDialog() {
  try {
    const path = await pickFile("spec");
    if (path) await openPath(path);
  } catch (e) {
    show(String(e), "error");
  }
}

$("btn-open-file").addEventListener("click", openFileDialog);
$("start-open").addEventListener("click", openFileDialog);

// The buttons that need an open part follow what is shown.
const renderBase = render;
render = function (view, keepSheet) {
  renderBase(view, keepSheet);
  $("btn-copy").disabled = !view;
  $("path").value = view.path;
};

// ---- Start screen lists ----------------------------------------------------

function baseName(path) {
  return path.split(/[\\/]/).pop();
}

function renderRecentList() {
  const list = $("recent-list");
  const paths = recentPaths();
  if (!paths.length) {
    list.replaceChildren(el("li", { class: "meta", text: "Nothing yet." }));
    return;
  }
  list.replaceChildren(
    ...paths.map((p) => {
      const open = el("button", { type: "button", class: "link", text: baseName(p) });
      open.addEventListener("click", () => openPath(p));
      const done = progressOf(p);
      const badge = done ? el("span", { class: "badge" + (done.split("/")[0] === done.split("/")[1] ? " all" : ""), text: done.replace("/", " of ") + " approved" }) : null;
      return el("li", {}, open, ...(badge ? [badge] : []), el("span", { class: "meta", text: p }));
    })
  );
}

const renderRecentBase = renderRecent;
renderRecent = function () {
  renderRecentBase();
  renderRecentList();
};

async function openExample(example, fresh) {
  try {
    const view = await invoke("open_example_part", { name: example.name, fresh });
    if (example.pdf) rememberPdf(view.path, example.pdf);
    render(view);
    remember(view.path);
    show(
      fresh
        ? "Opened a fresh copy of " + example.mpn + "."
        : "Opened your working copy of " + example.mpn + ". The original is not changed.",
      "ok"
    );
  } catch (e) {
    show(String(e), "error");
  }
}

async function renderExamples() {
  const list = $("examples");
  let examples = [];
  try {
    examples = await invoke("examples", {});
  } catch {
    // Outside the app there are no examples.
  }
  if (!examples.length) {
    list.replaceChildren(el("li", { class: "meta", text: "No examples found (they are part of a checkout of Kirpitch)." }));
    return;
  }
  list.replaceChildren(
    ...examples.map((ex) => {
      const open = el("button", { type: "button", class: "link", text: ex.mpn });
      open.addEventListener("click", () => openExample(ex, false));
      const fresh = el("button", { type: "button", class: "link fresh", text: "fresh copy" });
      fresh.title = "Start again from the original";
      fresh.addEventListener("click", () => openExample(ex, true));
      return el("li", {}, open, el("span", { class: "meta", text: ex.package + (ex.pdf ? " · datasheet available" : "") }), fresh);
    })
  );
}

renderExamples();
renderRecentList();

// ---- Browse buttons --------------------------------------------------------

$("pdf-browse").addEventListener("click", async () => {
  try {
    const path = await pickFile("pdf");
    if (!path) return;
    $("pdf-path").value = path;
    $("pdf-form").requestSubmit();
  } catch (e) {
    show(String(e), "error");
  }
});

$("out-browse").addEventListener("click", async () => {
  try {
    const path = await pickFolder();
    if (path) $("out-dir").value = path;
  } catch (e) {
    show(String(e), "error");
  }
});

// ---- Save a copy -----------------------------------------------------------

$("btn-copy").addEventListener("click", async () => {
  if (!current) return;
  try {
    const stem = baseName(current.path).replace(/\.json$/i, "");
    const dest = await pickSave("spec", stem + "-copy.json");
    if (!dest) return;
    const pdf = savedPdf(current.path);
    const view = await invoke("copy_part", { path: current.path, dest });
    if (pdf) rememberPdf(view.path, pdf);
    render(view);
    remember(view.path);
    show("Saved a copy and opened it: " + view.path, "ok");
  } catch (e) {
    show(String(e), "error");
  }
});

// ---- New part --------------------------------------------------------------

// Dimensions are in millimetres. Ranges are [min, max].
const PRESETS = [
  { name: "SOIC-8 (3.9 mm body)", kind: "dual-row", family: "SOIC", pin_count: 8, pitch: 1.27, height: 1.75, body_length: 4.9, body_width: 3.9, lead_span: [5.8, 6.2], lead_length: [0.4, 1.27], lead_width: [0.31, 0.51] },
  { name: "SOIC-16 (7.5 mm body)", kind: "dual-row", family: "SOIC", pin_count: 16, pitch: 1.27, height: 2.65, body_length: 10.3, body_width: 7.5, lead_span: [10.0, 10.65], lead_length: [0.4, 1.27], lead_width: [0.31, 0.51] },
  { name: "TSSOP-14 (4.4 mm body)", kind: "dual-row", family: "TSSOP", pin_count: 14, pitch: 0.65, height: 1.2, body_length: 5.0, body_width: 4.4, lead_span: [6.2, 6.6], lead_length: [0.45, 0.75], lead_width: [0.19, 0.3] },
  { name: "SOT-23-5", kind: "sot23", family: "SOT-23", pin_count: 5, pitch: 0.95, height: 1.45, body_length: 2.9, body_width: 1.6, lead_span: [2.6, 3.0], lead_length: [0.3, 0.6], lead_width: [0.3, 0.5] },
  { name: "DIP-8 (300 mil)", kind: "dip", family: "DIP", pin_count: 8, pitch: 2.54, height: 5.0, body_length: 9.3, body_width: 6.4, row_spacing: 7.62, pad_diameter: 1.6, drill: 0.8 },
  { name: "QFN-16 (3 x 3 mm, 0.5 pitch)", kind: "qfn", family: "QFN", pins_x: 4, pins_y: 4, pitch: 0.5, height: 0.8, body_x: [2.95, 3.05], body_y: [2.95, 3.05], lead_length: [0.3, 0.5], lead_width: [0.18, 0.3], exposed_pad: [1.7, 1.7] },
  { name: "QFN-32 (4 x 4 mm, 0.4 pitch)", kind: "qfn", family: "QFN", pins_x: 8, pins_y: 8, pitch: 0.4, height: 1.0, body_x: [3.95, 4.05], body_y: [3.95, 4.05], lead_length: [0.35, 0.45], lead_width: [0.15, 0.25], exposed_pad: [2.65, 2.65] },
];

// The fields of each kind: [key, label, help, type]; type is "n" (number), "r"
// (range), "c" (count) or "t" (text).
const COMMON = [["pitch", "Pitch (e)", "distance between pins", "n"], ["height", "Height (A)", "largest height", "n"]];
const FIELDS = {
  "dual-row": [["family", "Family name", "for the footprint name, e.g. SOIC", "t"], ["pin_count", "Pins", "all terminals", "c"], ...COMMON, ["body_length", "Body length (D)", "along the pin rows", "n"], ["body_width", "Body width (E1)", "across the rows", "n"], ["lead_span", "Lead span (E)", "tip to tip: min, max", "r"], ["lead_length", "Lead length (L)", "min, max", "r"], ["lead_width", "Lead width (b)", "min, max", "r"]],
  sot23: [["family", "Family name", "e.g. SOT-23", "t"], ["pin_count", "Pins", "3, 5 or 6", "c"], ...COMMON, ["body_length", "Body length (D)", "along the pin rows", "n"], ["body_width", "Body width (E1)", "across the rows", "n"], ["lead_span", "Lead span (E)", "tip to tip: min, max", "r"], ["lead_length", "Lead length (L)", "min, max", "r"], ["lead_width", "Lead width (b)", "min, max", "r"]],
  dip: [["family", "Family name", "e.g. DIP", "t"], ["pin_count", "Pins", "all pins", "c"], ...COMMON, ["body_length", "Body length (D)", "along the pin rows", "n"], ["body_width", "Body width (E1)", "across the rows", "n"], ["row_spacing", "Row spacing", "between the two rows", "n"], ["pad_diameter", "Pad diameter", "", "n"], ["drill", "Drill", "", "n"]],
  qfn: [["family", "Family name", "e.g. QFN", "t"], ["pins_x", "Pins top and bottom", "on each of those sides", "c"], ["pins_y", "Pins left and right", "on each of those sides", "c"], ...COMMON, ["body_x", "Body size D", "min, max", "r"], ["body_y", "Body size E", "min, max", "r"], ["lead_length", "Terminal length (L)", "min, max", "r"], ["lead_width", "Terminal width (b)", "min, max", "r"], ["exposed_pad", "Exposed pad", "width, height; empty if none", "r?"]],
};

function fieldInput(key, type, value) {
  const num = (id, v) => {
    const i = el("input", { type: "number", step: "any", min: "0", "data-key": id });
    if (v !== undefined && v !== null) i.value = String(v);
    return i;
  };
  if (type === "t") {
    const i = el("input", { type: "text", "data-key": key, spellcheck: "false" });
    i.value = value || "";
    return i;
  }
  if (type === "n" || type === "c") {
    const i = num(key, value);
    if (type === "c") i.step = "1";
    return i;
  }
  const pair = el("span", { class: "pair" }, num(key + ".0", value ? value[0] : null), num(key + ".1", value ? value[1] : null));
  return pair;
}

function buildFields(kind, values) {
  const box = $("new-fields");
  box.replaceChildren(el("legend", { text: "Package dimensions (mm)" }));
  for (const [key, label, help, type] of FIELDS[kind]) {
    const lab = el("label", {}, el("span", { text: label }), fieldInput(key, type, values[key]), el("span", { class: "help", text: help }));
    box.append(lab);
  }
  $("new-note").textContent =
    kind === "qfn"
      ? "Fine-pitch QFNs (0.4 mm pitch or less) have pads closer than the generic fab profile allows; the findings will say so, and a fab profile of your PCB maker (--fab, KIRPITCH_FAB_PROFILE) sets the limits."
      : "";
}

function readNumber(id) {
  const node = document.querySelector('[data-key="' + id + '"]');
  const v = node ? node.value.trim() : "";
  return v === "" ? null : Number(v);
}

function readForm() {
  const kind = $("new-kind").value;
  const form = {
    kind,
    mpn: $("new-mpn").value,
    manufacturer: $("new-maker").value,
    description: $("new-desc").value,
  };
  for (const [key, , , type] of FIELDS[kind]) {
    if (type === "t") {
      form[key] = document.querySelector('[data-key="' + key + '"]').value.trim() || null;
    } else if (type === "n" || type === "c") {
      const v = readNumber(key);
      if (v !== null) form[key] = v;
    } else {
      const a = readNumber(key + ".0");
      const b = readNumber(key + ".1");
      if (a !== null && b !== null) form[key] = [a, b];
      else if (a !== null || b !== null) throw new Error("Give both the min and the max of " + key.replace("_", " ") + ".");
      else if (type === "r") throw new Error("Give the min and the max of " + key.replace("_", " ") + ".");
    }
  }
  return form;
}

(() => {
  const select = $("new-preset");
  PRESETS.forEach((p, i) => select.append(el("option", { value: String(i), text: p.name })));
  select.append(el("option", { value: "", text: "Empty form" }));
  const apply = () => {
    const p = PRESETS[Number(select.value)];
    if (p) {
      $("new-kind").value = p.kind;
      buildFields(p.kind, p);
    } else {
      buildFields($("new-kind").value, {});
    }
  };
  select.addEventListener("change", apply);
  $("new-kind").addEventListener("change", () => {
    select.value = "";
    buildFields($("new-kind").value, {});
  });
  apply();
})();

function openNewDialog() {
  $("new-error").hidden = true;
  $("new-dialog").showModal();
  $("new-mpn").focus();
}

$("btn-new").addEventListener("click", openNewDialog);
$("start-new").addEventListener("click", openNewDialog);
$("new-cancel").addEventListener("click", () => $("new-dialog").close());

$("new-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const error = (m) => {
    $("new-error").textContent = m;
    $("new-error").hidden = false;
  };
  try {
    const input = readForm();
    if (!input.mpn.trim()) return error("Give the part number.");
    const path = await pickSave("spec", input.mpn.trim().replace(/[^\w.-]+/g, "_") + ".json");
    if (!path) return;
    input.path = path;
    const view = await invoke("create_part", { input });
    $("new-dialog").close();
    render(view);
    remember(view.path);
    show("Made " + view.mpn + " with " + view.pins.length + " placeholder pins. Name them in the pin table, or drag a box over the pin table of the datasheet.", "ok");
  } catch (e) {
    error(String(e && e.message ? e.message : e));
  }
});

// ---- From a datasheet ------------------------------------------------------

function openDatasheetDialog(pdf) {
  $("ds-error").hidden = true;
  $("ds-result").hidden = true;
  if (!buildRunning()) {
    $("ds-progress").hidden = true;
    $("ds-dialog").classList.remove("running");
  }
  if (pdf) $("ds-pdf").value = pdf;
  // The default of "read twice" is a setting.
  if (typeof appSettings !== "undefined" && appSettings && appSettings.extraction && !$("ds-dialog").dataset.touched) $("ds-second").checked = !!appSettings.extraction.second_read;
  if (typeof gdReset === "function") gdReset();
  $("ds-dialog").showModal();
}

$("btn-datasheet").addEventListener("click", () => openDatasheetDialog());
$("start-datasheet").addEventListener("click", () => openDatasheetDialog());
$("ds-cancel").addEventListener("click", () => $("ds-dialog").close());

async function browseInto(id, kind, save) {
  try {
    const path = save ? await pickSave(kind, save) : await pickFile(kind);
    if (path) $(id).value = path;
    return path;
  } catch (e) {
    $("ds-error").textContent = String(e);
    $("ds-error").hidden = false;
    return null;
  }
}

$("ds-pdf-browse").addEventListener("click", async () => {
  const path = await browseInto("ds-pdf", "pdf");
  if (path && !$("ds-out").value) $("ds-out").value = path.replace(/\.pdf$/i, "") + ".partspec.json";
});
$("ds-out-browse").addEventListener("click", () => browseInto("ds-out", "spec", "part.json"));
$("ds-open-settings").addEventListener("click", () => {
  $("ds-dialog").close();
  if (typeof showSettings === "function") showSettings();
});

function listOf(items, cls) {
  return el("ul", {}, ...items.map((t) => el("li", { class: cls || "", text: t })));
}

// What a build gave: what it is, what failed, what to look at, and a way to open the part.
function showBuildResult(outcome, pdf) {
  const box = $("ds-result");
  if (outcome.view && pdf) invoke("remember_datasheet", { pdf }).catch(() => {});
  box.replaceChildren(el("h3", { text: outcome.view ? "Done" : "Not finished" }));
  if (outcome.models.length) box.append(el("p", { class: "muted small", text: "Models" }), listOf(outcome.models));
  box.append(el("p", { class: "muted small", text: outcome.usage }));
  if (outcome.failures.length) box.append(el("p", { text: "What failed" }), listOf(outcome.failures, "bad"));
  if (outcome.notes.length) box.append(el("p", { text: "Check these in review" }), listOf(outcome.notes));
  if (outcome.pins_csv) {
    box.append(el("p", { text: "The pins were read and saved to " + outcome.pins_csv + ". Make the part with New part…, then import the pins from that file by dragging a box over the pin table, or with the pins command." }));
  }
  box.hidden = false;
  if (outcome.view) {
    const open = el("button", { type: "button", class: "primary", text: "Open the part" });
    open.addEventListener("click", () => {
      rememberPdf(outcome.view.path, pdf);
      $("ds-dialog").close();
      render(outcome.view);
      remember(outcome.view.path);
      show("Review the pinout first, then approve each stage.", "ok");
    });
    box.append(open);
  }
}

$("ds-form").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  // Step by step: the part is made from the values that were confirmed.
  if (typeof gdConfirmedMode === "function" && gdConfirmedMode()) return gdBuildConfirmed();
  $("ds-error").hidden = true;
  $("ds-result").hidden = true;
  $("ds-run").disabled = true;
  $("ds-cancel").disabled = true;
  buildBegin($("ds-second").checked);
  try {
    const input = {
      pdf: $("ds-pdf").value.trim(),
      out: $("ds-out").value.trim(),
      hint: $("ds-hint").value,
      second_read: $("ds-second").checked,
      config: "",
      guide: null,
    };
    const outcome = await invoke("build_part", { input });
    buildEnd(!!outcome.view, outcome.failures.length);
    showBuildResult(outcome, input.pdf);
  } catch (e) {
    const message = String(e);
    buildEnd(false, 1, message);
    $("ds-error").textContent = message;
    $("ds-error").hidden = false;
    if (/no language model is configured/.test(message) && typeof showSettings === "function") {
      const open = el("button", { type: "button", class: "primary small-btn", text: "Open settings" });
      open.addEventListener("click", () => {
        $("ds-dialog").close();
        showSettings();
      });
      $("ds-error").append(" ", open);
    }
  } finally {
    $("ds-run").disabled = false;
    $("ds-cancel").disabled = false;
  }
});

// ---- Dropping files on the window -----------------------------------------

(() => {
  const tauri = window.__TAURI__;
  if (!tauri || !tauri.event) return;
  tauri.event.listen("tauri://drag-drop", async (event) => {
    const paths = (event.payload && event.payload.paths) || [];
    const first = paths[0];
    if (!first) return;
    if (/\.json$/i.test(first)) {
      await openPath(first);
    } else if (/\.pdf$/i.test(first)) {
      if (current) {
        $("pdf-path").value = first;
        $("pdf-form").requestSubmit();
      } else {
        openDatasheetDialog(first);
      }
    } else {
      show("Drop a PartSpec (.json) or a datasheet (.pdf).", "error");
    }
  });
})();
