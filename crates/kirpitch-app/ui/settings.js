// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// The settings screen: models and their keys, which model does which task,
// privacy, and the folders and defaults. The settings are saved in the same
// kirpitch.toml the command line reads; keys go to the system keychain.

let appSettings = null; // what the Rust side says is saved
let draft = null; // what is on the screen, possibly changed
let settingsReturn = null; // the part to go back to
let activePane = "models";
const testResults = {}; // model name -> result of the last test
const discovered = new WeakMap(); // draft model -> { state: "loading" | "done" | "error", models, error }

const PRESETS_MODELS = [
  { label: "Ollama on this computer", blurb: "Free, private. Models you have pulled.", name: "ollama-local", provider: "ollama", base_url: "", model: "", keychain: false, local: true },
  { label: "LM Studio on this computer", blurb: "Free, private. The local server of LM Studio.", name: "lm-studio", provider: "openai-compatible", base_url: "http://localhost:1234/v1", model: "", keychain: false, local: true },
  { label: "Ollama cloud (ollama.com)", blurb: "Big open models run by Ollama. Needs a key.", name: "ollama-cloud", provider: "ollama", base_url: "https://ollama.com", model: "", keychain: true, local: false },
  { label: "Anthropic Claude", blurb: "Claude models. Needs a key.", name: "claude", provider: "anthropic", base_url: "", model: "", keychain: true, local: false },
  { label: "OpenAI", blurb: "GPT models. Needs a key.", name: "openai", provider: "openai", base_url: "", model: "", keychain: true, local: false },
  { label: "OpenRouter", blurb: "Hundreds of models with one key.", name: "openrouter", provider: "openai-compatible", base_url: "https://openrouter.ai/api/v1", model: "", keychain: true, local: false },
  { label: "Mistral", blurb: "Mistral models. Needs a key.", name: "mistral", provider: "openai-compatible", base_url: "https://api.mistral.ai/v1", model: "", keychain: true, local: false },
  { label: "Google Gemini", blurb: "Gemini models. Needs a key.", name: "gemini", provider: "openai-compatible", base_url: "https://generativelanguage.googleapis.com/v1beta/openai", model: "", keychain: true, local: false },
  { label: "Another OpenAI-compatible server", blurb: "vLLM, llama.cpp, Azure and others. You give the address.", name: "my-server", provider: "openai-compatible", base_url: "", model: "", keychain: false, local: false },
];

const clone = (x) => JSON.parse(JSON.stringify(x));

// Rebuilds the draft from the saved settings and keeps what the screen knew
// about each model by name: its found list, whether its card was open.
function rebuildDraft(old) {
  const kept = new Map((old || []).map((m) => [m.name, { found: discovered.get(m), open: expanded.has(m), tried: triedAuto.has(m) }]));
  draft = toDraft(appSettings);
  for (const m of draft.models) {
    const k = kept.get(m.name);
    if (!k) continue;
    if (k.found) discovered.set(m, k.found);
    if (k.open) expanded.add(m);
    if (k.tried) triedAuto.add(m);
  }
}

function toDraft(view) {
  return {
    models: view.models.map((m) => ({ ...clone(m), old_name: m.name, new: false, newKey: "" })),
    pin_tables: view.pin_tables,
    drawings: view.drawings,
    metadata: view.metadata,
    local_only: view.local_only,
    parts: view.parts,
    output: view.output,
    datasheets: view.datasheets,
    kicad_target: view.kicad_target,
    fab_profile: view.fab_profile,
    pdfium_dir: view.pdfium_dir,
  };
}

function dirty() {
  if (!appSettings || !draft) return false;
  const a = JSON.stringify(toDraft(appSettings));
  return a !== JSON.stringify(draft);
}

function updateDirty() {
  const d = dirty();
  $("settings-save").disabled = !d;
  $("settings-discard").disabled = !d;
  $("settings-dirty").hidden = !d;
  $("btn-settings").classList.toggle("has-changes", d);
}

async function loadSettings() {
  try {
    appSettings = await invoke("get_settings", {});
    rebuildDraft(draft ? draft.models : []);
  } catch (e) {
    // Outside the app there are no settings.
  }
}

// ---- Showing and leaving -----------------------------------------------------------

async function showSettings() {
  settingsReturn = current;
  current = null;
  $("empty").hidden = true;
  $("work").hidden = true;
  $("settings").hidden = false;
  await loadSettingsIfClean();
  renderSettings();
  renderCrumbsSettings();
  window.scrollTo({ top: 0, behavior: "auto" });
}

async function loadSettingsIfClean() {
  if (!draft || !dirty()) await loadSettings();
}

function renderCrumbsSettings() {
  const box = $("crumbs");
  const home = el("button", { type: "button", class: "crumb", text: "Home" });
  home.addEventListener("click", goHome);
  box.replaceChildren(home, el("span", { class: "sep", "aria-hidden": "true", text: "›" }), el("span", { class: "crumb here", text: "Settings" }));
}

function leaveSettings() {
  $("settings").hidden = true;
  if (settingsReturn) {
    const view = settingsReturn;
    settingsReturn = null;
    render(view, true);
  } else {
    goHome();
  }
}

// ---- Building the screen -------------------------------------------------------------------

function field(label, input, help) {
  const wrap = el("label", { class: "f" }, el("span", { class: "lab", text: label }), input);
  if (help) wrap.append(el("span", { class: "help", text: help }));
  return wrap;
}

function textInput(value, onChange, opts = {}) {
  const i = el("input", { type: opts.type || "text", spellcheck: "false", autocomplete: "off" });
  if (opts.placeholder) i.placeholder = opts.placeholder;
  i.value = value || "";
  i.addEventListener("input", () => {
    onChange(i.value);
    updateDirty();
  });
  return i;
}

function selectInput(options, value, onChange) {
  const s = el("select");
  options.forEach(([v, label]) => {
    const o = el("option", { value: v, text: label });
    if (v === value) o.selected = true;
    s.append(o);
  });
  s.addEventListener("change", () => {
    onChange(s.value);
    updateDirty();
  });
  return s;
}

function providerInfo(id) {
  return appSettings.providers.find((p) => p.id === id) || { default_url: "", key_hint: "" };
}

// ---- Finding the models a server offers ---------------------------------------------

function money(x) {
  return x === 0 ? "free" : "$" + (x >= 100 ? x.toFixed(0) : x >= 1 ? x.toFixed(2).replace(/\.?0+$/, "") : x.toPrecision(2));
}

function ctxText(n) {
  return n >= 1000000 ? (n / 1000000).toFixed(n % 1000000 ? 1 : 0) + "M" : Math.round(n / 1000) + "k";
}

// What is known about a model, as a short line.
function foundLine(f) {
  const bits = [];
  if (f.label && f.label !== f.id) bits.push(f.label);
  if (f.price_in != null && f.price_out != null) bits.push(money(f.price_in) + " in / " + money(f.price_out) + " out per million tokens");
  if (f.context) bits.push(ctxText(f.context) + " context");
  if (f.vision === true) bits.push("reads images");
  return bits.join(" · ");
}

const listCache = new Map(); // "provider|address|name" -> { at, models }
const LIST_TTL_MS = 10 * 60 * 1000;
const triedAuto = new WeakSet(); // models whose list was asked for without a click
const pickers = new WeakMap(); // draft model -> the Model field now on the screen

// Redraws only the Model field of one card, so nothing else on the screen moves.
function refreshPicker(m) {
  const old = pickers.get(m);
  if (!old || !old.isConnected) return;
  const hadFocus = old.contains(document.activeElement);
  const fresh = modelPicker(m);
  old.replaceWith(fresh);
  if (hadFocus) {
    const input = fresh.querySelector("input");
    if (input) input.focus();
  }
}

function listKey(m) {
  return [m.provider, m.base_url, m.old_name || m.name].join("|");
}

function canAsk(m) {
  if (m.provider === "openai-compatible" && !m.base_url) return false;
  return !m.keychain || m.has_key || m.env_set || !!m.newKey;
}

async function discoverModels(m, force) {
  const key = listKey(m);
  const cached = listCache.get(key);
  if (!force && cached && Date.now() - cached.at < LIST_TTL_MS) {
    discovered.set(m, { state: "done", models: cached.models, error: "" });
    refreshPicker(m);
    return;
  }
  discovered.set(m, { state: "loading", models: [], error: "" });
  refreshPicker(m);
  try {
    const models = await invoke("find_models", {
      input: {
        name: m.name,
        old_name: m.old_name,
        provider: m.provider,
        base_url: m.base_url,
        api_key_env: m.api_key_env,
        api_key_header: m.api_key_header,
        keychain: m.keychain,
        new_key: m.newKey || "",
      },
    });
    listCache.set(key, { at: Date.now(), models });
    discovered.set(m, { state: "done", models, error: "" });
    if (!m.model && models.length === 1) {
      applyFound(m, models[0]);
      renderSettings();
      return;
    }
  } catch (e) {
    discovered.set(m, { state: "error", models: [], error: String(e) });
  }
  refreshPicker(m);
}

function applyFound(m, f) {
  m.model = f.id;
  if (f.price_in != null && f.price_out != null) {
    m.price_in = f.price_in;
    m.price_out = f.price_out;
  }
  if (f.vision != null) m.vision = f.vision;
  updateDirty();
}

// ---- A list you can search and pick from ----------------------------------------------

// A text box that opens the whole list when you click it, narrows the list while
// you type, and also takes a name that is not in the list. Up/Down, Enter and
// Esc work as in any combo box.
function combo(m, found) {
  const models = found && found.state === "done" ? found.models : [];
  const wrap = el("div", { class: "combo" });
  const id = "combo-" + Math.random().toString(36).slice(2);
  const input = el("input", { type: "text", spellcheck: "false", autocomplete: "off", role: "combobox", "aria-autocomplete": "list", "aria-expanded": "false", "aria-controls": id });
  input.placeholder = models.length ? "Search " + models.length + " models" : "Model name";
  input.value = m.model || "";
  const list = el("ul", { class: "combo-list", id, role: "listbox" });
  list.hidden = true;
  let shown = [];
  let active = -1;
  let filter = "";

  const close = () => {
    list.hidden = true;
    input.setAttribute("aria-expanded", "false");
    active = -1;
  };
  const choose = (f) => {
    applyFound(m, f);
    close();
    renderSettings();
  };
  const paint = () => {
    const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
    shown = models.filter((f) => {
      const hay = (f.id + " " + (f.label || "")).toLowerCase();
      return words.every((w) => hay.includes(w));
    });
    list.replaceChildren();
    if (!shown.length) {
      list.append(el("li", { class: "combo-empty", text: filter ? "No model matches. The name you typed will be used as it is." : "The server listed no models." }));
    }
    shown.slice(0, 200).forEach((f, i) => {
      const li = el("li", { role: "option", class: "combo-item" + (f.id === m.model ? " chosen" : ""), "aria-selected": String(i === active) });
      li.append(el("span", { class: "combo-id", text: f.id }));
      const line = foundLine(f);
      if (line) li.append(el("span", { class: "combo-line", text: line }));
      // mousedown, not click: the box loses focus first and would close the list.
      li.addEventListener("mousedown", (ev) => {
        ev.preventDefault();
        choose(f);
      });
      if (i === active) li.classList.add("active");
      list.append(li);
    });
    if (shown.length > 200) list.append(el("li", { class: "combo-empty", text: shown.length - 200 + " more: type to narrow the list." }));
    const on = list.querySelector(".active");
    if (on) on.scrollIntoView({ block: "nearest" });
  };
  const open = () => {
    if (!models.length) return;
    list.hidden = false;
    input.setAttribute("aria-expanded", "true");
    paint();
  };

  input.addEventListener("focus", () => {
    filter = "";
    open();
  });
  input.addEventListener("click", () => {
    if (list.hidden) {
      filter = "";
      open();
    }
  });
  input.addEventListener("input", () => {
    m.model = input.value.trim();
    filter = input.value;
    active = -1;
    open();
    updateDirty();
  });
  input.addEventListener("blur", () => {
    close();
    const f = models.find((x) => x.id === m.model);
    if (f && (m.price_in == null || m.price_out == null)) {
      applyFound(m, f);
      renderSettings();
    }
  });
  input.addEventListener("keydown", (ev) => {
    if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
      ev.preventDefault();
      if (list.hidden) {
        filter = "";
        open();
      }
      const n = Math.min(shown.length, 200);
      if (!n) return;
      active = ev.key === "ArrowDown" ? (active + 1) % n : (active - 1 + n) % n;
      paint();
    } else if (ev.key === "Enter" && !list.hidden && active >= 0 && shown[active]) {
      ev.preventDefault();
      choose(shown[active]);
    } else if (ev.key === "Escape" && !list.hidden) {
      ev.stopPropagation();
      close();
    }
  });
  wrap.append(input, list);
  return wrap;
}

function modelPicker(m) {
  const found = discovered.get(m);
  const row = el("div", { class: "pick-row" });
  const loading = !!(found && found.state === "loading");
  const refresh = el("button", { type: "button", class: found && found.state === "done" ? "ghost" : "", text: loading ? "Looking…" : found && found.state === "done" ? "Refresh list" : "Find models" });
  refresh.disabled = loading;
  refresh.addEventListener("click", () => discoverModels(m, true));
  row.append(combo(m, found), refresh);
  const wrap = field("Model", row);
  pickers.set(m, wrap);
  const note = el("span", { class: "help note" });
  wrap.append(note);
  if (!found) {
    note.textContent = canAsk(m) ? "Press Find models to see what this server offers." : m.keychain ? "Save the key first; then the list of models appears here." : "Press Find models to see what this server offers.";
  } else if (loading) {
    note.textContent = "Asking the server…";
  } else if (found.state === "error") {
    note.className = "help note err-text";
    note.textContent = found.error;
  } else if (found.state === "done") {
    const f = found.models.find((x) => x.id === m.model);
    let text = found.models.length + " models on this server.";
    if (f) {
      const line = foundLine(f);
      text = (line || "This model is on the server.") + (f.price_source === "OpenRouter list" ? " (price from the OpenRouter list; check your plan)" : "");
    } else if (m.model) {
      text = "This name is not in the server's list. It will be used as typed.";
      note.className = "help note warn-text";
    }
    note.textContent = text;
  }
  return wrap;
}

// ---- One model ---------------------------------------------------------------------------

const KEY_PLACES = {
  anthropic: "console.anthropic.com, under API keys",
  openai: "platform.openai.com/api-keys",
  ollama: "ollama.com/settings/keys (a local Ollama needs no key)",
  "https://openrouter.ai/api/v1": "openrouter.ai/keys",
  "https://api.mistral.ai/v1": "console.mistral.ai, under API keys",
  "https://generativelanguage.googleapis.com/v1beta/openai": "aistudio.google.com/apikey",
};

function keyPlace(m) {
  return KEY_PLACES[m.base_url] || KEY_PLACES[m.provider] || "";
}

const expanded = new WeakSet(); // models whose card is open
const removing = new WeakSet(); // models waiting for "Remove it?"

function priceInput(m, key, placeholder) {
  const input = textInput(m[key] == null ? "" : String(m[key]), (v) => {
    const text = v.trim().replace(",", ".");
    const n = Number(text);
    const bad = text !== "" && (!Number.isFinite(n) || n < 0);
    input.classList.toggle("invalid", bad);
    m[key] = text === "" || bad ? null : n;
  }, { placeholder });
  input.inputMode = "decimal";
  return input;
}

function nameProblem(m) {
  if (!m.name) return "Give the model a name.";
  if (!/^[A-Za-z0-9_.-]+$/.test(m.name)) return "Use letters, digits, - _ . only.";
  if (draft.models.some((o) => o !== m && o.name === m.name)) return "Another model already has this name.";
  return "";
}

function statusChip(m) {
  const r = testResults[m.name];
  if (r && !r.error) {
    const ok = r.reachable && r.structured_json;
    return el("span", { class: "pill " + (ok ? "good" : "bad"), text: ok ? "tested, works" : "test failed" });
  }
  if (m.keychain && !m.has_key && !m.env_set) return el("span", { class: "pill warn", text: "needs a key" });
  return null;
}

function modelCard(m, index) {
  const open = expanded.has(m);
  const card = el("section", { class: "model-card panel" + (open ? " open" : "") });
  const head = el("div", { class: "model-head" });
  const toggle = el("button", { type: "button", class: "model-toggle", "aria-expanded": String(open) });
  toggle.append(el("span", { class: "chev", "aria-hidden": "true", text: open ? "▾" : "▸" }), el("h4", { text: m.name || "New model" }), el("span", { class: "model-sub muted small", text: [providerLabel(m), m.model].filter(Boolean).join(" · ") }));
  toggle.addEventListener("click", () => {
    if (open) expanded.delete(m);
    else expanded.add(m);
    renderSettings();
  });
  head.append(toggle);
  const chip1 = statusChip(m);
  head.append(el("span", { class: "pill " + (m.local ? "good" : "warn"), text: m.local ? "stays on this computer or network" : "data leaves this computer" }));
  if (chip1) head.append(chip1);
  if (removing.has(m)) {
    const yes = el("button", { type: "button", class: "danger small-btn", text: "Remove it" });
    yes.addEventListener("click", () => {
      removing.delete(m);
      draft.models.splice(index, 1);
      for (const k of ["pin_tables", "drawings", "metadata"]) if (draft[k] === m.name) draft[k] = "";
      renderSettings();
    });
    const no = el("button", { type: "button", class: "ghost small-btn", text: "Keep" });
    no.addEventListener("click", () => {
      removing.delete(m);
      renderSettings();
    });
    head.append(el("span", { class: "muted small", text: "Remove this model? Its stored key is deleted when you save." }), yes, no);
  } else {
    const remove = el("button", { type: "button", class: "ghost small-btn", text: "Remove" });
    remove.addEventListener("click", () => {
      removing.add(m);
      renderSettings();
    });
    head.append(remove);
  }
  card.append(head);
  if (!open) return card;

  const body = el("div", { class: "model-body" });
  const nameInput = textInput(m.name, (v) => {
    m.name = v.trim();
    const p = nameProblem(m);
    nameHelp.textContent = p || "How you will refer to it.";
    nameHelp.classList.toggle("err-text", !!p);
  }, { placeholder: "claude" });
  const nameHelp = el("span", { class: "help", text: "How you will refer to it." });
  const nameField = el("label", { class: "f" }, el("span", { class: "lab", text: "Name" }), nameInput, nameHelp);
  const problem = nameProblem(m);
  if (problem) {
    nameHelp.textContent = problem;
    nameHelp.classList.add("err-text");
  }
  const grid = el("div", { class: "grid2" });
  grid.append(
    nameField,
    field(
      "Kind of server",
      selectInput(appSettings.providers.map((p) => [p.id, p.label]), m.provider, (v) => {
        m.provider = v;
        discovered.delete(m);
        triedAuto.delete(m);
        renderSettings();
      }),
      m.provider === "openai-compatible" && /ollama.com|:11434/.test(m.base_url || "") ? "This is an Ollama server. Choose the Ollama kind: it lets Kirpitch turn thinking off and fix the shape of the answer, which models on Ollama need." : ""
    ),
    modelPicker(m),
    field(
      "Address",
      textInput(m.base_url, (v) => (m.base_url = v.trim()), { placeholder: providerInfo(m.provider).default_url || "http://localhost:8000/v1" }),
      providerInfo(m.provider).default_url ? "Leave empty for the usual address." : "Needed for this kind of server."
    )
  );
  body.append(grid);

  // The key
  const keyBox = el("div", { class: "key-box" });
  const keepBox = el("label", { class: "check" });
  const keep = el("input", { type: "checkbox" });
  keep.checked = !!m.keychain;
  keep.addEventListener("change", () => {
    m.keychain = keep.checked;
    renderSettings();
  });
  keepBox.append(keep, el("span", { text: "This model needs an API key, keep it in the system keychain" }));
  keyBox.append(keepBox);
  if (m.keychain) {
    const status = m.has_key ? "A key is stored." : "No key stored yet.";
    keyBox.append(el("p", { class: m.has_key ? "ok-text small" : "muted small", text: status + " It is never written to the settings file." }));
    const place = keyPlace(m);
    if (place && !m.has_key) keyBox.append(el("p", { class: "muted small", text: "Get a key at " + place + "." }));
    const row = el("div", { class: "pick-row" });
    const key = textInput(m.newKey, (v) => (m.newKey = v), { type: "password", placeholder: m.has_key ? "Type a new key to replace it" : "Paste the API key" });
    key.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter") {
        ev.preventDefault();
        saveKey(m);
      }
    });
    const peek = el("button", { type: "button", class: "ghost", text: "Show" });
    peek.addEventListener("click", () => {
      const hidden = key.type === "password";
      key.type = hidden ? "text" : "password";
      peek.textContent = hidden ? "Hide" : "Show";
    });
    const save = el("button", { type: "button", text: m.has_key ? "Replace key" : "Save key" });
    save.addEventListener("click", () => saveKey(m));
    row.append(key, peek, save);
    if (m.has_key) {
      const del = el("button", { type: "button", class: "ghost", text: "Delete key" });
      del.addEventListener("click", () => deleteKey(m));
      row.append(del);
    }
    keyBox.append(row);
  }
  body.append(keyBox);

  // Advanced
  const adv = el("details", { class: "advanced" }, el("summary", { text: "More options" }));
  const advGrid = el("div", { class: "grid2" });
  advGrid.append(
    field("Environment variable with the key", textInput(m.api_key_env, (v) => (m.api_key_env = v.trim()), { placeholder: "ANTHROPIC_API_KEY" }), "Optional. When it is set it is used instead of the keychain."),
    field("Header for the key", textInput(m.api_key_header, (v) => (m.api_key_header = v.trim()), { placeholder: "api-key" }), "Only if the server does not use the usual one (Azure OpenAI uses api-key)."),
    field("Price per million input tokens (US dollars)", priceInput(m, "price_in", "3.0"), "For the cost estimate. Filled in when you pick a model that has a known price."),
    field("Price per million output tokens (US dollars)", priceInput(m, "price_out", "15.0"), "Leave empty when unknown.")
  );
  if (m.provider === "ollama") {
    advGrid.append(field("Let the model think first", selectInput([["", "Server default"], ["false", "No (faster)"], ["true", "Yes"]], m.think == null ? "" : String(m.think), (v) => (m.think = v === "" ? null : v === "true")), "Models that think can use their whole answer on it and answer nothing."));
  }
  adv.append(advGrid);
  body.append(adv);

  // Test
  const test = el("div", { class: "test-row" });
  const run = el("button", { type: "button", class: "primary", text: "Save and test" });
  run.addEventListener("click", () => testModel(m, run));
  test.append(run, resultView(testResults[m.name]));
  body.append(test);
  card.append(body);
  return card;
}

function providerLabel(m) {
  const p = appSettings.providers.find((x) => x.id === m.provider);
  return p ? p.label.replace(/ \(.*$/, "") : m.provider;
}

function chip(ok, label) {
  const c = el("span", { class: "pill " + (ok === true ? "good" : ok === false ? "bad" : ""), text: label + (ok === true ? " ✓" : ok === false ? " ✗" : " (not tested)") });
  return c;
}

function resultView(r) {
  const box = el("div", { class: "test-result" });
  if (!r) return box;
  if (r.error) {
    box.append(el("span", { class: "err-text", text: r.error }));
    return box;
  }
  box.append(chip(r.reachable, "Reachable"), chip(r.structured_json, "Structured JSON"), chip(r.vision, "Images"), chip(r.context_8k, "Long text"), el("span", { class: "muted small", text: r.seconds.toFixed(1) + " s" }));
  r.notes.forEach((n) => box.append(el("p", { class: "muted small", text: n })));
  return box;
}

// ---- Adding a model ---------------------------------------------------------------------------

let adding = false;

function addPreset(p) {
  let name = p.name;
  let n = 2;
  while (draft.models.some((m) => m.name === name)) name = p.name + "-" + n++;
  const m = { name, old_name: null, new: true, newKey: "", provider: p.provider, base_url: p.base_url, effective_url: "", model: p.model, api_key_env: "", keychain: p.keychain, has_key: false, env_set: false, api_key_header: "", think: null, price_in: null, price_out: null, local: p.provider === "ollama" ? !p.base_url : p.base_url.includes("localhost") };
  draft.models.push(m);
  expanded.add(m);
  triedAuto.add(m);
  adding = false;
  renderSettings();
  updateDirty();
  const cards = document.querySelectorAll("#pane-models .model-card");
  if (cards.length) cards[cards.length - 1].scrollIntoView({ block: "nearest", behavior: "smooth" });
  if (canAsk(m)) discoverModels(m);
}

function presetGrid() {
  const box = el("div", { class: "preset-box" });
  box.append(el("h4", { text: draft.models.length ? "Add a model" : "Choose where your model runs" }));
  const grid = el("div", { class: "preset-grid" });
  PRESETS_MODELS.forEach((p) => {
    const b = el("button", { type: "button", class: "preset" });
    b.append(el("strong", { text: p.label }), el("span", { class: "muted small", text: p.blurb }), el("span", { class: "pill " + (p.local ? "good" : "warn"), text: p.local ? "on your computer" : "in the cloud" }));
    b.addEventListener("click", () => addPreset(p));
    grid.append(b);
  });
  box.append(grid);
  if (draft.models.length) {
    const cancel = el("button", { type: "button", class: "ghost", text: "Cancel" });
    cancel.addEventListener("click", () => {
      adding = false;
      renderSettings();
    });
    box.append(cancel);
  }
  return box;
}

function renderModels() {
  const pane = $("pane-models");
  if (draft.models.length === 1 && !triedAuto.has(draft.models[0]) && !expanded.has(draft.models[0])) expanded.add(draft.models[0]);
  const cards = draft.models.map((m, i) => modelCard(m, i));
  // A list that is ready when the card is opened, without a click.
  const toAsk = draft.models.filter((m) => {
    if (triedAuto.has(m)) return false;
    triedAuto.add(m);
    return !discovered.has(m) && canAsk(m);
  });
  let tail;
  if (adding || !draft.models.length) {
    tail = presetGrid();
  } else {
    const add = el("button", { type: "button", class: "add-model-btn", text: "+ Add a model" });
    add.addEventListener("click", () => {
      adding = true;
      renderSettings();
    });
    tail = add;
  }
  const intro = el("p", { class: "muted", text: draft.models.length ? "Each model can do some or all of the tasks. Choose which under Tasks and privacy." : "No models yet. Without one, Kirpitch still works by hand: New part, then import the pins. Add a model to read datasheets automatically." });
  pane.replaceChildren(intro, ...cards, tail);
  setTimeout(() => toAsk.forEach((m) => discoverModels(m)), 0);
}

function modelOptions(blank) {
  return [["", blank], ...draft.models.filter((m) => m.name).map((m) => [m.name, m.name])];
}

function renderTasks() {
  const pane = $("pane-tasks");
  const rows = [
    ["pin_tables", "Pin tables", "Reads the pin names and types."],
    ["drawings", "Package drawings", "Reads the package dimensions."],
    ["metadata", "Part number and description", "Reads the title page."],
  ].map(([key, label, help]) => {
    const chosen = draft.models.find((m) => m.name === (draft[key] || (draft.models[0] && draft.models[0].name)));
    const where = chosen ? (chosen.local ? "stays on this computer or network" : "text of the datasheet goes to " + (chosen.effective_url || chosen.base_url || chosen.provider)) : "no model";
    return el("div", { class: "task-row" }, field(label, selectInput(modelOptions(draft.models.length ? "The first model" : "No model yet"), draft[key], (v) => {
      draft[key] = v;
      renderTasks();
    }), help), el("span", { class: "pill " + (chosen && chosen.local ? "good" : chosen ? "warn" : ""), text: where }));
  });
  const privacy = el("label", { class: "check big" });
  const box = el("input", { type: "checkbox" });
  box.checked = draft.local_only;
  box.addEventListener("change", () => {
    draft.local_only = box.checked;
    updateDirty();
  });
  privacy.append(box, el("span", {}, el("strong", { text: "Local only" }), el("span", { class: "help", text: " Refuse every model that is not on this computer or your network. Cloud models, and Ollama models with a cloud tag, will not be used." })));
  pane.replaceChildren(el("p", { class: "muted", text: "Choose which model does which job. A task without its own model uses the first model." }), ...rows, el("hr"), privacy);
}

function folderRow(key, label, help, kind) {
  const input = textInput(draft[key], (v) => (draft[key] = v.trim()), { placeholder: "Not set" });
  const browse = el("button", { type: "button", text: "Browse…" });
  browse.addEventListener("click", async () => {
    try {
      const path = kind === "folder" ? await pickFolder() : await pickFile(kind);
      if (path) {
        draft[key] = path;
        input.value = path;
        updateDirty();
      }
    } catch (e) {
      show(String(e), "error");
    }
  });
  const clear = el("button", { type: "button", class: "ghost", text: "Clear" });
  clear.addEventListener("click", () => {
    draft[key] = "";
    input.value = "";
    updateDirty();
  });
  return field(label, el("span", { class: "pick-row" }, input, browse, clear), help);
}

function renderFolders() {
  const pane = $("pane-folders");
  const target = selectInput([["", "The installed KiCad"], ["10", "KiCad 10"], ["9", "KiCad 9"], ["8", "KiCad 8"]], draft.kicad_target, (v) => (draft.kicad_target = v));
  const pdf = appSettings.pdfium_found
    ? el("p", { class: "ok-text small", text: "pdfium found: " + appSettings.pdfium_found })
    : el("p", { class: "err-text small", text: "pdfium not found: the datasheet pages cannot be shown. Run cargo xtask fetch-pdfium, or choose its folder here." });
  pane.replaceChildren(
    el("p", { class: "muted", text: "Where things are saved and opened by default. Every dialog starts there." }),
    folderRow("parts", "Parts folder", "Where new parts, copies and Open start.", "folder"),
    folderRow("output", "KiCad output folder", "Where the Export step writes. Each part gets its own subfolder.", "folder"),
    folderRow("datasheets", "Datasheets folder", "Where the datasheet dialog starts.", "folder"),
    field("KiCad version to write for", target, "The Export step uses this unless you pick another."),
    folderRow("fab_profile", "Fab profile (limits of your PCB maker)", "A JSON file, see the documentation. Empty: the generic limits.", "spec"),
    folderRow("pdfium_dir", "pdfium folder", "The library that reads datasheet PDFs. Empty: find it automatically.", "folder"),
    pdf
  );
}

function renderSettings() {
  const scroll = window.scrollY;
  renderSettingsNow();
  window.scrollTo(0, scroll);
}

function renderSettingsNow() {
  if (!draft || !appSettings) {
    $("pane-models").replaceChildren(el("p", { class: "muted", text: "The settings are only available in the Kirpitch app." }));
    return;
  }
  $("settings-path").textContent = appSettings.path;
  const warn = $("settings-warning");
  warn.hidden = !appSettings.config_error;
  warn.textContent = appSettings.config_error ? "The settings file could not be read (" + appSettings.config_error + "). Saving replaces it." : "";
  document.querySelectorAll("#settings-tabs button").forEach((b) => {
    b.classList.toggle("current", b.dataset.pane === activePane);
    b.setAttribute("aria-selected", String(b.dataset.pane === activePane));
  });
  for (const p of ["models", "tasks", "folders"]) $("pane-" + p).hidden = p !== activePane;
  renderModels();
  renderTasks();
  renderFolders();
  updateDirty();
}

// ---- Saving, keys and tests ------------------------------------------------------------------

function toInput() {
  return {
    models: draft.models.map((m) => ({
      old_name: m.old_name,
      name: m.name,
      provider: m.provider,
      base_url: m.base_url,
      model: m.model,
      api_key_env: m.api_key_env,
      keychain: !!m.keychain,
      api_key_header: m.api_key_header,
      think: m.think,
      price_in: m.price_in,
      price_out: m.price_out,
    })),
    pin_tables: draft.pin_tables,
    drawings: draft.drawings,
    metadata: draft.metadata,
    local_only: draft.local_only,
    parts: draft.parts,
    output: draft.output,
    datasheets: draft.datasheets,
    kicad_target: draft.kicad_target,
    fab_profile: draft.fab_profile,
    pdfium_dir: draft.pdfium_dir,
  };
}

async function saveSettings() {
  const typedKeys = draft.models.filter((m) => m.newKey && m.keychain).map((m) => ({ name: m.name, key: m.newKey }));
  try {
    appSettings = await invoke("put_settings", { input: toInput() });
    for (const k of typedKeys) appSettings = await invoke("put_api_key", { name: k.name, key: k.key });
    rebuildDraft(draft.models);
    renderSettings();
    show("Settings saved.", "ok");
    return true;
  } catch (e) {
    show(String(e), "error");
    return false;
  }
}

async function saveKey(m) {
  if (!m.newKey.trim()) return show("Paste the key first.", "error");
  const name = m.name;
  if (!(await saveSettings())) return;
  show("Key stored in the system keychain.", "ok");
  const saved = draft.models.find((x) => x.name === name);
  if (saved) discoverModels(saved);
}

async function deleteKey(m) {
  try {
    if (dirty() && !(await saveSettings())) return;
    appSettings = await invoke("remove_api_key", { name: m.name });
    rebuildDraft(draft.models);
    renderSettings();
    show("Key removed from the keychain.", "ok");
  } catch (e) {
    show(String(e), "error");
  }
}

async function testModel(m, button) {
  button.disabled = true;
  button.textContent = "Testing…";
  try {
    if (!(await saveSettings())) return;
    testResults[m.name] = await invoke("check_model", { name: m.name });
  } catch (e) {
    testResults[m.name] = { error: String(e) };
  } finally {
    renderSettings();
  }
}

$("settings-save").addEventListener("click", saveSettings);
$("settings-discard").addEventListener("click", () => {
  draft = toDraft(appSettings);
  renderSettings();
  show("Changes discarded.", "ok");
});
$("settings-back").addEventListener("click", leaveSettings);
document.querySelectorAll("#settings-tabs button").forEach((b) =>
  b.addEventListener("click", () => {
    activePane = b.dataset.pane;
    renderSettings();
    window.scrollTo({ top: 0, behavior: "auto" });
  })
);
$("btn-settings").addEventListener("click", showSettings);

// ---- Defaults used by the rest of the app --------------------------------------------------------

// The folder a dialog starts in: the one in the settings, else the last one used.
const lastDirBase = lastDir;
lastDir = function (kind) {
  const s = appSettings;
  if (s) {
    const set = kind === "pdf" ? s.datasheets : kind === "out" ? s.output : kind === "spec" || kind === "toml" ? s.parts : "";
    if (set) return set;
  }
  return lastDirBase(kind);
};

function safeName(text) {
  return String(text).replace(/[^\w.-]+/g, "_");
}

// The Export step starts with the folder and KiCad version of the settings.
const renderBeforeSettings = render;
render = function (view, keepSheet) {
  renderBeforeSettings(view, keepSheet);
  if (appSettings) {
    if (!$("out-dir").value && appSettings.output) {
      const sep = appSettings.output.includes("\\") ? "\\" : "/";
      $("out-dir").value = appSettings.output.replace(/[\\/]+$/, "") + sep + safeName(view.mpn);
    }
    if (appSettings.kicad_target) $("target").value = appSettings.kicad_target;
  }
};

// Escape leaves the settings (the changes stay until you save or discard them).
document.addEventListener("keydown", (ev) => {
  if (ev.key !== "Escape" || $("settings").hidden) return;
  if (document.querySelector("dialog[open]") || ev.target.closest("input, select, textarea")) return;
  leaveSettings();
});

// Coming back to the window (after setting a variable, fetching pdfium, ...)
// re-reads the saved settings, unless there are unsaved changes.
window.addEventListener("focus", async () => {
  if ($("settings").hidden || !draft || dirty()) return;
  try {
    const fresh = await invoke("get_settings", {});
    if (JSON.stringify(fresh) === JSON.stringify(appSettings)) return;
    appSettings = fresh;
    rebuildDraft(draft.models);
    renderSettings();
  } catch (e) {
    // Leave the screen as it is.
  }
});

loadSettings().then(() => {
  if ($("settings").hidden === false) renderSettings();
});
