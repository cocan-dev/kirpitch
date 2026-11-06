// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// Shows how reading a datasheet goes: the steps in order, the one being worked
// on spinning, finished ones with a green check and their time, and a clock
// that counts up. The Rust side keeps a log of what it has done; this asks for
// the new entries a few times a second.

const BUILD_STEPS = [
  { id: "read", label: "Read the text of the PDF" },
  { id: "metadata", label: "Part number, maker and description" },
  { id: "pins", label: "Pin table" },
  { id: "package", label: "Package outline" },
  { id: "footprint", label: "Recommended footprint" },
  { id: "assemble", label: "Check that the pieces fit" },
  { id: "save", label: "Save the part" },
];

let build = null; // the run now on the screen

function buildRunning() {
  return !!build && !build.finished;
}

function fmtTime(seconds) {
  const s = Math.max(0, Math.floor(seconds));
  if (s < 60) return s + " s";
  return Math.floor(s / 60) + " min " + String(s % 60).padStart(2, "0") + " s";
}

function startStep(s) {
  s.state = "running";
  s.at = Date.now();
}

function buildBegin(second) {
  build = {
    started: Date.now(),
    finished: false,
    ok: false,
    seen: 0,
    steps: BUILD_STEPS.map((s) => ({
      ...s,
      label: s.id === "pins" && second ? "Pin table, read twice" : s.label,
      state: "pending",
      detail: "",
      at: 0,
      took: 0,
    })),
  };
  // Something spins from the first moment.
  startStep(build.steps[0]);
  $("ds-progress").hidden = false;
  $("ds-dialog").classList.add("running");
  $("pg-title").textContent = "Reading the datasheet";
  ["gd-back", "gd-next", "gd-skip"].forEach((id) => ($(id).hidden = true));
  resetLog();
  paintBuild();
  build.clock = setInterval(paintBuild, 250);
  build.poll = setInterval(pollBuild, 300);
}

async function pollBuild() {
  const run = build;
  if (!run || run.finished || run.polling) return;
  run.polling = true;
  try {
    const events = await invoke("build_progress", { since: run.seen });
    if (build !== run) return;
    run.seen += events.length;
    events.forEach(buildEvent);
  } catch (e) {
    // The next poll tries again.
  } finally {
    run.polling = false;
  }
}

// One report from the Rust side.
function buildEvent(p) {
  if (!build || build.finished) return;
  if (p.state === "call") {
    if (p.call) logCall(p.call);
    return;
  }
  const i = build.steps.findIndex((x) => x.id === p.stage);
  if (i < 0) return;
  const s = build.steps[i];
  const now = Date.now();
  if (p.state === "started") {
    startStep(s);
  } else {
    if (!s.at) s.at = now;
    s.took = (now - s.at) / 1000;
    s.state = p.state === "done" ? "done" : p.state === "failed" ? "failed" : "skipped";
    // The next step that has not started is the one being worked on now.
    const next = build.steps.slice(i + 1).find((x) => x.state === "pending");
    if (next && !build.steps.some((x) => x.state === "running")) startStep(next);
  }
  s.detail = p.detail || "";
  paintBuild();
}

async function buildEnd(ok, problems, error) {
  const run = build;
  if (!run) return;
  clearInterval(run.poll);
  // Pick up the last reports before closing the run.
  try {
    const events = await invoke("build_progress", { since: run.seen });
    run.seen += events.length;
    events.forEach(buildEvent);
  } catch (e) {
    // Nothing more to show.
  }
  clearInterval(run.clock);
  run.finished = true;
  run.ok = ok && !problems;
  // A part that did not work goes back to the form so it can be tried again.
  if (!ok) {
    $("ds-dialog").classList.remove("running");
    if (typeof gdShow === "function") gdShow(5); // back to the last step to change something and try again
  }
  const now = Date.now();
  for (const s of run.steps) {
    if (s.state === "running") {
      s.state = "failed";
      s.took = (now - s.at) / 1000;
      if (error) s.detail = error;
    }
  }
  const total = (now - run.started) / 1000;
  $("pg-title").textContent = run.ok ? "Done in " + fmtTime(total) : ok ? "Finished with problems in " + fmtTime(total) : "Stopped after " + fmtTime(total);
  paintBuild(total);
}

const STEP_MARK = { pending: "", running: "", done: "✓", failed: "✗", skipped: "–" };
const STEP_WORD = { pending: "", running: "working…", done: "", failed: "failed", skipped: "not needed" };

function paintBuild(frozenTotal) {
  if (!build) return;
  const now = Date.now();
  tickLog(now);
  const total = frozenTotal != null ? frozenTotal : (now - build.started) / 1000;
  $("pg-time").textContent = fmtTime(total);

  const list = $("pg-steps");
  for (const s of build.steps) {
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
    li.querySelector(".pg-detail").textContent = s.detail || STEP_WORD[s.state];
    li.querySelector(".pg-took").textContent = time;
  }
}

// ---- The model output ---------------------------------------------------------------------------
//
// Every call to a model is shown as it happens: what is asked (how much, which
// model), then what comes back, the thinking when the server returns it, and why
// an answer was not accepted. This panel can be folded; its state is remembered.

const VERBOSE_KEY = "kirpitch.verboseOutput";
let logCalls = new Map(); // "label#attempt" -> { box, head, body, sentAt }

function resetLog() {
  logCalls = new Map();
  $("pg-log").replaceChildren();
  $("pg-verbose-count").textContent = "";
  try {
    $("pg-verbose").open = localStorage.getItem(VERBOSE_KEY) === "open";
  } catch (e) {
    // The panel starts folded.
  }
}

$("pg-verbose").addEventListener("toggle", () => {
  try {
    localStorage.setItem(VERBOSE_KEY, $("pg-verbose").open ? "open" : "closed");
  } catch (e) {
    // Nothing to remember it with.
  }
});

$("pg-copy").addEventListener("click", async () => {
  const text = [...$("pg-log").querySelectorAll(".call")].map((c) => c.innerText).join("\n\n");
  try {
    await navigator.clipboard.writeText(text);
    $("pg-copy").textContent = "Copied";
  } catch (e) {
    $("pg-copy").textContent = "Select the text and copy it";
  }
  setTimeout(() => ($("pg-copy").textContent = "Copy all"), 2000);
});

function nice(n) {
  return Number(n).toLocaleString();
}

function callEntry(c) {
  const key = c.label + "#" + c.attempt;
  let e = logCalls.get(key);
  if (!e) {
    const box = el("div", { class: "call" });
    const head = el("div", { class: "call-head" });
    const body = el("div", { class: "call-body" });
    box.append(head, body);
    const log = $("pg-log");
    const atBottom = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
    log.append(box);
    if (atBottom) log.scrollTop = log.scrollHeight;
    e = { box, head, body, sentAt: 0 };
    logCalls.set(key, e);
    $("pg-verbose-count").textContent = "(" + logCalls.size + " call" + (logCalls.size === 1 ? "" : "s") + ")";
  }
  return e;
}

function pre(text, cls) {
  return el("pre", { class: "call-text " + (cls || ""), text });
}

function logCall(c) {
  const e = callEntry(c);
  const title = c.label + (c.attempt > 1 ? ", try " + c.attempt + " (asking again)" : "");
  if (c.phase === "sent") {
    e.sentAt = Date.now();
    e.box.className = "call running";
    e.head.textContent = "→ " + title + " · " + c.model + " · " + nice(c.prompt_chars) + " characters sent, answer up to " + nice(c.max_tokens) + " tokens";
    e.body.replaceChildren(el("p", { class: "muted small call-wait", text: "waiting for the model… 0 s" }));
    return;
  }
  if (c.phase === "rejected") {
    e.box.classList.add("rejected");
    e.body.append(el("p", { class: "call-problem small", text: "✗ not accepted: " + c.problem }));
    return;
  }
  e.sentAt = 0;
  if (c.phase === "failed") {
    e.box.className = "call failed";
    e.head.textContent = "✗ " + title + " · " + c.model + " · the call failed";
    e.body.replaceChildren(el("p", { class: "call-problem small", text: c.problem }));
    return;
  }
  if (c.phase === "cached") {
    e.box.className = "call cached";
    e.head.textContent = "← " + title + " · answer from the cache (no model was asked)";
    e.body.replaceChildren(pre(c.text));
    return;
  }
  // received
  e.box.className = "call done";
  const bits = [c.seconds.toFixed(0) + " s", nice(c.tokens_out) + " tokens out"];
  if (c.finish) bits.push("stopped: " + c.finish);
  e.head.textContent = "← " + title + " · " + c.model + " · " + bits.join(" · ");
  const parts = [];
  if (c.text.trim()) {
    parts.push(pre(c.text));
  } else {
    const why = c.thinking
      ? "The answer is empty. The model wrote " + nice(c.thinking.length) + " characters of thinking and no answer" + (c.finish === "length" ? ", then ran out of room." : ".")
      : "The answer is empty." + (c.finish === "length" ? " The model ran out of room (the server does not show its thinking)." : "");
    parts.push(el("p", { class: "call-empty", text: why }));
  }
  if (c.thinking) {
    const d = el("details", { class: "call-thinking" }, el("summary", { text: "Thinking (" + nice(c.thinking.length) + " characters)" }));
    d.append(pre(c.thinking, "dim"));
    parts.push(d);
  }
  e.body.replaceChildren(...parts);
}

// The seconds of calls that are waiting count up.
function tickLog(now) {
  for (const e of logCalls.values()) {
    if (!e.sentAt) continue;
    const w = e.body.querySelector(".call-wait");
    if (w) w.textContent = "waiting for the model… " + Math.floor((now - e.sentAt) / 1000) + " s";
  }
}
