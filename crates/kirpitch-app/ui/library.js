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
  open.addEventListener("click", () => {
    lib.returnTo = null;
    $("library").hidden = true;
    openPath(p.path);
  });
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
    kids.push(el("div", { class: "lib-grid" }, ...shown.map(libCard)));
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
  if (ev.key === "Escape" && !$("library").hidden && document.activeElement !== $("lib-search")) leaveLibrary();
});
