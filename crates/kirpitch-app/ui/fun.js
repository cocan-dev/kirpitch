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

function pinsOf(packageName) {
  const m = /-(\d+)/.exec(packageName || "");
  return m ? Number(m[1]) : 8;
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
      glyph.innerHTML = chipSvg(pinsOf(ex.package), false).replace('class="chip-art"', "");
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

// ---- Recent list: friendlier empty state ---------------------------------------

const renderRecentListBase = renderRecentList;
renderRecentList = function () {
  renderRecentListBase();
  const list = $("recent-list");
  if (list.children.length === 1 && list.firstChild.classList.contains("meta")) {
    list.replaceChildren(el("li", {}, withMascot("Nothing here yet. Open something and I will remember it!")));
  }
};
renderRecentList();

// ---- Findings: no news is good news ----------------------------------------------

const renderFindingsBase = renderFindings;
renderFindings = function (view) {
  renderFindingsBase(view);
  if (!view.findings.length) $("view-findings").replaceChildren(withMascot("No findings. Suspiciously perfect."));
};

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
$("ready-write").addEventListener("click", () => {
  $("out-dir").scrollIntoView({ behavior: reduceMotion() ? "auto" : "smooth", block: "center" });
  $("out-dir").focus();
});

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
  if (samePart) {
    const chips = document.querySelectorAll("#gates .chip");
    view.gates.forEach((g, i) => {
      if (before.gates[i] && before.gates[i].status !== g.status && chips[i]) chips[i].classList.add("pop");
    });
  }
  updateReady(view);
  if (samePart && !allApproved(before) && allApproved(view)) celebrate();
};

// ---- Words ----------------------------------------------------------------------------

// Messages while a datasheet is read.
(() => {
  const lines = [
    "Reading the datasheet…",
    "Squinting at the pin table…",
    "Checking every pin against the page…",
    "Measuring the package outline…",
    "Asking the model nicely…",
    "Almost there, probably…",
  ];
  let timer = null;
  const busy = $("ds-busy");
  new MutationObserver(() => {
    if (busy.hidden) {
      clearInterval(timer);
      timer = null;
      return;
    }
    let i = 0;
    busy.textContent = lines[0];
    timer = setInterval(() => {
      i = (i + 1) % lines.length;
      busy.textContent = lines[i];
    }, 2600);
  }).observe(busy, { attributes: true, attributeFilter: ["hidden"] });
})();
