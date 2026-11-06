// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// Shows how reading a datasheet goes: each step with its state and time, the
// time since the start, and an estimate of what is left. The estimate uses the
// times of your earlier runs on this computer (kept in the browser storage of
// the window), else rough defaults, and says which one it is.

const BUILD_STEPS = [
  { id: "read", label: "Read the text of the PDF", guess: 2 },
  { id: "metadata", label: "Part number, maker and description", guess: 12 },
  { id: "pins", label: "Pin table", guess: 50 },
  { id: "package", label: "Package drawing", guess: 30 },
  { id: "assemble", label: "Check that the pieces fit", guess: 1 },
  { id: "save", label: "Save the part", guess: 2 },
];
const HISTORY_KEY = "kirpitch.stageSeconds";

let build = null; // the run now on the screen

function buildRunning() {
  return !!build && !build.finished;
}

function loadHistory() {
  try {
    return JSON.parse(localStorage.getItem(HISTORY_KEY) || "{}") || {};
  } catch (e) {
    return {};
  }
}

function saveHistory(h) {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(h));
  } catch (e) {
    // Without storage the estimate falls back to the defaults.
  }
}

function fmtTime(seconds) {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return s + " s";
  return Math.floor(s / 60) + " min " + String(s % 60).padStart(2, "0") + " s";
}

function fmtLeft(seconds) {
  const s = Math.max(0, seconds);
  if (s < 8) return "a few seconds";
  if (s < 50) return "about " + Math.round(s / 5) * 5 + " seconds";
  const m = Math.round(s / 30) / 2;
  return "about " + (m === 1 ? "1 minute" : m + " minutes");
}

function buildBegin(second) {
  const history = loadHistory();
  build = {
    started: Date.now(),
    finished: false,
    ok: false,
    history,
    known: BUILD_STEPS.some((s) => history[s.id] != null),
    steps: BUILD_STEPS.map((s) => ({
      ...s,
      label: s.id === "pins" && second ? "Pin table, read twice" : s.label,
      state: "pending",
      detail: "",
      at: 0,
      took: 0,
      expect: history[s.id] != null ? history[s.id] : s.guess * (s.id === "pins" && second ? 1.8 : 1),
    })),
  };
  $("ds-progress").hidden = false;
  $("ds-dialog").classList.add("running");
  $("pg-title").textContent = "Reading the datasheet";
  paintBuild();
  build.timer = setInterval(paintBuild, 250);
}

// A report from the Rust side.
function buildEvent(p) {
  if (!build || build.finished) return;
  const s = build.steps.find((x) => x.id === p.stage);
  if (!s) return;
  const now = Date.now();
  if (p.state === "started") {
    s.state = "running";
    s.at = now;
  } else {
    if (!s.at) s.at = now;
    s.took = (now - s.at) / 1000;
    s.state = p.state === "done" ? "done" : p.state === "failed" ? "failed" : "skipped";
  }
  s.detail = p.detail || "";
  paintBuild();
}

function buildEnd(ok, problems, error) {
  if (!build) return;
  clearInterval(build.timer);
  build.finished = true;
  build.ok = ok && !problems;
  // A part that did not work goes back to the form so it can be tried again.
  if (!ok) $("ds-dialog").classList.remove("running");
  const total = (Date.now() - build.started) / 1000;
  // Anything still marked as running did not finish.
  build.steps.forEach((s) => {
    if (s.state === "running") {
      s.state = "failed";
      s.took = (Date.now() - s.at) / 1000;
      if (error) s.detail = error;
    }
  });
  // Learn from the steps that really ran (a step answered from the cache takes
  // no time and would make the next estimate too hopeful).
  const history = { ...build.history };
  for (const s of build.steps) {
    if (s.state === "done" && s.took >= 1) history[s.id] = history[s.id] != null ? history[s.id] * 0.4 + s.took * 0.6 : s.took;
  }
  saveHistory(history);
  $("pg-title").textContent = build.ok ? "Done in " + fmtTime(total) : ok ? "Finished with problems in " + fmtTime(total) : "Stopped after " + fmtTime(total);
  paintBuild(total);
}

function remainingSeconds() {
  let left = 0;
  const now = Date.now();
  for (const s of build.steps) {
    if (s.state === "pending") left += s.expect;
    else if (s.state === "running") {
      const spent = (now - s.at) / 1000;
      left += Math.max(s.expect - spent, s.expect * 0.15);
    }
  }
  return left;
}

const STEP_MARK = { pending: "", running: "", done: "✓", failed: "✗", skipped: "–" };
const STEP_WORD = { pending: "waiting", running: "working", done: "done", failed: "failed", skipped: "not needed" };

function paintBuild(frozenTotal) {
  if (!build) return;
  const now = Date.now();
  const total = frozenTotal != null ? frozenTotal : (now - build.started) / 1000;
  $("pg-time").textContent = fmtTime(total);

  // The bar: finished steps count in full, the running one by how far past its
  // expected time it is (never to the end until it is done).
  const weight = build.steps.reduce((a, s) => a + s.expect, 0);
  let doneW = 0;
  for (const s of build.steps) {
    if (s.state === "done" || s.state === "skipped" || s.state === "failed") doneW += s.expect;
    else if (s.state === "running") doneW += s.expect * Math.min(0.9, (now - s.at) / 1000 / s.expect);
  }
  const pct = build.finished ? 100 : Math.min(97, (doneW / weight) * 100);
  $("pg-fill").style.width = pct.toFixed(1) + "%";
  $("pg-fill").className = build.finished ? (build.ok ? "ok" : "bad") : "";
  document.querySelector(".pg-bar").setAttribute("aria-valuenow", String(Math.round(pct)));

  const eta = $("pg-eta");
  if (build.finished) {
    eta.textContent = "";
  } else {
    const running = build.steps.find((s) => s.state === "running");
    const over = running && (now - running.at) / 1000 > running.expect * 1.5;
    eta.textContent = (over ? "Taking longer than usual. " : "") + fmtLeft(remainingSeconds()) + " left, " + (build.known ? "going by your earlier runs." : "a rough guess: this is the first run.");
  }

  const list = $("pg-steps");
  const items = build.steps.map((s) => {
    let word = STEP_WORD[s.state];
    let time = "";
    if (s.state === "running") time = fmtTime((now - s.at) / 1000);
    else if (s.state === "done" || s.state === "failed") time = s.took < 1 ? "< 1 s" : fmtTime(s.took);
    let li = list.querySelector('[data-step="' + s.id + '"]');
    if (!li) {
      li = el("li", { "data-step": s.id });
      li.append(el("span", { class: "pg-mark" }), el("span", { class: "pg-text" }, el("span", { class: "pg-label" }), el("span", { class: "pg-detail muted small" })), el("span", { class: "pg-took" }));
      list.append(li);
    }
    li.className = "pg-step " + s.state;
    li.querySelector(".pg-mark").textContent = STEP_MARK[s.state];
    li.querySelector(".pg-label").textContent = s.label;
    li.querySelector(".pg-detail").textContent = s.detail || (s.state === "pending" ? "" : word);
    li.querySelector(".pg-took").textContent = time;
    return li;
  });
  void items;
}

(() => {
  const tauri = window.__TAURI__;
  if (tauri && tauri.event) tauri.event.listen("build-progress", (event) => buildEvent(event.payload));
})();
