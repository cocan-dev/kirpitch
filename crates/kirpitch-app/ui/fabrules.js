// Fab rules in the settings: choose, import, edit and save the limits of a PCB maker.
// The choice is the `fab_profile` setting (draft.fab_profile): empty for the generic limits,
// `preset:<name>` for a built-in profile, or the path of a saved file. It is kept with the
// other settings by the Save button. Importing, saving and deleting a profile act at once
// because they only write files in the fab folder.

const fab = { view: null, loading: false, edit: null, error: "" };

function fabMm(v) {
  return String(Math.round(v * 1000) / 1000) + " mm";
}

async function fabReload() {
  fab.loading = true;
  try {
    fab.view = await invoke("fab_rules", { current: draft ? draft.fab_profile || "" : "" });
    fab.error = "";
  } catch (e) {
    fab.error = String(e);
  }
  fab.loading = false;
  renderFabRules();
}

function fabSummary(entry) {
  const pick = ["min_copper_gap", "min_drill", "min_silk_text_height"];
  return pick
    .map((k) => entry.limits.find((l) => l.key === k))
    .filter(Boolean)
    .map((l) => l.label.toLowerCase() + " " + fabMm(l.value))
    .join(" · ");
}

function fabChoose(setting) {
  draft.fab_profile = setting;
  updateDirty();
  renderFabRules();
}

function fabEditor() {
  const e = fab.edit;
  const box = el("section", { class: "panel fab-edit" });
  box.append(el("h4", { text: e.title }));
  if (e.note) box.append(el("p", { class: "muted small", text: e.note }));
  const name = textInput(e.name, (v) => (e.name = v), { placeholder: "Name of the profile, for example JLCPCB 4 layers" });
  box.append(field("Name", name));
  const grid = el("div", { class: "fab-limits" });
  for (const l of e.limits) {
    const input = el("input", { type: "number", step: "0.001", min: "0.001", inputmode: "decimal" });
    input.value = String(l.value);
    input.addEventListener("input", () => (l.value = parseFloat(input.value)));
    grid.append(el("label", { class: "fab-limit" }, el("span", { text: l.label }), el("span", { class: "fab-unit" }, input, el("span", { text: "mm" }))));
  }
  box.append(grid);
  if (e.used && e.used.length) {
    box.append(el("details", { class: "fab-report", open: "" }, el("summary", { text: "Taken from the file (" + e.used.length + ")" }), el("ul", {}, ...e.used.map((t) => el("li", { text: t })))));
  }
  if (e.skipped && e.skipped.length) {
    box.append(el("details", { class: "fab-report" }, el("summary", { text: "Not used (" + e.skipped.length + ")" }), el("ul", {}, ...e.skipped.map((t) => el("li", { text: t })))));
  }
  const err = el("p", { class: "err-text small", text: "", hidden: "" });
  const save = el("button", { type: "button", class: "primary", text: "Save and use" });
  save.addEventListener("click", async () => {
    err.hidden = true;
    try {
      const limits = {};
      for (const l of e.limits) {
        if (!(l.value > 0)) throw new Error(l.label + " must be a number greater than 0");
        limits[l.key] = l.value;
      }
      const path = await invoke("fab_save", { input: { name: e.name, limits } });
      fab.edit = null;
      draft.fab_profile = path;
      updateDirty();
      await fabReload();
    } catch (x) {
      err.textContent = String(x.message || x);
      err.hidden = false;
    }
  });
  const cancel = el("button", { type: "button", class: "ghost", text: "Cancel" });
  cancel.addEventListener("click", () => {
    fab.edit = null;
    renderFabRules();
  });
  box.append(err, el("div", { class: "row-actions" }, save, cancel));
  return box;
}

function fabCard(entry) {
  const chosen = (draft.fab_profile || "") === entry.setting;
  const card = el("div", { class: "fab-card" + (chosen ? " chosen" : "") });
  const radio = el("input", { type: "radio", name: "fab-choice", "aria-label": "Use " + entry.name });
  radio.checked = chosen;
  radio.addEventListener("change", () => fabChoose(entry.setting));
  const text = el(
    "div",
    { class: "fab-text" },
    el("strong", { text: entry.name }),
    el("span", { class: "tag", text: entry.builtin ? "built in" : "yours" }),
    el("div", { class: "muted small", text: fabSummary(entry) })
  );
  const actions = el("div", { class: "fab-actions" });
  const copy = el("button", { type: "button", class: "ghost", text: entry.builtin ? "Edit a copy" : "Edit" });
  copy.addEventListener("click", () => {
    fab.edit = {
      title: entry.builtin ? "A copy of " + entry.name : "Edit " + entry.name,
      name: entry.builtin ? entry.name + " (copy)" : entry.name,
      limits: entry.limits.map((l) => ({ ...l })),
      note: entry.builtin ? "The built-in profile stays as it is; this makes your own." : "",
    };
    renderFabRules();
  });
  actions.append(copy);
  if (!entry.builtin) {
    const del = el("button", { type: "button", class: "ghost", text: "Delete" });
    del.addEventListener("click", async () => {
      try {
        await invoke("fab_delete", { path: entry.setting });
        if (draft.fab_profile === entry.setting) draft.fab_profile = "";
        updateDirty();
        await fabReload();
      } catch (x) {
        show(String(x), "error");
      }
    });
    actions.append(del);
  }
  const label = el("label", { class: "fab-pick" }, radio, text);
  card.append(label, actions);
  return card;
}

function renderFabRules() {
  const pane = $("pane-fab");
  if (!pane) return;
  if (!draft || !appSettings) {
    pane.replaceChildren(el("p", { class: "muted", text: "The settings are only available in the Kirpitch app." }));
    return;
  }
  if (!fab.view) {
    pane.replaceChildren(el("p", { class: "muted", text: fab.error || "Loading the fab rules…" }));
    if (!fab.loading && !fab.error) fabReload();
    return;
  }
  const intro = el("p", {
    class: "muted",
    text: "The limits of your PCB maker. Kirpitch checks every pad, hole and silkscreen line against them and builds the footprint to respect them. Choose one, import your maker’s rules, or set the numbers yourself.",
  });
  const list = el("div", { class: "fab-list", role: "radiogroup", "aria-label": "Fab rules in use" }, ...fab.view.entries.map(fabCard));
  const choice = draft.fab_profile || "";
  if (choice && !fab.view.entries.some((x) => x.setting === choice)) {
    list.append(el("p", { class: "err-text small", text: "The profile in the settings (" + choice + ") could not be read; the generic limits are used until you choose another." }));
  }
  const imp = el("button", { type: "button", text: "Import a rules file…" });
  imp.addEventListener("click", async () => {
    try {
      const path = await pickFile("rules");
      if (!path) return;
      const r = await invoke("fab_import", { path });
      fab.edit = {
        title: "Rules from " + path.split(/[\\/]/).pop(),
        name: r.name,
        limits: r.limits.map((l) => ({ ...l })),
        used: r.used,
        skipped: r.skipped,
        note: "Check the numbers, correct what does not fit your order, then save.",
      };
      renderFabRules();
    } catch (x) {
      show(String(x), "error");
    }
  });
  const fresh = el("button", { type: "button", class: "ghost", text: "Set the numbers myself" });
  fresh.addEventListener("click", () => {
    const base = fab.view.entries[0];
    fab.edit = { title: "New fab profile", name: "", limits: base.limits.map((l) => ({ ...l })), note: "Starts from the generic limits." };
    renderFabRules();
  });
  const help = el(
    "details",
    { class: "fab-help" },
    el("summary", { text: "Where do I get the rules of my maker?" }),
    el("p", {
      text: "There is no universal format. The files that exist are KiCad custom rules (.kicad_dru) and Eagle design rules (.dru); Kirpitch reads both. Most makers (JLCPCB among them) publish their limits only as a web page, and there is no official JLCPCB file, but the community shares .kicad_dru files for it. The JLCPCB profile built in here follows their capabilities page. Rules for one kind of object only (inner layers, non-plated holes, slots) are skipped, and the smallest minimum of the others is used. Always compare with your maker’s page before you order.",
    })
  );
  const parts = [intro, list, el("div", { class: "row-actions" }, imp, fresh)];
  if (fab.view.problems.length) parts.push(el("p", { class: "err-text small", text: "Some saved profiles could not be read: " + fab.view.problems.join("; ") }));
  if (fab.edit) parts.push(fabEditor());
  parts.push(help, el("p", { class: "muted small", text: "Saved profiles are kept in " + fab.view.folder + ". Your choice is saved with the Save button of the settings." }));
  pane.replaceChildren(...parts);
}
