// SPDX-License-Identifier: GPL-3.0-or-later

// Zoom and pan for a drawing (an SVG with a viewBox) in a holder element:
// the wheel zooms around the pointer, dragging moves the drawing, the buttons
// (and the keys + - 0 and the arrows, when the drawing has the focus) do the same,
// and double-click or Fit shows all of it again. The drawing is replaced by a new
// one on every render; call zoomable(holder) after each one.

const ZOOM_MIN = 0.7;
const ZOOM_MAX = 60;

function zoomable(holder) {
  const svg = holder.querySelector("svg");
  holder.classList.toggle("zoomable", !!svg);
  holder.querySelectorAll(".zoombar").forEach((n) => n.remove());
  if (!svg || !svg.viewBox || !svg.viewBox.baseVal || !svg.viewBox.baseVal.width) return;
  const vb = svg.viewBox.baseVal;
  const base = { x: vb.x, y: vb.y, w: vb.width, h: vb.height };
  // The same drawing, redrawn after an edit, keeps the zoom the person chose.
  const before = holder._zoom;
  const same = before && Math.abs(before.base.w - base.w) < 1e-6 && Math.abs(before.base.h - base.h) < 1e-6;
  let view = same ? { ...before.get() } : { ...base };
  if (same) svg.setAttribute("viewBox", [view.x, view.y, view.w, view.h].join(" "));
  holder._zoom ={ svg, base, get: () => view, set: (v) => (view = v) };

  const apply = () => svg.setAttribute("viewBox", [view.x, view.y, view.w, view.h].join(" "));
  const scale = () => base.w / view.w;
  // Zoom by a factor, keeping the point (px, py) of the drawing where it is on screen.
  const zoomAt = (factor, px, py) => {
    const s = Math.min(Math.max(scale() * factor, ZOOM_MIN), ZOOM_MAX);
    const w = base.w / s;
    const h = base.h / s;
    view = { x: px - ((px - view.x) * w) / view.w, y: py - ((py - view.y) * h) / view.h, w, h };
    apply();
  };
  const centre = () => [view.x + view.w / 2, view.y + view.h / 2];
  const fit = () => {
    view = { ...base };
    apply();
  };
  const pan = (dx, dy) => {
    view = { ...view, x: view.x + dx, y: view.y + dy };
    apply();
  };

  // Where a screen point is in the drawing.
  const toDrawing = (clientX, clientY) => {
    const m = svg.getScreenCTM();
    if (!m) return centre();
    const p = new DOMPoint(clientX, clientY).matrixTransform(m.inverse());
    return [p.x, p.y];
  };

  const bar = document.createElement("div");
  bar.className = "zoombar";
  bar.setAttribute("role", "group");
  bar.setAttribute("aria-label", "Zoom the drawing");
  const button = (text, label, run) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "ghost";
    b.textContent = text;
    b.title = label;
    b.setAttribute("aria-label", label);
    b.addEventListener("click", (ev) => {
      ev.stopPropagation();
      run();
    });
    return b;
  };
  bar.append(
    button("+", "Zoom in", () => zoomAt(1.5, ...centre())),
    button("−", "Zoom out", () => zoomAt(1 / 1.5, ...centre())),
    button("Fit", "Show all of the drawing", fit)
  );
  holder.append(bar);

  holder._zoomApi = { zoomAt, centre, fit, pan, toDrawing, scale };

  if (holder.dataset.zoomReady) return;
  holder.dataset.zoomReady = "1";
  holder.tabIndex = 0;
  holder.setAttribute("aria-label", "Drawing: scroll to zoom, drag to move, Fit to show all");

  holder.addEventListener(
    "wheel",
    (ev) => {
      const api = holder._zoomApi;
      if (!api || !holder.querySelector("svg")) return;
      ev.preventDefault();
      const [px, py] = api.toDrawing(ev.clientX, ev.clientY);
      api.zoomAt(Math.exp(-ev.deltaY * (ev.deltaMode === 1 ? 0.05 : 0.0015)), px, py);
    },
    { passive: false }
  );

  // Dragging moves the drawing; a drag is not a click on a pin.
  let drag = null;
  let moved = false;
  holder.addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0 || ev.target.closest(".zoombar") || !holder._zoomApi) return;
    drag = { x: ev.clientX, y: ev.clientY, id: ev.pointerId, captured: false };
    moved = false;
  });
  holder.addEventListener("pointermove", (ev) => {
    if (!drag || ev.pointerId !== drag.id) return;
    const dx = ev.clientX - drag.x;
    const dy = ev.clientY - drag.y;
    if (!moved && Math.hypot(dx, dy) < 4) return;
    if (!moved) {
      moved = true;
      holder.classList.add("panning");
      try {
        holder.setPointerCapture(ev.pointerId);
      } catch {
        // Not every pointer can be captured.
      }
    }
    const m = holder.querySelector("svg").getScreenCTM();
    if (!m) return;
    holder._zoomApi.pan(-dx / m.a, -dy / m.d);
    drag.x = ev.clientX;
    drag.y = ev.clientY;
  });
  const endDrag = (ev) => {
    if (!drag || ev.pointerId !== drag.id) return;
    drag = null;
    holder.classList.remove("panning");
  };
  holder.addEventListener("pointerup", endDrag);
  holder.addEventListener("pointercancel", endDrag);
  // Runs before the click on a pin, and stops it after a drag.
  holder.addEventListener(
    "click",
    (ev) => {
      if (moved) {
        moved = false;
        ev.stopPropagation();
        ev.preventDefault();
      }
    },
    true
  );
  holder.addEventListener("dblclick", (ev) => {
    if (ev.target.closest(".zoombar") || !holder._zoomApi) return;
    holder._zoomApi.fit();
  });
  holder.addEventListener("keydown", (ev) => {
    const api = holder._zoomApi;
    if (!api || ev.target !== holder) return;
    const step = 0.1 * (holder._zoom ? holder._zoom.get().w : 10);
    const keys = {
      "+": () => api.zoomAt(1.5, ...api.centre()),
      "=": () => api.zoomAt(1.5, ...api.centre()),
      "-": () => api.zoomAt(1 / 1.5, ...api.centre()),
      0: () => api.fit(),
      ArrowLeft: () => api.pan(-step, 0),
      ArrowRight: () => api.pan(step, 0),
      ArrowUp: () => api.pan(0, -step),
      ArrowDown: () => api.pan(0, step),
    };
    if (keys[ev.key]) {
      ev.preventDefault();
      keys[ev.key]();
    }
  });
}
