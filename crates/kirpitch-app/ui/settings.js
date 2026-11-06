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

const PRESETS_MODELS = [
  { label: "Ollama on this computer", name: "ollama-local", provider: "ollama", base_url: "", model: "qwen3-vl:7b", keychain: false },
  { label: "Anthropic Claude", name: "claude", provider: "anthropic", base_url: "", model: "claude-sonnet", keychain: true },
  { label: "OpenAI", name: "openai", provider: "openai", base_url: "", model: "gpt-5", keychain: true },
  { label: "LM Studio on this computer", name: "lm-studio", provider: "openai-compatible", base_url: "http://localhost:1234/v1", model: "local-model", keychain: false },
  { label: "OpenRouter", name: "openrouter", provider: "openai-compatible", base_url: "https://openrouter.ai/api/v1", model: "vendor/model", keychain: true },
  { label: "Mistral", name: "mistral", provider: "openai-compatible", base_url: "https://api.mistral.ai/v1", model: "mistral-large-latest", keychain: true },
  { label: "Google Gemini", name: "gemini", provider: "openai-compatible", base_url: "https://generativelanguage.googleapis.com/v1beta/openai", model: "gemini-pro", keychain: true },
  { label: "Another OpenAI-compatible server", name: "my-server", provider: "openai-compatible", base_url: "http://localhost:8000/v1", model: "model-name", keychain: false },
];

const clone = (x) => JSON.parse(JSON.stringify(x));

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
    draft = toDraft(appSettings);
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

function modelCard(m, index) {
  const card = el("section", { class: "model-card panel" });
  const head = el("div", { class: "model-head" });
  const title = el("h4", { text: m.name || "New model" });
  const where = el("span", { class: "pill " + (m.local ? "good" : "warn"), text: m.local ? "stays on this computer or network" : "data leaves this computer" });
  head.append(title, where);
  const remove = el("button", { type: "button", class: "ghost small-btn", text: "Remove" });
  remove.addEventListener("click", () => {
    draft.models.splice(index, 1);
    for (const k of ["pin_tables", "drawings", "metadata"]) if (draft[k] === m.name) draft[k] = "";
    renderSettings();
  });
  head.append(remove);
  card.append(head);

  const grid = el("div", { class: "grid2" });
  grid.append(
    field("Name", textInput(m.name, (v) => (m.name = v.trim()), { placeholder: "claude" }), "How you will refer to it. Letters, digits, - _ ."),
    field(
      "Kind of server",
      selectInput(appSettings.providers.map((p) => [p.id, p.label]), m.provider, (v) => {
        m.provider = v;
        renderSettings();
      })
    ),
    field("Model", textInput(m.model, (v) => (m.model = v.trim()), { placeholder: "claude-sonnet" }), "The name the server knows it by."),
    field("Address", textInput(m.base_url, (v) => (m.base_url = v.trim()), { placeholder: providerInfo(m.provider).default_url || "http://localhost:8000/v1" }), providerInfo(m.provider).default_url ? "Leave empty for the usual address." : "Needed for this kind of server.")
  );
  card.append(grid);

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
    keyBox.append(el("p", { class: m.has_key ? "ok-text small" : "muted small", text: status + " The key is never written to the settings file." }));
    const row = el("div", { class: "pick-row" });
    const key = textInput(m.newKey, (v) => (m.newKey = v), { type: "password", placeholder: m.has_key ? "Type a new key to replace it" : "Paste the API key" });
    const save = el("button", { type: "button", text: m.has_key ? "Replace key" : "Save key" });
    save.addEventListener("click", () => saveKey(m));
    row.append(key, save);
    if (m.has_key) {
      const del = el("button", { type: "button", class: "ghost", text: "Delete key" });
      del.addEventListener("click", () => deleteKey(m));
      row.append(del);
    }
    keyBox.append(row);
  }
  card.append(keyBox);

  // Advanced
  const adv = el("details", { class: "advanced" }, el("summary", { text: "More options" }));
  const advGrid = el("div", { class: "grid2" });
  advGrid.append(
    field("Environment variable with the key", textInput(m.api_key_env, (v) => (m.api_key_env = v.trim()), { placeholder: "ANTHROPIC_API_KEY" }), "Optional. When it is set it is used instead of the keychain."),
    field("Header for the key", textInput(m.api_key_header, (v) => (m.api_key_header = v.trim()), { placeholder: "api-key" }), "Only if the server does not use the usual one (Azure OpenAI uses api-key)."),
    field("Price per million input tokens", textInput(m.price_in == null ? "" : String(m.price_in), (v) => (m.price_in = v.trim() === "" ? null : Number(v)), { placeholder: "3.0" }), "For the cost estimate."),
    field("Price per million output tokens", textInput(m.price_out == null ? "" : String(m.price_out), (v) => (m.price_out = v.trim() === "" ? null : Number(v)), { placeholder: "15.0" }))
  );
  if (m.provider === "ollama") {
    advGrid.append(field("Let the model think first", selectInput([["", "Server default"], ["false", "No (faster)"], ["true", "Yes"]], m.think == null ? "" : String(m.think), (v) => (m.think = v === "" ? null : v === "true")), "Models that think can use their whole answer on it and answer nothing."));
  }
  adv.append(advGrid);
  card.append(adv);

  // Test
  const test = el("div", { class: "test-row" });
  const run = el("button", { type: "button", text: "Save and test" });
  run.addEventListener("click", () => testModel(m, run));
  test.append(run, resultView(testResults[m.name]));
  card.append(test);
  return card;
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

function renderModels() {
  const pane = $("pane-models");
  const cards = draft.models.map((m, i) => modelCard(m, i));
  const add = el("div", { class: "add-model" });
  const select = selectInput([["", "Add a model…"], ...PRESETS_MODELS.map((p, i) => [String(i), p.label])], "", () => {});
  select.addEventListener("change", () => {
    if (select.value === "") return;
    const p = PRESETS_MODELS[Number(select.value)];
    let name = p.name;
    let n = 2;
    while (draft.models.some((m) => m.name === name)) name = p.name + "-" + n++;
    draft.models.push({ name, old_name: null, new: true, newKey: "", provider: p.provider, base_url: p.base_url, effective_url: "", model: p.model, api_key_env: "", keychain: p.keychain, has_key: false, env_set: false, api_key_header: "", think: null, price_in: null, price_out: null, local: p.provider === "ollama" || p.base_url.includes("localhost") });
    renderSettings();
    updateDirty();
  });
  add.append(select);
  const intro = el("p", { class: "muted", text: draft.models.length ? "Each model can do some or all of the tasks. Choose which under Tasks and privacy." : "No models yet. Without one, Kirpitch still works by hand: New part, then import the pins. Add a model to read datasheets automatically." });
  pane.replaceChildren(intro, ...cards, add);
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
    draft = toDraft(appSettings);
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
  if (!(await saveSettings())) return;
  show("Key stored in the system keychain.", "ok");
}

async function deleteKey(m) {
  try {
    if (dirty() && !(await saveSettings())) return;
    appSettings = await invoke("remove_api_key", { name: m.name });
    draft = toDraft(appSettings);
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

loadSettings().then(() => {
  if ($("settings").hidden === false) renderSettings();
});
