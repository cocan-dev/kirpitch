// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// The Evaluate tab of the settings: run the models and prompts that are saved
// against datasheets with known answers, see how many pins and dimensions come
// out right, and keep every run with the prompts that produced it.

const ev = {
  cases: null, // list of cases, null until loaded
  runs: null, // saved runs, newest first
  picked: new Set(), // names of the cases to run
  second: false,
  label: "",
  adding: false,
  add: { name: "", spec: "", pdf: "", hint: "" },
  running: false,
  seen: 0,
  rows: {}, // case name -> { state, stage, result }
  shown: null, // the run whose table is shown
  error: "",
  poll: null,
};

const EV_STAGE = { read: "reading the PDF", metadata: "part number", pins: "pin table", package: "package outline", footprint: "recommended footprint", assemble: "checking", save: "saving" };

async function evalLoad() {
  try {
    ev.cases = await invoke("eval_cases", {});
    ev.runs = await invoke("eval_runs", {});
    if (!ev.shown && ev.runs.length) ev.shown = ev.runs[0].id;
    if (!ev.picked.size) ev.cases.filter((c) => c.ready).forEach((c) => ev.picked.add(c.name));
  } catch (e) {
    ev.error = String(e);
  }
  renderEvaluate();
}

function pct(a, b) {
  return b ? Math.round((a / b) * 100) + "%" : "–";
}

function share(x) {
  return Math.round(x * 100) + "%";
}

function evalMessage(text, kind) {
  ev.error = kind === "error" ? text : "";
  renderEvaluate();
}

// ---- Running ------------------------------------------------------------------------------

async function evalRun() {
  if (ev.running) return;
  const names = [...ev.picked];
  if (!names.length) return evalMessage("Choose at least one case.", "error");
  // The run uses the saved settings: save what is on the screen first.
  if (dirty() && !(await saveSettings())) return;
  try {
    await invoke("eval_start", { input: { cases: names, second_read: ev.second, label: ev.label } });
  } catch (e) {
    return evalMessage(String(e), "error");
  }
  ev.running = true;
  ev.seen = 0;
  ev.rows = {};
  ev.error = "";
  names.forEach((n) => (ev.rows[n] = { state: "waiting", stage: "", result: null }));
  ev.shown = null;
  ev.poll = setInterval(evalPoll, 500);
  renderEvaluate();
}

async function evalPoll() {
  try {
    const s = await invoke("eval_status", { since: ev.seen });
    ev.seen += s.events.length;
    for (const e of s.events) {
      const row = ev.rows[e.case] || (ev.rows[e.case] = { state: "waiting", stage: "", result: null });
      if (e.state === "started") {
        row.state = "running";
        row.stage = e.stage;
      } else {
        row.state = "done";
        row.result = e.result;
      }
    }
    if (!s.running) {
      clearInterval(ev.poll);
      ev.poll = null;
      ev.running = false;
      ev.runs = await invoke("eval_runs", {});
      ev.shown = s.run_id || (ev.runs[0] && ev.runs[0].id) || null;
      if (s.error) ev.error = s.error;
      ev.rows = {};
    }
  } catch (e) {
    // The next poll tries again.
  }
  renderEvaluate();
}

// ---- Cases --------------------------------------------------------------------------------

async function evalPickPdf(c) {
  try {
    const path = await pickFile("pdf");
    if (!path) return;
    ev.cases = await invoke("eval_set_pdf", { name: c.name, pdf: path });
    ev.cases.filter((x) => x.ready && x.name === c.name).forEach((x) => ev.picked.add(x.name));
    renderEvaluate();
  } catch (e) {
    evalMessage(String(e), "error");
  }
}

async function evalRemoveCase(c) {
  try {
    ev.cases = await invoke("eval_remove_case", { name: c.name });
    ev.picked.delete(c.name);
    renderEvaluate();
  } catch (e) {
    evalMessage(String(e), "error");
  }
}

async function evalSaveCase() {
  const a = ev.add;
  try {
    ev.cases = await invoke("eval_add_case", { name: a.name, spec: a.spec, pdf: a.pdf, hint: a.hint });
    ev.picked.add(a.name.trim());
    ev.adding = false;
    ev.add = { name: "", spec: "", pdf: "", hint: "" };
    ev.error = "";
    renderEvaluate();
  } catch (e) {
    evalMessage(String(e), "error");
  }
}

function pathRow(label, key, kind, help) {
  const input = textInput(ev.add[key], (v) => (ev.add[key] = v.trim()), { placeholder: kind === "spec" ? "A reviewed part (.json)" : "The datasheet (.pdf)" });
  const browse = el("button", { type: "button", text: "Browse…" });
  browse.addEventListener("click", async () => {
    try {
      const path = await pickFile(kind);
      if (path) {
        ev.add[key] = path;
        if (key === "spec" && !ev.add.name) ev.add.name = safeName(path.split(/[\\/]/).pop().replace(/\.(partspec\.)?json$/i, ""));
        renderEvaluate();
      }
    } catch (e) {
      evalMessage(String(e), "error");
    }
  });
  return field(label, el("span", { class: "pick-row" }, input, browse), help);
}

function caseRow(c) {
  const row = el("label", { class: "case-row" });
  const box = el("input", { type: "checkbox" });
  box.checked = ev.picked.has(c.name);
  box.disabled = !c.ready || ev.running;
  box.addEventListener("change", () => {
    if (box.checked) ev.picked.add(c.name);
    else ev.picked.delete(c.name);
  });
  const text = el("span", { class: "case-text" }, el("strong", { text: c.mpn || c.name }), el("span", { class: "muted small", text: [c.name, c.package].filter(Boolean).join(" · ") }));
  const tag = el("span", { class: "pill " + (c.ready ? "good" : "warn"), text: c.ready ? "datasheet found" : "needs its datasheet" });
  row.append(box, text, el("span", { class: "pill", text: c.source === "golden" ? "golden part" : "my case" }), tag);
  if (!c.ready && c.source === "golden") {
    const b = el("button", { type: "button", class: "small-btn", text: "Choose the PDF…" });
    b.addEventListener("click", (e) => {
      e.preventDefault();
      evalPickPdf(c);
    });
    row.append(b);
  }
  if (c.source === "mine" || (c.pdf && c.source === "golden" && c.ready)) {
    const b = el("button", { type: "button", class: "ghost small-btn", text: c.source === "mine" ? "Remove" : "Forget PDF" });
    b.addEventListener("click", (e) => {
      e.preventDefault();
      evalRemoveCase(c);
    });
    row.append(b);
  }
  return row;
}

// ---- Results ------------------------------------------------------------------------------

function resultRow(name, r, row) {
  const tr = el("tr");
  tr.append(el("td", { text: name }));
  if (!r) {
    const live = row && row.state === "running" ? "working: " + (EV_STAGE[row.stage] || row.stage) + "…" : "waiting";
    const td = el("td", { colspan: "6", class: "muted" });
    if (row && row.state === "running") td.append(el("span", { class: "mini-spin" }), " " + live);
    else td.textContent = live;
    tr.append(td);
    return [tr];
  }
  if (r.status === "skipped") {
    tr.append(el("td", { colspan: "6", class: "muted", text: "skipped: " + (r.failures[0] || "no datasheet") }));
    return [tr];
  }
  const cls = (a, b) => (b && a === b ? "good-text" : a / (b || 1) < 0.6 ? "bad-text" : "");
  tr.append(
    el("td", { class: cls(r.pins_correct, r.pins_expected), text: r.pins_correct + "/" + r.pins_expected + " · " + pct(r.pins_correct, r.pins_expected) }),
    el("td", { class: cls(r.dims_correct, r.dims_expected), text: r.dims_correct + "/" + r.dims_expected + " · " + pct(r.dims_correct, r.dims_expected) }),
    el("td", { class: cls(r.pads_correct, r.pads_expected), text: r.pads_correct + "/" + r.pads_expected + " · " + pct(r.pads_correct, r.pads_expected) }),
    el("td", { text: String(r.pins_extra) }),
    el("td", { text: r.tokens.toLocaleString() }),
    el("td", { text: r.seconds.toFixed(0) + " s" })
  );
  const out = [tr];
  const notes = [...r.failures.map((f) => "failed: " + f), ...r.mismatches];
  if (r.status === "failed" && !notes.length) notes.push("no part came out");
  if (notes.length) {
    const d = el("tr", { class: "detail" });
    const td = el("td", { colspan: "7" });
    notes.forEach((n) => td.append(el("p", { class: "small " + (n.startsWith("failed") ? "err-text" : "muted"), text: n })));
    d.append(td);
    out.push(d);
  }
  return out;
}

function resultsTable(run, live) {
  const table = el("table", { class: "eval-table" });
  table.append(el("thead", {}, el("tr", {}, ...["Part", "Pins right", "Dimensions right", "Pads right", "Extra pins", "Tokens", "Time"].map((h) => el("th", { text: h })))));
  const body = el("tbody");
  if (live) {
    Object.entries(ev.rows).forEach(([name, row]) => resultRow(name, row.result, row).forEach((tr) => body.append(tr)));
  } else {
    run.cases.forEach((c) => resultRow(c.name, c, null).forEach((tr) => body.append(tr)));
    const t = run.totals;
    body.append(
      el("tr", { class: "total" }, el("td", { text: "All " + t.scored + " of " + t.cases }), el("td", { text: share(t.pinout) }), el("td", { text: share(t.footprint) }), el("td", { text: share(t.pads) }), el("td", { text: "" }), el("td", { text: t.tokens.toLocaleString() }), el("td", { text: t.seconds.toFixed(0) + " s" }))
    );
  }
  table.append(body);
  return table;
}

function delta(now, before) {
  if (before == null) return null;
  const d = Math.round((now - before) * 100);
  if (d === 0) return el("span", { class: "muted small", text: "same" });
  return el("span", { class: "small " + (d > 0 ? "good-text" : "bad-text"), text: (d > 0 ? "▲ +" : "▼ ") + d + " pts" });
}

function runRow(run, older) {
  const li = el("li", { class: "run-row" + (ev.shown === run.id ? " shown" : "") });
  const when = new Date(run.when);
  const head = el("div", { class: "run-head" });
  head.append(
    el("strong", { text: isNaN(when) ? run.when : when.toLocaleString() }),
    el("span", { class: "pill", title: "The code of the prompts used", text: "prompts " + run.prompts_fingerprint }),
    run.label ? el("span", { class: "muted", text: run.label }) : "",
    el("span", { class: "muted small", text: run.models.map((m) => m.model || m.name).filter((v, i, a) => a.indexOf(v) === i).join(", ") + (run.second_read ? " · read twice" : "") })
  );
  const nums = el("div", { class: "run-nums" });
  const prev = older ? older.totals : null;
  const stat = (label, now, before) => {
    const d = prev ? delta(now, before) : null;
    return el("span", { class: "run-stat" }, el("span", { class: "muted small", text: label }), el("strong", { text: share(now) }), d || "");
  };
  nums.append(stat("pins", run.totals.pinout, prev && prev.pinout), stat("dimensions", run.totals.footprint, prev && prev.footprint), stat("pads", run.totals.pads, prev && prev.pads), el("span", { class: "muted small", text: run.totals.tokens.toLocaleString() + " tokens · " + run.totals.seconds.toFixed(0) + " s" }));
  const actions = el("div", { class: "run-actions" });
  const show = el("button", { type: "button", class: "small-btn", text: ev.shown === run.id ? "Shown below" : "Show details" });
  show.disabled = ev.shown === run.id;
  show.addEventListener("click", () => {
    ev.shown = run.id;
    renderEvaluate();
  });
  const restore = el("button", { type: "button", class: "small-btn", text: "Use these prompts" });
  restore.title = "Put the prompts of this run back into the Extraction prompts tab";
  restore.addEventListener("click", () => {
    for (const [k, v] of Object.entries(run.prompts)) draft.prompts[k] = v;
    activePane = "prompts";
    renderSettings();
    updateDirty();
    show_message("Prompts of the run " + run.prompts_fingerprint + " put back. Save the settings to keep them.");
  });
  const del = el("button", { type: "button", class: "ghost small-btn", text: "Delete" });
  del.addEventListener("click", async () => {
    try {
      ev.runs = await invoke("eval_delete", { id: run.id });
      if (ev.shown === run.id) ev.shown = ev.runs[0] ? ev.runs[0].id : null;
      renderEvaluate();
    } catch (e) {
      evalMessage(String(e), "error");
    }
  });
  actions.append(show, restore, del);
  li.append(head, nums, actions);
  return li;
}

function show_message(text) {
  if (typeof show === "function") show(text, "ok");
}

// ---- The tab -------------------------------------------------------------------------------

function renderEvaluate() {
  const pane = $("pane-evaluate");
  if (!pane || activePane !== "evaluate") return;
  // Do not rebuild while the person types in this tab.
  if (pane.contains(document.activeElement) && document.activeElement.tagName === "INPUT" && document.activeElement.type === "text") return;
  if (!appSettings) return;
  if (ev.cases === null) {
    pane.replaceChildren(el("p", { class: "muted", text: "Loading…" }));
    evalLoad();
    return;
  }
  const scroll = window.scrollY;
  const parts = [];
  parts.push(
    el("p", { class: "muted", text: "Run the saved models and prompts on datasheets whose right answer is known, and see how many pins and package dimensions come out right. Every run is kept with the exact prompts and models, so you can compare changes and bring back the best prompts. Evaluation sends the datasheet text to your models (about as much as building each part), and it ignores the answer cache so every run is honest." })
  );
  const used = [["Pin table", appSettings.pin_tables], ["Package", appSettings.drawings], ["Part number", appSettings.metadata]].map(([l, n]) => l + ": " + (n || (appSettings.models[0] && appSettings.models[0].name) || "no model"));
  parts.push(el("p", { class: "muted small", text: "Models, " + used.join(" · ") + ". Saved prompt set " + (appSettings.prompts_fingerprint || "") + (dirty() ? ". You have unsaved changes: they are saved first when you run." : ".") }));

  if (ev.error) parts.push(el("p", { class: "message error", text: ev.error }));

  // Cases
  const casesBox = el("section", { class: "panel eval-box" });
  casesBox.append(el("h4", { text: "Cases" }));
  if (!ev.cases.length) casesBox.append(el("p", { class: "muted", text: "No golden parts here. Add a part you have reviewed, with its datasheet, as your own case." }));
  ev.cases.forEach((c) => casesBox.append(caseRow(c)));
  if (ev.cases.some((c) => c.source === "golden" && !c.ready)) {
    casesBox.append(el("p", { class: "muted small", text: "The datasheets of the golden parts are not part of Kirpitch (they belong to their makers). Choose a PDF you downloaded, or run cargo xtask fetch-datasheets." }));
  }
  if (ev.adding) {
    const form = el("div", { class: "case-form" });
    const name = textInput(ev.add.name, (v) => (ev.add.name = v.trim()), { placeholder: "my-part" });
    const hint = textInput(ev.add.hint, (v) => (ev.add.hint = v), { placeholder: "SOIC-8, pinout on page 3" });
    const save = el("button", { type: "button", class: "primary", text: "Add case" });
    save.addEventListener("click", evalSaveCase);
    const cancel = el("button", { type: "button", class: "ghost", text: "Cancel" });
    cancel.addEventListener("click", () => {
      ev.adding = false;
      renderEvaluate();
    });
    form.append(
      pathRow("The right answer", "spec", "spec", "A part you have reviewed and approved. It is what the models should produce."),
      pathRow("Its datasheet", "pdf", "pdf", "The PDF the part was made from."),
      el("div", { class: "grid2" }, field("Name", name), field("Hint (optional)", hint, "Same as the hint of the datasheet dialog.")),
      el("div", { class: "pick-row" }, save, cancel)
    );
    casesBox.append(form);
  } else {
    const add = el("button", { type: "button", class: "add-model-btn", text: "+ Add my own case" });
    add.addEventListener("click", () => {
      ev.adding = true;
      renderEvaluate();
    });
    casesBox.append(add);
  }
  parts.push(casesBox);

  // Run
  const runBox = el("div", { class: "eval-run" });
  const second = el("input", { type: "checkbox" });
  second.checked = ev.second;
  second.disabled = ev.running;
  second.addEventListener("change", () => (ev.second = second.checked));
  const secondLabel = el("label", { class: "check" }, second, el("span", { text: "Read the pin table twice" }));
  const label = textInput(ev.label, (v) => (ev.label = v), { placeholder: "A note for the history, for example: shorter pin prompt" });
  label.disabled = ev.running;
  const go = el("button", { type: "button", class: "primary", text: ev.running ? "Running…" : "Run evaluation" });
  go.disabled = ev.running;
  go.addEventListener("click", evalRun);
  runBox.append(secondLabel, el("div", { class: "eval-note" }, label), go);
  if (ev.running) {
    const stop = el("button", { type: "button", class: "ghost", text: "Stop after this case" });
    stop.addEventListener("click", () => {
      invoke("eval_stop", {});
      stop.disabled = true;
    });
    runBox.append(stop);
  }
  parts.push(runBox);

  // Results
  if (ev.running) {
    const box = el("section", { class: "panel eval-box" }, el("h4", { text: "Running" }), resultsTable(null, true));
    parts.push(box);
  } else if (ev.shown && ev.runs) {
    const run = ev.runs.find((r) => r.id === ev.shown);
    if (run) parts.push(el("section", { class: "panel eval-box" }, el("h4", { text: "Results of " + new Date(run.when).toLocaleString() + " · prompts " + run.prompts_fingerprint }), resultsTable(run, false)));
  }

  // History
  if (ev.runs && ev.runs.length) {
    const list = el("ol", { class: "run-list" });
    ev.runs.forEach((r, i) => list.append(runRow(r, ev.runs[i + 1])));
    parts.push(el("section", { class: "panel eval-box" }, el("h4", { text: "History" }), el("p", { class: "muted small", text: "Newest first. The arrows compare each run with the one before it." }), list));
  } else if (!ev.running) {
    parts.push(el("p", { class: "muted", text: "No runs yet." }));
  }
  pane.replaceChildren(...parts);
  window.scrollTo(0, scroll);
}
