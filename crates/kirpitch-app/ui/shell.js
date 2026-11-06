// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// The frame of the part view: the steps (pinout, footprint, 3D model, export),
// the findings of the step you are on, the bar with Approve, the way home, and
// the keyboard. The tables, drawings and datasheet pane are in app.js.

const STEPS = [
  { id: "pinout", gate: "pinout", label: "Pinout", hint: "Check every pin" },
  { id: "footprint", gate: "footprint", label: "Footprint", hint: "Check the pads" },
  { id: "3d", gate: "3d", label: "3D model", hint: "Check the fit" },
  { id: "export", gate: null, label: "Export", hint: "Write the files" },
];

let activeStep = 0;

const plural = (n, word) => n + " " + word + (n === 1 ? "" : "s");

function gateFor(view, i) {
  return STEPS[i].gate ? view.gates.find((g) => g.name === STEPS[i].gate) : null;
}

function earlierApproved(view, i) {
  return view.gates.slice(0, i).every((g) => g.status === "approved");
}

function firstOpenStep(view) {
  const i = view.gates.findIndex((g) => g.status !== "approved");
  return i < 0 ? STEPS.length - 1 : i;
}

// ---- Status that the start screen shows for recent parts ----------------------

function rememberProgress(view) {
  try {
    const done = view.gates.filter((g) => g.status === "approved").length;
    localStorage.setItem("kirpitch.progress:" + view.path, done + "/" + view.gates.length);
  } catch {
    // Only a convenience.
  }
}

function progressOf(path) {
  try {
    return localStorage.getItem("kirpitch.progress:" + path);
  } catch {
    return null;
  }
}

// ---- The steps rail, progress and the bar -----------------------------------------

function stepStatus(view, i) {
  const g = gateFor(view, i);
  if (!g) return view.gates.every((x) => x.status === "approved") ? "ready" : "optional";
  if (g.status === "approved") return "approved";
  if (g.errors > 0) return "errors";
  if (!earlierApproved(view, i)) return "waiting";
  return g.status === "stale" ? "stale" : "todo";
}

function stepNote(view, i) {
  const g = gateFor(view, i);
  if (!g) return STEPS[i].hint;
  if (g.status === "approved") return "Approved";
  if (g.errors > 0) return plural(g.errors, "error") + " to fix";
  if (g.status === "stale") return "Changed, approve again";
  if (!earlierApproved(view, i)) return "After the step before";
  return g.warnings ? plural(g.warnings, "warning") : STEPS[i].hint;
}

// What each step looked like the last time, so a step that just changed pops.
let lastStatuses = { path: null, list: [] };

function renderRail(view) {
  const rail = $("steps");
  const before = lastStatuses.path === view.path ? lastStatuses.list : null;
  lastStatuses = { path: view.path, list: view.gates.map((g) => g.status) };
  rail.replaceChildren(
    ...STEPS.map((s, i) => {
      const status = stepStatus(view, i);
      const changed = before && view.gates[i] && before[i] !== view.gates[i].status;
      const mark = el("span", { class: "mark" + (changed ? " pop" : ""), "aria-hidden": "true", text: status === "approved" ? "✓" : String(i + 1) });
      const item = el(
        "button",
        { type: "button", class: "step-item " + status + (i === activeStep ? " current" : ""), "aria-current": i === activeStep ? "step" : "false" },
        mark,
        el("span", { class: "label" }, el("strong", { text: s.label }), el("span", { class: "note", text: stepNote(view, i) }))
      );
      item.addEventListener("click", () => setStep(i));
      return item;
    })
  );
  const bar = $("progress");
  bar.setAttribute("aria-label", view.gates.filter((g) => g.status === "approved").length + " of " + view.gates.length + " stages approved");
  bar.replaceChildren(
    ...view.gates.map((g) => el("span", { class: "seg " + (g.status === "approved" ? "done" : g.errors ? "err" : "") , title: g.name + ": " + g.status })),
    el("span", { class: "progress-text", text: view.gates.filter((g) => g.status === "approved").length + " of " + view.gates.length + " approved" })
  );
}

async function approveActive() {
  if (!current) return;
  const gate = gateFor(current, activeStep);
  if (!gate) return;
  const note = $("step-note");
  const was = activeStep;
  try {
    const next = await invoke("approve", { path: current.path, stage: gate.name, note: note && note.value ? note.value : null });
    render(next, true);
    show("Approved " + STEPS[was].label + ".", "ok");
    if (was < STEPS.length - 1) setStep(was + 1);
  } catch (e) {
    show(String(e), "error");
  }
}

function renderBar(view) {
  const bar = $("stepbar");
  const i = activeStep;
  const step = STEPS[i];
  const gate = gateFor(view, i);
  const back = el("button", { type: "button", class: "ghost", text: i === 0 ? "Home" : "← " + STEPS[i - 1].label });
  back.addEventListener("click", () => (i === 0 ? goHome() : setStep(i - 1)));
  const left = el("div", { class: "bar-left" }, back);
  const right = el("div", { class: "bar-right" });
  const info = el("div", { class: "bar-info" });
  if (!gate) {
    info.append(el("strong", { text: "Export" }), el("span", { class: "muted", text: " Write the KiCad files with the form above." }));
  } else {
    const earlier = earlierApproved(view, i);
    const blocked = gate.errors > 0 || !earlier;
    info.append(el("strong", { text: step.label }), " ");
    if (gate.status === "approved") {
      info.append(el("span", { class: "ok-text", text: "Approved ✓" }), el("span", { class: "muted", text: gate.approved_at ? " " + gate.approved_at : "" }));
    } else if (!earlier) {
      info.append(el("span", { class: "muted", text: "Approve " + STEPS[view.gates.findIndex((g) => g.status !== "approved")].label + " first." }));
    } else if (gate.errors > 0) {
      info.append(el("span", { class: "err-text", text: "Fix " + plural(gate.errors, "error") + " first." }));
    } else {
      info.append(el("span", { class: "muted", text: gate.warnings ? plural(gate.warnings, "warning") + " to look at, then approve." : "Looks good? Approve it." }));
    }
    if (gate.status !== "approved") {
      const note = el("input", { type: "text", id: "step-note", placeholder: "Note (optional)", "aria-label": "Approval note" });
      const approve = el("button", { type: "button", class: "primary", text: "Approve " + step.label });
      approve.disabled = blocked;
      if (blocked) approve.title = gate.errors > 0 ? "Fix the errors first" : "Approve the step before first";
      approve.addEventListener("click", approveActive);
      note.addEventListener("keydown", (ev) => {
        if (ev.key === "Enter" && !blocked) approveActive();
      });
      right.append(note, approve);
    } else if (gate.note) {
      info.append(el("span", { class: "muted", text: " “" + gate.note + "”" }));
    }
  }
  if (i < STEPS.length - 1 && (!gate || gate.status === "approved")) {
    const next = el("button", { type: "button", class: "primary", text: "Next: " + STEPS[i + 1].label + " →" });
    next.addEventListener("click", () => setStep(i + 1));
    right.append(next);
  } else if (i < STEPS.length - 1) {
    const skip = el("button", { type: "button", class: "ghost", text: "Skip ahead →" });
    skip.addEventListener("click", () => setStep(i + 1));
    right.append(skip);
  }
  bar.replaceChildren(left, info, right);
}

// ---- Findings of the step you are on ----------------------------------------------------

function pinNumberOfPath(view, path) {
  let m = /^pins\[(\d+)\]/.exec(path);
  if (m && view.pins[Number(m[1])]) return view.pins[Number(m[1])].number;
  m = /^land_pattern\.pads\[(\d+)\]/.exec(path);
  if (m && view.pads[Number(m[1])]) return view.pads[Number(m[1])].number;
  return null;
}

function renderFindings(view) {
  const gate = STEPS[activeStep].gate;
  const list = view.findings
    .filter((f) => !gate || f.gate === gate)
    .sort((a, b) => (a.level === b.level ? 0 : a.level === "error" ? -1 : 1));
  const errors = list.filter((f) => f.level === "error").length;
  const panel = $("findings-panel");
  panel.hidden = !gate;
  panel.className = "findings-panel" + (errors ? " has-errors" : list.length ? " has-warnings" : " clean");
  $("findings-count").textContent = list.length ? "(" + list.length + ")" : "";
  const box = $("view-findings");
  if (!list.length) {
    box.replaceChildren(typeof withMascot === "function" ? withMascot("No findings. Suspiciously perfect.") : el("p", { class: "muted", text: "No findings." }));
    return;
  }
  const lead = errors
    ? "Fix the errors before you can approve this step."
    : "Nothing blocks you. Have a look at these, then approve.";
  box.replaceChildren(
    el("p", { class: "lead", text: lead }),
    ...list.map((f) => {
      const row = el(
        "div",
        { class: "finding " + f.level },
        el("span", { class: "lvl " + f.level, text: f.level }),
        el("div", { class: "msg" }, el("div", { text: f.message + (f.overridden_by ? " — accepted: " + f.overridden_by : "") }), el("div", { class: "path", text: f.check + " · " + f.path }))
      );
      const pin = pinNumberOfPath(view, f.path);
      if (pin !== null) {
        row.classList.add("jump");
        row.tabIndex = 0;
        row.title = "Show it";
        const go = () => {
          selectPin(pin, true);
          const target = document.querySelector('tr[data-pin="' + esc(pin) + '"]');
          if (target) target.scrollIntoView({ block: "center", behavior: "smooth" });
        };
        row.addEventListener("click", go);
        row.addEventListener("keydown", (ev) => {
          if (ev.key === "Enter") go();
        });
      }
      return row;
    })
  );
}

// ---- The export step ----------------------------------------------------------------------

function renderApprovals(view) {
  $("approvals").replaceChildren(
    ...view.gates.map((g, i) => {
      const ok = g.status === "approved";
      const li = el("li", { class: ok ? "done" : "todo" }, el("span", { class: "mark", text: ok ? "✓" : "•" }), el("span", { text: STEPS[i].label + (ok ? " approved" : g.status === "stale" ? " changed since it was approved" : " not approved yet") }));
      if (!ok) {
        const go = el("button", { type: "button", class: "ghost small-btn", text: "Review" });
        go.addEventListener("click", () => setStep(i));
        li.append(go);
      }
      return li;
    }),
    ...(view.gates.every((g) => g.status === "approved")
      ? []
      : [el("li", { class: "note" }, "You can write the files anyway, but approving every step first is the safe way.")])
  );
}

// ---- Moving between steps -----------------------------------------------------------------

function setStep(i) {
  activeStep = Math.min(Math.max(i, 0), STEPS.length - 1);
  STEPS.forEach((s, k) => {
    $("step-" + s.id).hidden = k !== activeStep;
  });
  $("shell").dataset.step = STEPS[activeStep].id;
  $("sheet").hidden = activeStep > 1;
  if (current) {
    renderRail(current);
    renderFindings(current);
    renderBar(current);
    renderApprovals(current);
  }
  renderCrumbs();
  const stage = $("stage");
  stage.classList.remove("swap");
  void stage.offsetWidth;
  stage.classList.add("swap");
  window.scrollTo({ top: 0, behavior: "auto" });
}

function renderCrumbs() {
  const box = $("crumbs");
  if (!current || $("work").hidden) {
    box.replaceChildren();
    return;
  }
  const home = el("button", { type: "button", class: "crumb", text: "Home" });
  home.addEventListener("click", goHome);
  box.replaceChildren(home, el("span", { class: "sep", "aria-hidden": "true", text: "›" }), el("span", { class: "crumb here", text: current.mpn }), el("span", { class: "sep", "aria-hidden": "true", text: "›" }), el("span", { class: "crumb here", text: STEPS[activeStep].label }));
}

function goHome() {
  current = null;
  selected = null;
  $("work").hidden = true;
  $("empty").hidden = false;
  $("btn-copy").disabled = true;
  $("btn-close").disabled = true;
  renderCrumbs();
  if (typeof renderRecentList === "function") renderRecentList();
  window.scrollTo({ top: 0, behavior: "auto" });
}

function render(view, keepSheet) {
  const newPart = !current || current.path !== view.path;
  current = view;
  $("empty").hidden = true;
  $("work").hidden = false;
  $("mpn").textContent = view.mpn;
  $("part-line").textContent = [view.manufacturer, view.description].filter(Boolean).join(" · ");
  $("package-name").textContent = view.package_name;
  if (newPart) activeStep = firstOpenStep(view);
  renderPinout(view);
  renderFootprint(view);
  // The drawings are made by our own Rust code from numbers and escaped text.
  $("svg").innerHTML = view.footprint_svg;
  $("symbol").innerHTML = view.symbol_svg || '<p class="empty-note">Fix the errors of the pinout to see the symbol.</p>';
  $("model-summary").textContent = view.model_summary;
  const views = $("model-views");
  views.replaceChildren();
  view.model_views.forEach((v) => {
    const fig = el("figure", {}, el("figcaption", { text: v.name }));
    const holder = document.createElement("div");
    holder.innerHTML = v.svg;
    fig.append(holder);
    views.append(fig);
  });
  selected = null;
  rememberProgress(view);
  setStep(activeStep);
  if (newPart) {
    $("btn-close").disabled = false;
  }
  if (!keepSheet) {
    const saved = savedPdf(view.path);
    $("pdf-path").value = saved;
    if (saved) showPage(1).catch((e) => show(String(e), "error"));
  }
}

// ---- Menu, home and the keyboard ---------------------------------------------------------------

(() => {
  const btn = $("menu-btn");
  const pop = $("menu-pop");
  const close = () => {
    pop.hidden = true;
    btn.setAttribute("aria-expanded", "false");
  };
  btn.addEventListener("click", (ev) => {
    ev.stopPropagation();
    pop.hidden = !pop.hidden;
    btn.setAttribute("aria-expanded", String(!pop.hidden));
  });
  pop.addEventListener("click", (ev) => {
    if (ev.target.closest("button")) close();
  });
  document.addEventListener("click", (ev) => {
    if (!pop.hidden && !ev.target.closest(".menu")) close();
  });
  $("home").addEventListener("click", goHome);
  $("btn-close").addEventListener("click", goHome);

  // Capture phase: the plain Escape of app.js first clears a selected pin, so
  // only an Escape with nothing to clear goes home.
  document.addEventListener(
    "keydown",
    (ev) => {
      const typing = ev.target.closest && ev.target.closest("input, select, textarea");
      const dialogOpen = document.querySelector("dialog[open]");
      if (ev.key === "Escape") {
        if (!pop.hidden) return close();
        if (!dialogOpen && !typing && current && selected === null) goHome();
        return;
      }
      if ((ev.ctrlKey || ev.metaKey) && !dialogOpen) {
        const key = ev.key.toLowerCase();
        if (key === "o") {
          ev.preventDefault();
          $("btn-open-file").click();
        } else if (key === "n") {
          ev.preventDefault();
          $("btn-new").click();
        }
      }
      if (!typing && !dialogOpen && current && ev.altKey) {
        if (ev.key === "ArrowRight") setStep(activeStep + 1);
        if (ev.key === "ArrowLeft") setStep(activeStep - 1);
      }
    },
    true
  );
})();
