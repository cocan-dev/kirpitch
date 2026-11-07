// SPDX-License-Identifier: GPL-3.0-or-later
"use strict";

// A 3D view of the model (boxes: the body, the pin 1 mark and the terminals) that
// can be turned, moved and zoomed. Plain WebGL, no library, nothing to download.
//
// Mouse: drag turns, right-drag (or Shift+drag, or middle) moves, the wheel zooms,
// double-click shows all of it again. Touch: one finger turns, two fingers move and
// zoom. Keys (with the view in focus): arrows turn, + and - zoom, 0 resets.
// Coordinates are the model's: millimetres, x right, y up the board, z up from it.

const M3D_FOV = (35 * Math.PI) / 180;
const M3D_VIEWS = {
  iso: { az: -55, el: 28 },
  top: { az: -90, el: 89 },
  front: { az: -90, el: 0 },
  left: { az: 180, el: 0 },
};

const M3D_VERTEX = `
attribute vec3 aPos; attribute vec3 aNormal; attribute vec3 aColor;
uniform mat4 uProj; uniform mat4 uView; uniform mat3 uNormal;
varying vec3 vN; varying vec3 vC;
void main() { gl_Position = uProj * uView * vec4(aPos, 1.0); vN = uNormal * aNormal; vC = aColor; }`;
const M3D_FRAGMENT = `
precision mediump float;
varying vec3 vN; varying vec3 vC;
uniform float uLit; uniform float uAlpha;
void main() {
  vec3 n = normalize(vN);
  float key = max(dot(n, normalize(vec3(0.35, 0.55, 0.75))), 0.0);
  float fill = max(dot(n, normalize(vec3(-0.6, -0.2, 0.4))), 0.0);
  vec3 lit = vC * (0.38 + 0.62 * key + 0.18 * fill);
  gl_FragColor = vec4(mix(vC, lit, uLit), uAlpha);
}`;

// ---- Small matrix helpers (column-major, as WebGL wants) ------------------------------------

function m3dPerspective(fov, aspect, near, far) {
  const f = 1 / Math.tan(fov / 2);
  const nf = 1 / (near - far);
  return new Float32Array([f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) * nf, -1, 0, 0, 2 * far * near * nf, 0]);
}

const m3dSub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const m3dCross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const m3dNorm = (a) => {
  const l = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};
const m3dDot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

// Looking from `eye` at `target`, z is up. Also gives the screen's right and up in the model.
function m3dLookAt(eye, target) {
  const f = m3dNorm(m3dSub(target, eye));
  const r = m3dNorm(m3dCross(f, [0, 0, 1]));
  const u = m3dCross(r, f);
  const view = new Float32Array([r[0], u[0], -f[0], 0, r[1], u[1], -f[1], 0, r[2], u[2], -f[2], 0, -m3dDot(r, eye), -m3dDot(u, eye), m3dDot(f, eye), 1]);
  const normal = new Float32Array([r[0], u[0], -f[0], r[1], u[1], -f[1], r[2], u[2], -f[2]]);
  return { view, normal, right: r, up: u };
}

// ---- The geometry ---------------------------------------------------------------------------

// Faces of a box: a normal and the four corners (as 0/1 along x, y, z), counter-clockwise from outside.
const M3D_FACES = [
  [[1, 0, 0], [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]]],
  [[-1, 0, 0], [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]]],
  [[0, 1, 0], [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]]],
  [[0, -1, 0], [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]],
  [[0, 0, 1], [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]],
  [[0, 0, -1], [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]]],
];
const M3D_EDGES = [
  [[0, 0, 0], [1, 0, 0]], [[0, 1, 0], [1, 1, 0]], [[0, 0, 1], [1, 0, 1]], [[0, 1, 1], [1, 1, 1]],
  [[0, 0, 0], [0, 1, 0]], [[1, 0, 0], [1, 1, 0]], [[0, 0, 1], [0, 1, 1]], [[1, 0, 1], [1, 1, 1]],
  [[0, 0, 0], [0, 0, 1]], [[1, 0, 0], [1, 0, 1]], [[0, 1, 0], [0, 1, 1]], [[1, 1, 0], [1, 1, 1]],
];

// The colour as shown: the real body is almost black, which hides its shape.
function m3dColour(box) {
  const lift = box.material === "body" ? 0.14 : 0;
  return box.rgb.map((c) => Math.min(1, c * (box.material === "body" ? 0.85 : 1) + lift));
}

function m3dBuild(boxes) {
  const tri = [];
  const lines = [];
  for (const b of boxes) {
    const at = (c) => [b.min[0] + c[0] * (b.max[0] - b.min[0]), b.min[1] + c[1] * (b.max[1] - b.min[1]), b.min[2] + c[2] * (b.max[2] - b.min[2])];
    const colour = m3dColour(b);
    for (const [n, corners] of M3D_FACES) {
      const p = corners.map(at);
      for (const i of [0, 1, 2, 0, 2, 3]) tri.push(...p[i], ...n, ...colour);
    }
    for (const [a, c] of M3D_EDGES) {
      lines.push(...at(a), 0, 0, 0, 0.12, 0.12, 0.14, ...at(c), 0, 0, 0, 0.12, 0.12, 0.14);
    }
  }
  return { tri: new Float32Array(tri), lines: new Float32Array(lines) };
}

// ---- The view -------------------------------------------------------------------------------

function model3d(holder) {
  if (holder._m3d) return holder._m3d;
  holder.replaceChildren();
  const canvas = document.createElement("canvas");
  canvas.className = "m3d-canvas";
  canvas.tabIndex = 0;
  canvas.setAttribute("aria-label", "3D view of the part: drag to turn, right-drag to move, scroll to zoom");
  const gl = canvas.getContext("webgl", { antialias: true, alpha: true }) || canvas.getContext("experimental-webgl");
  if (!gl) {
    holder.append(Object.assign(document.createElement("p"), { className: "muted small m3d-none", textContent: "This computer cannot show the 3D view (WebGL is not available). The drawings next to it show the same model." }));
    holder._m3d = { set() {}, none: true };
    return holder._m3d;
  }

  const compile = (type, src) => {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    return s;
  };
  const prog = gl.createProgram();
  gl.attachShader(prog, compile(gl.VERTEX_SHADER, M3D_VERTEX));
  gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, M3D_FRAGMENT));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    holder.append(Object.assign(document.createElement("p"), { className: "muted small m3d-none", textContent: "The 3D view could not start on this computer. The drawings next to it show the same model." }));
    holder._m3d = { set() {}, none: true };
    return holder._m3d;
  }
  gl.useProgram(prog);
  const loc = (n) => gl.getAttribLocation(prog, n);
  const uni = (n) => gl.getUniformLocation(prog, n);
  const A = { pos: loc("aPos"), normal: loc("aNormal"), colour: loc("aColor") };
  const U = { proj: uni("uProj"), view: uni("uView"), normal: uni("uNormal"), lit: uni("uLit"), alpha: uni("uAlpha") };

  const bufs = { tri: gl.createBuffer(), lines: gl.createBuffer(), extra: gl.createBuffer(), plane: gl.createBuffer() };
  const counts = { tri: 0, lines: 0, extra: 0, plane: 0 };
  const upload = (buf, data) => {
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    return data.length / 9;
  };
  const draw = (buf, count, mode) => {
    if (!count) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    for (const [a, size, offset] of [[A.pos, 3, 0], [A.normal, 3, 12], [A.colour, 3, 24]]) {
      gl.enableVertexAttribArray(a);
      gl.vertexAttribPointer(a, size, gl.FLOAT, false, 36, offset);
    }
    gl.drawArrays(mode, 0, count);
  };

  // The camera.
  const cam = { target: [0, 0, 0], dist: 10, az: M3D_VIEWS.iso.az, el: M3D_VIEWS.iso.el, radius: 5 };
  const state = { boxes: [], dirty: true, path: null };

  const frame = () => {
    const rad = (d) => (d * Math.PI) / 180;
    const eye = [
      cam.target[0] + cam.dist * Math.cos(rad(cam.el)) * Math.cos(rad(cam.az)),
      cam.target[1] + cam.dist * Math.cos(rad(cam.el)) * Math.sin(rad(cam.az)),
      cam.target[2] + cam.dist * Math.sin(rad(cam.el)),
    ];
    return m3dLookAt(eye, cam.target);
  };

  const fit = (view) => {
    const v = M3D_VIEWS[view || "iso"];
    cam.az = v.az;
    cam.el = v.el;
    cam.dist = (cam.radius / Math.sin(M3D_FOV / 2)) * 1.05;
    cam.target = state.centre.slice();
    state.dirty = true;
  };

  const set = (boxes, keepCamera) => {
    state.boxes = boxes || [];
    if (!state.boxes.length) {
      counts.tri = counts.lines = counts.extra = counts.plane = 0;
      state.dirty = true;
      return;
    }
    const lo = [Infinity, Infinity, Infinity];
    const hi = [-Infinity, -Infinity, -Infinity];
    for (const b of state.boxes) for (let i = 0; i < 3; i++) {
      lo[i] = Math.min(lo[i], b.min[i]);
      hi[i] = Math.max(hi[i], b.max[i]);
    }
    state.centre = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2];
    cam.radius = Math.max(Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) / 2, 0.5);
    const built = m3dBuild(state.boxes);
    counts.tri = upload(bufs.tri, built.tri);
    counts.lines = upload(bufs.lines, built.lines);
    // The axes (x red, y green, z blue) from the corner of the part, and the board as a faint square.
    const len = cam.radius * 0.9;
    const o = [lo[0], lo[1], Math.min(lo[2], 0)];
    const axis = (d, c) => [...o, 0, 0, 0, ...c, o[0] + d[0] * len, o[1] + d[1] * len, o[2] + d[2] * len, 0, 0, 0, ...c];
    counts.extra = upload(bufs.extra, new Float32Array([...axis([1, 0, 0], [0.85, 0.2, 0.2]), ...axis([0, 1, 0], [0.2, 0.65, 0.25]), ...axis([0, 0, 1], [0.25, 0.4, 0.9])]));
    const half = Math.max(hi[0] - lo[0], hi[1] - lo[1]) * 0.9 + 1;
    const [cx, cy] = state.centre;
    const q = (x, y) => [x, y, 0, 0, 0, 1, 0.1, 0.42, 0.22];
    counts.plane = upload(bufs.plane, new Float32Array([...q(cx - half, cy - half), ...q(cx + half, cy - half), ...q(cx + half, cy + half), ...q(cx - half, cy - half), ...q(cx + half, cy + half), ...q(cx - half, cy + half)]));
    if (!keepCamera) fit("iso");
    state.dirty = true;
  };

  const size = () => {
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.max(1, Math.round(canvas.clientWidth * ratio));
    const h = Math.max(1, Math.round(canvas.clientHeight * ratio));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
      state.dirty = true;
    }
  };

  const paint = () => {
    state.dirty = false;
    size();
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    if (!counts.tri) return;
    const { view, normal } = frame();
    const proj = m3dPerspective(M3D_FOV, canvas.width / canvas.height, Math.max(cam.dist * 0.02, 0.01), cam.dist + cam.radius * 6);
    gl.uniformMatrix4fv(U.proj, false, proj);
    gl.uniformMatrix4fv(U.view, false, view);
    gl.uniformMatrix3fv(U.normal, false, normal);
    gl.enable(gl.DEPTH_TEST);
    gl.disable(gl.BLEND);
    gl.enable(gl.POLYGON_OFFSET_FILL);
    gl.polygonOffset(1, 1);
    gl.uniform1f(U.lit, 1);
    gl.uniform1f(U.alpha, 1);
    draw(bufs.tri, counts.tri, gl.TRIANGLES);
    gl.disable(gl.POLYGON_OFFSET_FILL);
    gl.uniform1f(U.lit, 0);
    draw(bufs.lines, counts.lines, gl.LINES);
    gl.disable(gl.DEPTH_TEST);
    draw(bufs.extra, counts.extra, gl.LINES);
    gl.enable(gl.DEPTH_TEST);
    // The board, see-through, behind everything else that was drawn.
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.depthMask(false);
    gl.uniform1f(U.alpha, 0.22);
    draw(bufs.plane, counts.plane, gl.TRIANGLES);
    gl.depthMask(true);
  };
  const loop = () => {
    if (state.dirty) paint();
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
  new ResizeObserver(() => (state.dirty = true)).observe(canvas);

  // ---- The mouse, touch and keys ----
  const clampEl = () => (cam.el = Math.max(-89, Math.min(89.5, cam.el)));
  const turn = (dx, dy) => {
    cam.az -= dx * 0.4;
    cam.el += dy * 0.4;
    clampEl();
    state.dirty = true;
  };
  const pan = (dx, dy) => {
    const { right, up } = frame();
    const k = (2 * cam.dist * Math.tan(M3D_FOV / 2)) / Math.max(canvas.clientHeight, 1);
    for (let i = 0; i < 3; i++) cam.target[i] += (-dx * right[i] + dy * up[i]) * k;
    state.dirty = true;
  };
  const zoom = (factor) => {
    cam.dist = Math.max(cam.radius * 0.25, Math.min(cam.radius * 40, cam.dist * factor));
    state.dirty = true;
  };

  const pointers = new Map();
  let last = null;
  canvas.addEventListener("pointerdown", (ev) => {
    canvas.focus({ preventScroll: true });
    canvas.setPointerCapture(ev.pointerId);
    pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
    last = null;
  });
  canvas.addEventListener("pointermove", (ev) => {
    const p = pointers.get(ev.pointerId);
    if (!p) return;
    const dx = ev.clientX - p.x;
    const dy = ev.clientY - p.y;
    if (pointers.size === 1) {
      if (ev.buttons & 2 || ev.buttons & 4 || ev.shiftKey) pan(dx, dy);
      else turn(dx, dy);
    } else if (pointers.size === 2) {
      // Two fingers: the gap between them zooms, their middle moves.
      const other = [...pointers.entries()].find(([id]) => id !== ev.pointerId)[1];
      const before = Math.hypot(p.x - other.x, p.y - other.y);
      const after = Math.hypot(ev.clientX - other.x, ev.clientY - other.y);
      if (before > 0 && after > 0) zoom(before / after);
      pan(dx / 2, dy / 2);
    }
    p.x = ev.clientX;
    p.y = ev.clientY;
  });
  const up = (ev) => pointers.delete(ev.pointerId);
  canvas.addEventListener("pointerup", up);
  canvas.addEventListener("pointercancel", up);
  canvas.addEventListener("contextmenu", (ev) => ev.preventDefault());
  canvas.addEventListener(
    "wheel",
    (ev) => {
      ev.preventDefault();
      zoom(Math.exp(ev.deltaY * (ev.deltaMode === 1 ? 0.05 : 0.0015)));
    },
    { passive: false }
  );
  canvas.addEventListener("dblclick", () => fit("iso"));
  canvas.addEventListener("keydown", (ev) => {
    const keys = {
      ArrowLeft: () => turn(-12, 0),
      ArrowRight: () => turn(12, 0),
      ArrowUp: () => turn(0, 12),
      ArrowDown: () => turn(0, -12),
      "+": () => zoom(1 / 1.25),
      "=": () => zoom(1 / 1.25),
      "-": () => zoom(1.25),
      0: () => fit("iso"),
    };
    if (keys[ev.key]) {
      ev.preventDefault();
      keys[ev.key]();
    }
  });

  // The buttons above the view.
  const bar = document.createElement("div");
  bar.className = "m3d-bar";
  const button = (text, label, run) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "ghost";
    b.textContent = text;
    b.title = label;
    b.setAttribute("aria-label", label);
    b.addEventListener("click", run);
    return b;
  };
  bar.append(
    button("3D", "Turned view", () => fit("iso")),
    button("Top", "Look from above", () => fit("top")),
    button("Front", "Look from the front", () => fit("front")),
    button("Left", "Look from the left", () => fit("left")),
    button("+", "Zoom in", () => zoom(1 / 1.4)),
    button("−", "Zoom out", () => zoom(1.4))
  );
  const hint = document.createElement("p");
  hint.className = "m3d-hint muted small";
  hint.textContent = "Drag to turn · right-drag or Shift+drag to move · scroll to zoom · double-click to start again";
  const stage = document.createElement("div");
  stage.className = "m3d-stage";
  stage.append(canvas);
  holder.append(bar, stage, hint);

  holder._m3d = { set, fit, state, cam };
  return holder._m3d;
}

// Shows the boxes of a part; the camera is kept when it is the same part redrawn after an edit.
function model3dShow(boxes, path) {
  const holder = document.getElementById("model3d");
  if (!holder) return;
  const v = model3d(holder);
  const same = v.state && v.state.path === path;
  v.set(boxes, same);
  if (v.state) v.state.path = path;
}
