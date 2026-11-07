// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// "My parts": the parts that were made, found in the folders you work in, with
// their name, footprint and review state, to search and open again. It uses the
// helpers of app.js and start.js ($, el, invoke, openPath, pickFolder, ...).

const LIB_KEY = "kirpitch.libdirs";
const lib = { parts: [], loaded: false, loading: false, error: "", query: "", sort: "recent", returnTo: null };

function libUserDirs() {
  try {
    const v = JSON.parse(localStorage.getItem(LIB_KEY) || "[]");
    return Array.isArray(v) ? v.filter((d) => typeof d === "string") : [];
  } catch {
    return [];
  }
}

function libSaveDirs(dirs) {
  try {
    localStorage.setItem(LIB_KEY, JSON.stringify(dirs));
  } catch {
    // Only a convenience.
  }
}

function libDirOf(path) {
  return String(path).replace(/[\\/][^\\/]*$/, "");
}

// Where parts are looked for: the folders you added, the folders of the parts you
// opened or made, and the last folders you chose. The default folder of the
// settings is added by the program itself.
function libraryDirs() {
  const dirs = [...libUserDirs()];
  try {
    recentPaths().forEach((p) => dirs.push(libDirOf(p)));
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key && key.startsWith("kirpitch.pdf:")) dirs.push(libDirOf(key.slice("kirpitch.pdf:".length)));
    }
  } catch {
    // The list is only a convenience.
  }
  ["spec", "out"].forEach((k) => {
    const d = lastDir(k);
    if (d) dirs.push(d);
  });
  const out = $("ds-out") && $("ds-out").value.trim();
  if (out) dirs.push(libDirOf(out));
  // The working copies of the examples are not parts you made.
  return [...new Set(dirs.filter((d) => d && !/kirpitch-examples/i.test(d)))];
}

async function libLoad() {
  lib.loading = true;
  lib.error = "";
  libPaint();
  try {
    lib.parts = await invoke("list_parts", { dirs: libraryDirs() });
    lib.loaded = true;
  } catch (e) {
    lib.error = String(e);
  }
  lib.loading = false;
  libPaint();
}

// ---- Showing and leaving -------------------------------------------------------------------

async function showLibrary() {
  lib.returnTo = current;
  current = null;
  $("empty").hidden = true;
  $("work").hidden = true;
  $("settings").hidden = true;
  $("library").hidden = false;
  const home = el("button", { type: "button", class: "crumb", text: "Home" });
  home.addEventListener("click", goHome);
  $("crumbs").replaceChildren(home, el("span", { class: "sep", "aria-hidden": "true", text: "›" }), el("span", { class: "crumb here", text: "My parts" }));
  window.scrollTo({ top: 0, behavior: "auto" });
  libPaint();
  await libLoad();
  const search = $("lib-search");
  if (search) search.focus();
}

function leaveLibrary() {
  $("library").hidden = true;
  if (lib.returnTo) {
    const view = lib.returnTo;
    lib.returnTo = null;
    render(view, true);
  } else {
    goHome();
  }
}

// ---- Drawing the screen ------------------------------------------------------------------------

function libAgo(secs) {
  if (!secs) return "";
  const s = Math.max(0, Date.now() / 1000 - secs);
  if (s < 90) return "just now";
  if (s < 3600) return Math.round(s / 60) + " minutes ago";
  if (s < 86400) return Math.round(s / 3600) + " hours ago";
  if (s < 86400 * 45) return Math.round(s / 86400) + " days ago";
  return new Date(secs * 1000).toLocaleDateString();
}

function libMm(v) {
  return String(Math.round(v * 100) / 100);
}

function libMatches(p, query) {
  const hay = [p.mpn, p.manufacturer, p.description, p.package, p.family, p.file, p.folder].join(" ").toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

const LIB_SORTS = {
  recent: (a, b) => (b.modified || 0) - (a.modified || 0),
  name: (a, b) => a.mpn.localeCompare(b.mpn, undefined, { numeric: true }),
  package: (a, b) => a.package.localeCompare(b.package, undefined, { numeric: true }) || a.mpn.localeCompare(b.mpn),
  pins: (a, b) => b.pin_count - a.pin_count || a.mpn.localeCompare(b.mpn),
};

function libSay(text, kind) {
  const m = $("lib-status");
  m.textContent = text;
  m.className = "message" + (kind ? " " + kind : "");
  m.hidden = !text;
  clearTimeout(libSay.timer);
  if (text && kind === "ok") libSay.timer = setTimeout(() => (m.hidden = true), 6000);
}

// The name of the file without its ending, which is what Rename changes.
function libStem(file) {
  return file.replace(/\.partspec\.json$/i, "").replace(/\.json$/i, "");
}

// ---- What can be done with a part: a menu (right click, or the ⋯ button) ---------------------

let libMenuEl = null;

function libMenuClose(focusBack) {
  if (!libMenuEl) return;
  const back = libMenuEl.opener;
  libMenuEl.remove();
  libMenuEl = null;
  if (focusBack && back && back.isConnected) back.focus();
}

function libMenuOpen(p, wrap, x, y, opener) {
  libMenuClose(false);
  const menu = el("ul", { class: "lib-menu", role: "menu", "aria-label": "Actions for " + p.mpn });
  const item = (label, run, danger) => {
    const b = el("button", { type: "button", role: "menuitem", class: danger ? "danger-item" : "", text: label });
    b.addEventListener("click", () => {
      libMenuClose(false);
      run();
    });
    menu.append(el("li", { role: "none" }, b));
  };
  item("Open", () => openLibraryPart(p));
  item("Rename file…", () => libRenamePanel(p, wrap));
  item("Copy file path", () => libCopyPath(p));
  item("Delete…", () => libDeletePanel(p, wrap), true);
  menu.opener = opener || null;
  document.body.append(menu);
  libMenuEl = menu;
  // Inside the window, whatever the place that was clicked.
  const w = menu.offsetWidth;
  const h = menu.offsetHeight;
  menu.style.left = Math.max(4, Math.min(x, window.innerWidth - w - 4)) + "px";
  menu.style.top = Math.max(4, Math.min(y, window.innerHeight - h - 4)) + "px";
  menu.querySelector("button").focus();
}

document.addEventListener("click", (ev) => {
  if (libMenuEl && !libMenuEl.contains(ev.target)) libMenuClose(false);
});
document.addEventListener("scroll", () => libMenuClose(false), true);
window.addEventListener("blur", () => libMenuClose(false));
document.addEventListener(
  "keydown",
  (ev) => {
    if (!libMenuEl) return;
    const buttons = [...libMenuEl.querySelectorAll("button")];
    const at = buttons.indexOf(document.activeElement);
    if (ev.key === "Escape") {
      ev.stopPropagation();
      libMenuClose(true);
    } else if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
      ev.preventDefault();
      const next = (at + (ev.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length;
      buttons[next].focus();
    } else if (ev.key === "Tab") {
      libMenuClose(false);
    }
  },
  true
);

function openLibraryPart(p) {
  lib.returnTo = null;
  $("library").hidden = true;
  openPath(p.path);
}

async function libCopyPath(p) {
  try {
    await navigator.clipboard.writeText(p.path);
    libSay("Copied the path of " + p.file + ".", "ok");
  } catch {
    libSay("Could not copy. The file is " + p.path, "");
  }
}

// A card turns into a small form, so nothing is asked in a window that could be hidden.
function libPanel(p, wrap, title, ...kids) {
  const panel = el("div", { class: "lib-panel", role: "group", "aria-label": title }, el("strong", { text: title }), ...kids);
  wrap.classList.add("editing");
  wrap.replaceChildren(panel);
  return panel;
}

function libRenamePanel(p, wrap) {
  const input = el("input", { type: "text", spellcheck: "false", autocomplete: "off", "aria-label": "New file name" });
  input.value = libStem(p.file);
  const err = el("p", { class: "err-text small", hidden: "" });
  const ending = /\.partspec\.json$/i.test(p.file) ? ".partspec.json" : ".json";
  const save = el("button", { type: "button", class: "primary", text: "Rename" });
  const cancel = el("button", { type: "button", class: "ghost", text: "Cancel" });
  const go = async () => {
    save.disabled = true;
    err.hidden = true;
    try {
      const to = await invoke("rename_part", { path: p.path, newName: input.value });
      libSay("Renamed to " + to.split(/[\\/]/).pop() + ".", "ok");
      await libLoad();
    } catch (e) {
      err.textContent = String(e);
      err.hidden = false;
      save.disabled = false;
      input.focus();
    }
  };
  save.addEventListener("click", go);
  cancel.addEventListener("click", libPaint);
  input.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") go();
    else if (ev.key === "Escape") {
      ev.stopPropagation();
      libPaint();
    }
  });
  libPanel(
    p,
    wrap,
    "Rename the file of " + p.mpn,
    el("div", { class: "lib-rename" }, input, el("span", { class: "muted", text: ending })),
    el("p", { class: "muted small", text: "Only the file name changes, in the same folder. The part number inside the part stays " + p.mpn + "." }),
    err,
    el("div", { class: "start-actions" }, save, cancel)
  );
  input.focus();
  input.select();
}

function libDeletePanel(p, wrap) {
  const err = el("p", { class: "err-text small", hidden: "" });
  const del = el("button", { type: "button", class: "danger", text: "Delete the file" });
  const cancel = el("button", { type: "button", text: "Keep it" });
  del.addEventListener("click", async () => {
    del.disabled = true;
    try {
      await invoke("delete_part", { path: p.path });
      try {
        localStorage.setItem("kirpitch.recent", JSON.stringify(recentPaths().filter((x) => x !== p.path)));
        renderRecent();
      } catch {
        // The list of recent files is only a convenience.
      }
      libSay("Deleted " + p.file + ".", "ok");
      await libLoad();
    } catch (e) {
      err.textContent = String(e);
      err.hidden = false;
      del.disabled = false;
    }
  });
  cancel.addEventListener("click", libPaint);
  cancel.addEventListener("keydown", (ev) => {
    if (ev.key === "Escape") {
      ev.stopPropagation();
      libPaint();
    }
  });
  libPanel(
    p,
    wrap,
    "Delete " + p.mpn + "?",
    el("p", { text: p.file + " in " + p.folder + " is deleted from your computer for good; it does not go to the recycle bin." }),
    el("p", { class: "muted small", text: "KiCad files that were made from this part, and the datasheet, are not touched." }),
    err,
    el("div", { class: "start-actions" }, del, cancel)
  );
  cancel.focus();
}

function libItem(p) {
  const wrap = el("div", { class: "lib-item" });
  const more = el("button", { type: "button", class: "lib-more", "aria-label": "Actions for " + p.mpn, "aria-haspopup": "menu", title: "Rename, delete…", text: "⋯" });
  more.addEventListener("click", (ev) => {
    ev.stopPropagation();
    const r = more.getBoundingClientRect();
    libMenuOpen(p, wrap, r.right - 170, r.bottom + 4, more);
  });
  wrap.addEventListener("contextmenu", (ev) => {
    ev.preventDefault();
    libMenuOpen(p, wrap, ev.clientX, ev.clientY, null);
  });
  wrap.append(libCard(p), more);
  return wrap;
}

function libCard(p) {
  const open = el("button", { type: "button", class: "lib-card", "aria-label": "Open " + p.mpn });
  const art = el("div", { class: "lib-fp", "aria-hidden": "true" });
  // The drawing is made by our own Rust code from numbers and escaped text.
  art.innerHTML = p.footprint_svg;
  const state =
    p.approved === p.gates
      ? el("span", { class: "badge all", text: "all approved" })
      : p.stale
        ? el("span", { class: "badge stale", text: p.stale + " changed since approval" })
        : el("span", { class: "badge", text: p.approved + " of " + p.gates + " approved" });
  const facts = [p.pin_count + " pins", libMm(p.pitch) + " mm pitch", libMm(p.body_width) + " × " + libMm(p.body_length) + " × " + libMm(p.body_height) + " mm body"];
  open.append(
    art,
    el(
      "div",
      { class: "lib-main" },
      el("strong", { class: "lib-mpn", text: p.mpn }),
      el("span", { class: "lib-sub", text: [p.manufacturer, p.description].filter(Boolean).join(" · ") }),
      el("span", { class: "lib-pkg", text: p.package }),
      el("span", { class: "lib-facts", text: facts.join(" · ") }),
      el("span", { class: "lib-line" }, state, el("span", { class: "meta", text: [p.origin, libAgo(p.modified)].filter(Boolean).join(" · ") })),
      el("span", { class: "lib-path", title: p.path, text: p.file + "  —  " + p.folder })
    )
  );
  open.addEventListener("click", () => openLibraryPart(p));
  return open;
}

function libPaint() {
  const box = $("lib-body");
  if (!box) return;
  const shown = lib.parts.filter((p) => libMatches(p, lib.query)).sort(LIB_SORTS[lib.sort]);
  $("lib-count").textContent = lib.loaded ? (lib.query ? shown.length + " of " + lib.parts.length : String(lib.parts.length)) + (lib.parts.length === 1 ? " part" : " parts") : "";
  const kids = [];
  if (lib.error) kids.push(el("p", { class: "message error", text: lib.error }));
  if (lib.loading && !lib.loaded) {
    kids.push(el("p", { class: "muted", text: "Looking for parts…" }));
  } else if (lib.loaded && !lib.parts.length) {
    const add = el("button", { type: "button", class: "primary", text: "Add a folder…" });
    add.addEventListener("click", libAddFolder);
    const from = el("button", { type: "button", text: "From datasheet…" });
    from.addEventListener("click", () => openDatasheetDialog());
    kids.push(el("div", { class: "lib-empty" }, el("h3", { text: "No parts yet" }), el("p", { class: "muted", text: "Parts you make with New part… or From datasheet… appear here. If your parts are somewhere else, tell Kirpitch where to look." }), el("div", { class: "start-actions" }, add, from)));
  } else if (!shown.length) {
    kids.push(el("p", { class: "muted", text: "Nothing matches “" + lib.query + "”." }));
  } else {
    kids.push(el("div", { class: "lib-grid" }, ...shown.map(libItem)));
  }
  box.replaceChildren(...kids);
  libPaintFolders();
}

function libPaintFolders() {
  const box = $("lib-folders");
  const mine = libUserDirs();
  const all = libraryDirs();
  const list = el("ul", { class: "lib-dirs" });
  all.forEach((d) => {
    const li = el("li", {}, el("span", { class: "lib-dir", text: d }));
    if (mine.includes(d)) {
      const rm = el("button", { type: "button", class: "ghost small-btn", "aria-label": "Stop looking in " + d, text: "✕" });
      rm.addEventListener("click", () => {
        libSaveDirs(libUserDirs().filter((x) => x !== d));
        libLoad();
      });
      li.append(rm);
    }
    list.append(li);
  });
  const add = el("button", { type: "button", class: "ghost small-btn", text: "Add a folder…" });
  add.addEventListener("click", libAddFolder);
  box.replaceChildren(
    el("summary", { text: "Folders looked in (" + all.length + ")" }),
    el("p", { class: "muted small", text: "The folders of the parts you opened or made, the folders you chose, the parts folder of Settings, and the ones you add here. Folders below them are searched too." }),
    list,
    add
  );
}

async function libAddFolder() {
  try {
    const dir = await pickFolder();
    if (!dir) return;
    libSaveDirs([...new Set([...libUserDirs(), dir])]);
    await libLoad();
  } catch (e) {
    lib.error = String(e);
    libPaint();
  }
}

$("btn-library").addEventListener("click", showLibrary);
$("start-library").addEventListener("click", showLibrary);
$("lib-back").addEventListener("click", leaveLibrary);
$("lib-refresh").addEventListener("click", libLoad);
$("lib-search").addEventListener("input", (ev) => {
  lib.query = ev.target.value;
  libPaint();
});
$("lib-sort").addEventListener("change", (ev) => {
  lib.sort = ev.target.value;
  libPaint();
});
document.addEventListener("keydown", (ev) => {
  if (ev.key === "Escape" && !$("library").hidden && document.activeElement !== $("lib-search") && !libMenuEl && !document.activeElement.closest(".lib-panel")) leaveLibrary();
});
