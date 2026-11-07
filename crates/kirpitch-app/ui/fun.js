// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// The playful layer: the chip drawing, the mascot, example cards, the pops when
// a stage is approved, the celebration, and friendlier words. It builds on the
// helpers of app.js and start.js.

const reduceMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// ---- Drawings ----------------------------------------------------------------

// A small chip with `pins` pins (half on each side) that plug in one by one.
function chipSvg(pins, animate) {
  const per = Math.max(2, Math.ceil(pins / 2));
  const w = 24 + per * 14;
  const h = 70;
  const pin = (x, y, top, i) =>
    `<rect class="pin" x="${x}" y="${y}" width="8" height="12" rx="2" fill="var(--copper)" stroke="var(--copper-line)" stroke-width="1" style="--from:${top ? -14 : 14}px;animation-delay:${animate ? i * 70 : 0}ms${animate ? "" : ";animation:none"}"/>`;
  let out = `<svg viewBox="0 0 ${w} ${h + 24}" role="img" aria-label="A chip with ${pins} pins" class="chip-art" xmlns="http://www.w3.org/2000/svg">`;
  for (let i = 0; i < per; i++) {
    out += pin(14 + i * 14, 4, true, i);
    if (i + per < pins || pins % 2 === 0) out += pin(14 + i * 14, h + 8, false, i + per);
  }
  out += `<g class="die"><rect x="6" y="14" width="${w - 12}" height="${h - 8}" rx="9" fill="var(--panel)" stroke="var(--body)" stroke-width="2.5"/>`;
  out += `<circle class="led" cx="20" cy="29" r="4" fill="var(--brick)"/>`;
  out += `<path d="M${w / 2 - 12} ${h / 2 + 18} q12 12 24 0" fill="none" stroke="var(--brick)" stroke-width="3" stroke-linecap="round"/>`;
  out += `<circle cx="${w / 2 - 8}" cy="${h / 2 + 6}" r="3" fill="var(--text)"/><circle cx="${w / 2 + 8}" cy="${h / 2 + 6}" r="3" fill="var(--text)"/></g></svg>`;
  return out;
}

// Pinny, the mascot: a cheerful pin.
const MASCOT = `<svg class="mascot" viewBox="0 0 48 64" role="img" aria-label="Pinny, a cheerful pin" xmlns="http://www.w3.org/2000/svg">
  <rect x="14" y="4" width="20" height="40" rx="8" fill="var(--copper)" stroke="var(--copper-line)" stroke-width="2"/>
  <rect x="19" y="44" width="10" height="16" rx="3" fill="var(--body)"/>
  <g class="eyes"><circle cx="20" cy="18" r="3" fill="var(--bg)"/><circle cx="28" cy="18" r="3" fill="var(--bg)"/>
  <circle cx="20.5" cy="18.5" r="1.4" fill="var(--text)"/><circle cx="28.5" cy="18.5" r="1.4" fill="var(--text)"/></g>
  <path d="M19 28 q5 6 10 0" fill="none" stroke="var(--bg)" stroke-width="2" stroke-linecap="round"/>
</svg>`;

function withMascot(text) {
  const box = el("div", { class: "cheer" });
  const art = document.createElement("span");
  art.innerHTML = MASCOT;
  box.append(art, el("span", { text }));
  return box;
}

// ---- Header logo and hero --------------------------------------------------------

(() => {
  const h1 = document.querySelector(".top h1");
  if (h1) {
    const logo = document.createElement("span");
    logo.innerHTML = chipSvg(4, false).replace('class="chip-art"', 'class="logo"');
    h1.prepend(logo.firstChild);
  }
  const hero = $("hero-art");
  if (hero) hero.innerHTML = chipSvg(8, true);
})();

// ---- Example cards ----------------------------------------------------------------

// A cartoon of the kind of package: not exact, only the same sort (pins on two
// sides, on four, flat pads, a grid of balls, a tab ...), with the same face.
function packageKind(name) {
  const n = String(name || "").toUpperCase();
  if (/BGA|CSP|LGA/.test(n)) return "bga";
  if (/QFN|MLF|LFCSP/.test(n)) return "qfn";
  if (/DFN|SON/.test(n)) return "dfn";
  if (/QFP/.test(n)) return "qfp";
  if (/SOT-?223|SOT-?89|SC-?73/.test(n)) return "sot223";
  if (/SOD|^[RCL]_|(^|[^\d])(0201|0402|0603|0805|1206|1210|2512)([^\d]|$)|AXIAL/.test(n)) return "chip2";
  if (/SOT|SC-?70|SC-?59/.test(n)) return "sot";
  if (/TO-?92/.test(n)) return "to92";
  if (/(^|[^A-Z])TO-?\d|DPAK/.test(n)) return "to220";
  if (/DIP|SIP/.test(n)) return "dip";
  return "dual";
}

// The number of pins a package name ends in ("SOIC-8_...", "SOT-23-5_..."), else 8.
function pinsOf(packageName) {
  const head = String(packageName || "").split("_")[0];
  const m = /-(\d+)$/.exec(head);
  return m ? Number(m[1]) : 8;
}

function partGlyph(packageName, pins) {
  const kind = packageKind(packageName);
  const n = String(packageName || "").toUpperCase();
  const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));
  const pin = (x, y, w, h, r = 2) => '<rect x="' + x + '" y="' + y + '" width="' + w + '" height="' + h + '" rx="' + r + '" fill="var(--copper)" stroke="var(--copper-line)" stroke-width="1"/>';
  const body = (x, y, w, h, r) => '<rect x="' + x + '" y="' + y + '" width="' + w + '" height="' + h + '" rx="' + r + '" fill="var(--panel)" stroke="var(--body)" stroke-width="2.5"/>';
  const face = (cx, cy, k) =>
    '<circle cx="' + (cx - 8 * k) + '" cy="' + (cy - 3 * k) + '" r="' + 2.8 * k + '" fill="var(--text)"/>' +
    '<circle cx="' + (cx + 8 * k) + '" cy="' + (cy - 3 * k) + '" r="' + 2.8 * k + '" fill="var(--text)"/>' +
    '<path d="M' + (cx - 9 * k) + " " + (cy + 7 * k) + " q" + 9 * k + " " + 9 * k + " " + 18 * k + ' 0" fill="none" stroke="var(--brick)" stroke-width="' + Math.max(2, 3 * k) + '" stroke-linecap="round"/>';
  const dot = (x, y) => '<circle cx="' + x + '" cy="' + y + '" r="3" fill="var(--brick)"/>';
  // n pins spread between a and b (the centres), each w wide.
  const row = (count, a, b) => Array.from({ length: count }, (_, i) => (count === 1 ? (a + b) / 2 : a + ((b - a) * i) / (count - 1)));
  let s = "";
  if (kind === "dual") {
    const thin = /TSSOP|SSOP|MSOP/.test(n);
    const per = clamp(Math.ceil(pins / 2) + (thin ? 1 : 0), 2, 7);
    const w = thin ? 5 : 7;
    for (const x of row(per, 22, 74)) s += pin(x - w / 2, 12, w, 12) + pin(x - w / 2, 72, w, 12);
    s += body(10, 22, 76, 52, 9) + dot(21, 33) + face(48, 50, 1);
  } else if (kind === "dip") {
    const per = clamp(Math.ceil(pins / 2), 3, 7);
    for (const y of row(per, 22, 76)) s += pin(11, y - 3.5, 17, 7, 3) + pin(68, y - 3.5, 17, 7, 3);
    s += body(26, 8, 44, 80, 6) + '<path d="M42 8 a6 6 0 0 0 12 0" fill="none" stroke="var(--body)" stroke-width="2.5"/>' + dot(34, 80) + face(48, 54, 0.8);
  } else if (kind === "qfp") {
    const per = clamp(Math.ceil(pins / 4), 3, 7);
    for (const p of row(per, 30, 66)) s += pin(p - 2, 11, 4, 12, 1) + pin(p - 2, 73, 4, 12, 1) + pin(11, p - 2, 12, 4, 1) + pin(73, p - 2, 12, 4, 1);
    s += body(21, 21, 54, 54, 7) + dot(30, 30) + face(48, 50, 0.8);
  } else if (kind === "qfn") {
    const per = clamp(Math.ceil(pins / 4), 3, 5);
    s += body(16, 16, 64, 64, 9);
    for (const p of row(per, 30, 66)) s += pin(p - 3, 13, 6, 8, 2) + pin(p - 3, 75, 6, 8, 2) + pin(13, p - 3, 8, 6, 2) + pin(75, p - 3, 8, 6, 2);
    s += '<rect x="34" y="34" width="28" height="28" rx="6" fill="var(--copper)" fill-opacity="0.25"/>' + dot(26, 26) + face(48, 49, 0.6);
  } else if (kind === "dfn") {
    const per = clamp(Math.ceil(pins / 2), 2, 5);
    s += body(14, 22, 68, 52, 9);
    for (const x of row(per, 26, 70)) s += pin(x - 3, 18, 6, 9, 2) + pin(x - 3, 69, 6, 9, 2);
    s += dot(23, 32) + face(48, 50, 0.85);
  } else if (kind === "bga") {
    s += body(16, 16, 64, 64, 9);
    const grid = row(5, 27, 69);
    grid.forEach((x, i) =>
      grid.forEach((y, j) => {
        if (i === 0 || i === 4 || j === 0 || j === 4) s += '<circle cx="' + x + '" cy="' + y + '" r="3.2" fill="var(--copper)" stroke="var(--copper-line)" stroke-width="1"/>';
      })
    );
    s += face(48, 50, 0.65);
  } else if (kind === "sot") {
    const p = pins <= 3 ? 3 : clamp(pins, 5, 8);
    const top = p === 3 ? 1 : Math.floor(p / 2);
    const bottom = p - top;
    for (const x of row(top, 36, 60)) s += pin(x - 3, 20, 6, 12, 2);
    for (const x of row(bottom, 36, 60)) s += pin(x - 3, 64, 6, 12, 2);
    s += body(24, 30, 48, 36, 7) + dot(32, 38) + face(48, 50, 0.6);
  } else if (kind === "sot223") {
    s += '<rect x="26" y="8" width="44" height="16" rx="3" fill="var(--copper)" stroke="var(--copper-line)" stroke-width="1"/>';
    for (const x of row(3, 30, 66)) s += pin(x - 4, 60, 8, 20, 2);
    s += body(18, 22, 60, 42, 6) + dot(27, 31) + face(48, 44, 0.7);
  } else if (kind === "to92") {
    for (const x of row(3, 34, 62)) s += pin(x - 2.5, 50, 5, 38, 2.5);
    s += '<path d="M24 54 V40 A24 24 0 0 1 72 40 V54 Z" fill="var(--panel)" stroke="var(--body)" stroke-width="2.5" stroke-linejoin="round"/>' + face(48, 42, 0.65);
  } else if (kind === "to220") {
    s += '<rect x="26" y="6" width="44" height="24" rx="3" fill="var(--copper)" stroke="var(--copper-line)" stroke-width="1"/><circle cx="48" cy="17" r="4.5" fill="var(--bg)"/>';
    for (const x of row(3, 34, 62)) s += pin(x - 3, 60, 6, 28, 2);
    s += body(26, 28, 44, 36, 4) + face(48, 46, 0.65);
  } else {
    // Two end caps: a resistor, a capacitor, a diode.
    s += pin(14, 34, 14, 28, 4) + pin(68, 34, 14, 28, 4) + body(24, 34, 48, 28, 6) + face(48, 47, 0.55);
  }
  return '<svg class="part-glyph" viewBox="0 0 96 96" aria-hidden="true" xmlns="http://www.w3.org/2000/svg">' + s + "</svg>";
}

// Start.js made a list; show the examples as cards instead.
renderExamples = async function () {
  const box = $("examples");
  let examples = [];
  try {
    examples = await invoke("examples", {});
  } catch {
    // Outside the app there are no examples.
  }
  if (!examples.length) {
    box.replaceChildren(el("p", { class: "cards-empty", text: "No examples here: they come with a checkout of Kirpitch." }));
    return;
  }
  box.replaceChildren(
    ...examples.map((ex, i) => {
      const card = el("div", { class: "card", role: "button", tabindex: "0" });
      card.style.animationDelay = i * 80 + "ms";
      const glyph = el("div", { class: "glyph" });
      glyph.innerHTML = partGlyph(ex.package, pinsOf(ex.package));
      card.append(glyph, el("strong", { text: ex.mpn }), el("span", { class: "meta", text: ex.package }));
      if (ex.pdf) card.append(el("span", { class: "tag", text: "datasheet included" }));
      const fresh = el("button", { type: "button", class: "fresh", text: "fresh copy", title: "Start again from the original" });
      fresh.addEventListener("click", (e) => {
        e.stopPropagation();
        openExample(ex, true);
      });
      card.append(fresh);
      const open = () => openExample(ex, false);
      card.addEventListener("click", open);
      card.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          open();
        }
      });
      return card;
    })
  );
};
renderExamples();

// ---- Pick up where you left off ---------------------------------------------------------

const RECENT_SHOWN = 5;
const GATE_LABEL = { pinout: "Pinout", footprint: "Footprint", "3d": "3D model" };
const recentInfo = new Map(); // path -> summary, or null when the file is gone
let recentRound = 0;
let recentAll = false;

function removeRecent(path) {
  try {
    localStorage.setItem("kirpitch.recent", JSON.stringify(recentPaths().filter((x) => x !== path)));
  } catch {
    // The list of recent files is only a convenience.
  }
  recentInfo.delete(path);
  renderRecent();
}

// The last two folders of a path, so the row says where the file is without a long line.
function whereIs(path) {
  const parts = path.split(/[\\/]/).slice(0, -1).filter(Boolean);
  return (parts.length > 2 ? "…/" : "") + parts.slice(-2).join("/");
}

function recentRow(path) {
  const info = recentInfo.get(path);
  const li = el("li", { class: "rc" + (info === null ? " missing" : "") });
  const remove = el("button", { type: "button", class: "rc-x", title: "Remove from this list (the file is not touched)", "aria-label": "Remove " + baseName(path) + " from this list", text: "×" });
  remove.addEventListener("click", () => removeRecent(path));
  if (info === null) {
    li.append(
      el("div", { class: "rc-main" },
        el("span", { class: "rc-glyph dim", text: "?" }),
        el("span", { class: "rc-text" },
          el("strong", { text: baseName(path) }),
          el("span", { class: "rc-sub warn", text: "Not found: it was moved, renamed or deleted." }),
          el("span", { class: "rc-path", title: path, text: path }))),
      remove
    );
    return li;
  }
  const open = el("button", { type: "button", class: "rc-main" });
  const glyph = el("span", { class: "rc-glyph" });
  const text = el("span", { class: "rc-text" });
  const status = el("span", { class: "rc-status" });
  if (info) {
    glyph.innerHTML = partGlyph(info.package, info.pin_count);
    const title = el("strong", { text: info.mpn || baseName(path) });
    text.append(title);
    if (/kirpitch-examples/i.test(path)) text.append(el("span", { class: "tag", text: "example copy" }));
    text.append(
      el("span", { class: "rc-sub", text: [info.package, info.manufacturer].filter(Boolean).join(" · ") }),
      el("span", { class: "rc-path", title: path, text: whereIs(path) + (info.modified ? " · edited " + libAgo(info.modified) : "") })
    );
    const all = info.gates > 0 && info.approved === info.gates;
    const segs = el("span", { class: "rc-segs", "aria-hidden": "true" });
    for (let i = 0; i < info.gates; i++) {
      const cls = i < info.approved ? "on" : i === info.approved && info.stale > 0 ? "stale" : "";
      segs.append(el("i", { class: cls }));
    }
    const words = all ? "Ready to export" : info.stale > 0 ? "Changed since approved" : info.approved + " of " + info.gates + " approved";
    status.append(segs, el("span", { class: "rc-words" + (all ? " ok" : info.stale > 0 ? " warn" : ""), text: words }));
    const next = GATE_LABEL[info.next_gate];
    status.append(el("span", { class: "rc-go", text: all ? "Open →" : "Continue with " + (next || "the review") + " →" }));
    open.title = path;
  } else {
    // Not looked at yet, or the app cannot say (a browser): the name and what was last known.
    glyph.innerHTML = partGlyph("", 8);
    text.append(el("strong", { text: baseName(path) }), el("span", { class: "rc-path", title: path, text: whereIs(path) }));
    const done = progressOf(path);
    if (done) status.append(el("span", { class: "rc-words", text: done.replace("/", " of ") + " approved" }));
  }
  open.append(glyph, text, status);
  open.addEventListener("click", () => openPath(path));
  li.append(open, remove);
  return li;
}

function paintRecent() {
  const list = $("recent-list");
  const paths = recentPaths();
  if (!paths.length) {
    list.replaceChildren(el("li", {}, withMascot("Nothing here yet. Open or build a part and it will wait for you here.")));
    return;
  }
  const shown = recentAll ? paths : paths.slice(0, RECENT_SHOWN);
  list.replaceChildren(...shown.map(recentRow));
  if (paths.length > RECENT_SHOWN) {
    const more = el("button", { type: "button", class: "ghost rc-more", text: recentAll ? "Show fewer" : "Show " + (paths.length - RECENT_SHOWN) + " more" });
    more.addEventListener("click", () => {
      recentAll = !recentAll;
      paintRecent();
    });
    list.append(el("li", { class: "rc-foot" }, more));
  }
}

// The rows appear at once from what is known, and are filled in when the files have been read.
renderRecentList = function () {
  paintRecent();
  const paths = recentPaths();
  const round = ++recentRound;
  invoke("part_summaries", { paths })
    .then((infos) => {
      if (round !== recentRound) return;
      paths.forEach((p, i) => recentInfo.set(p, infos[i] || null));
      paintRecent();
    })
    .catch(() => {});
};
renderRecentList();

// ---- Pops and the celebration ------------------------------------------------------

function celebrate() {
  if (reduceMotion()) return;
  const box = el("div", { class: "confetti", "aria-hidden": "true" });
  const colors = ["var(--copper)", "var(--brick)", "var(--ok)", "var(--copper-line)", "var(--warn)"];
  for (let i = 0; i < 70; i++) {
    const bit = document.createElement("i");
    bit.style.setProperty("--x", Math.random() * 100 + "%");
    bit.style.setProperty("--h", 10 + Math.random() * 14 + "px");
    bit.style.setProperty("--c", colors[i % colors.length]);
    bit.style.setProperty("--t", 1.8 + Math.random() * 1.6 + "s");
    bit.style.setProperty("--d", Math.random() * 0.5 + "s");
    bit.style.setProperty("--dx", Math.random() * 160 - 80 + "px");
    bit.style.setProperty("--r", Math.random() * 720 - 360 + "deg");
    box.append(bit);
  }
  document.body.append(box);
  setTimeout(() => box.remove(), 4200);
}

function allApproved(view) {
  return view.gates.length > 0 && view.gates.every((g) => g.status === "approved");
}

function updateReady(view) {
  const banner = $("ready");
  banner.hidden = !allApproved(view);
}

$("ready").firstElementChild.innerHTML = MASCOT;

const renderStart = render;
render = function (view, keepSheet) {
  const before = current;
  renderStart(view, keepSheet);
  const work = $("work");
  const samePart = before && before.path === view.path;
  if (!samePart) {
    work.classList.remove("enter");
    void work.offsetWidth; // restart the animation
    work.classList.add("enter");
  }
  updateReady(view);
  if (samePart && !allApproved(before) && allApproved(view)) celebrate();
};

// ---- Words ----------------------------------------------------------------------------

