// Menu radial do ShiftDrop (estilo Tangerine). Sem botões magnéticos nem
// cursor customizado: só realce suave do setor sob o cursor.
const T = window.__TAURI__;
const invoke = T.core.invoke;
const NS = "http://www.w3.org/2000/svg";
const C = 190;
const GAP = 1.5; // meia-largura do vão entre setores, em px

const root = document.getElementById("root");
const ring = document.getElementById("ring");
const center = document.getElementById("center");
const thumb = document.getElementById("thumb");
const ctitle = document.getElementById("ctitle");
const csub = document.getElementById("csub");

// Ícones de traço (24x24).
const ICONS = {
  image: '<rect x="3" y="4" width="18" height="16" rx="2.5"/><circle cx="9" cy="10" r="1.6"/><path d="M21 16l-5-5-9 9"/>',
  camera: '<path d="M4 7.5h3l1.8-2.5h6.4L17 7.5h3a1 1 0 0 1 1 1V19a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V8.5a1 1 0 0 1 1-1z"/><circle cx="12" cy="13.5" r="3.5"/>',
  globe: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3c2.5 2.7 3.6 5.7 3.6 9s-1.1 6.3-3.6 9c-2.5-2.7-3.6-5.7-3.6-9S9.5 5.7 12 3z"/>',
  doc: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/><path d="M9 13h6M9 17h4"/>',
  compress: '<path d="M4 14h6v6"/><path d="M20 10h-6V4"/><path d="M14 10l7-7"/><path d="M3 21l7-7"/>',
  crop: '<path d="M6 2v14a2 2 0 0 0 2 2h14"/><path d="M18 22V8a2 2 0 0 0-2-2H2"/>',
  resize: '<rect x="3" y="3" width="18" height="18" rx="2.5"/><path d="M12 21v-7a2 2 0 0 1 2-2h7"/>',
  film: '<rect x="2.5" y="5" width="14" height="14" rx="2.5"/><path d="M16.5 10l5-3v10l-5-3z"/>',
  gif: '<rect x="3" y="3" width="18" height="18" rx="2.5"/><path d="M7.5 3v18M16.5 3v18M3 8h4.5M3 16h4.5M16.5 8H21M16.5 16H21"/>',
  music: '<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>',
  wave: '<path d="M2 12h2M6 8v8M10 4.5v15M14 8.5v7M18 6v12M22 12h-2"/>',
  speaker: '<path d="M11 5L6 9H2v6h4l5 4z"/><path d="M15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14"/>',
  link: '<path d="M10 13a5 5 0 0 0 7.07 0l3-3a5 5 0 0 0-7.07-7.07l-1.5 1.5"/><path d="M14 11a5 5 0 0 0-7.07 0l-3 3a5 5 0 0 0 7.07 7.07l1.5-1.5"/>',
  file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
  drop: '<path d="M12 4v11"/><path d="M7 10l5 5 5-5"/><path d="M5 20h14"/>',
};
const ACTION_ICON = {
  to_png: "image", to_jpg: "camera", to_webp: "globe", to_pdf: "doc", doc_to_pdf: "doc",
  compress_image: "compress", compress_video: "compress", compress_audio: "compress",
  crop_square: "crop", resize_half: "resize", to_mp4: "film", to_webm: "globe", to_gif: "gif",
  extract_mp3: "music", to_mp3: "music", to_wav: "wave", to_ogg: "speaker",
  merge_video: "link", merge_audio: "link",
};

let actions = [];
let groups = []; // [{name, first, last}]
let twoRings = false;
let rot = 0; // giro do anel (primeira categoria centrada no topo)
let hover = -1;
let busy = false;
let dropped = false;
let hideTimer = null;
let idle = ["ShiftDrop", "Solte numa opção"];
let currentPaths = [];

const basename = (p) => String(p).split(/[\\/]/).pop();
const ext = (p) => (basename(p).split(".").pop() || "").toUpperCase().slice(0, 4);

// ---------- geometria ----------
function geo() {
  return twoRings
    ? { center: 58, in1: 62, in2: 92, out1: 95, out2: 184 }
    : { center: 58, out1: 62, out2: 184 };
}
function polar(r, a) { return [C + r * Math.sin(a), C - r * Math.cos(a)]; }
function annulus(r1, r2) {
  return `M${C - r2} ${C}a${r2} ${r2} 0 1 0 ${2 * r2} 0a${r2} ${r2} 0 1 0 ${-2 * r2} 0Z` +
         `M${C - r1} ${C}a${r1} ${r1} 0 1 0 ${2 * r1} 0a${r1} ${r1} 0 1 0 ${-2 * r1} 0Z`;
}
// Setor de anel com vão de largura constante.
function sector(r1, r2, a0, a1) {
  if (a1 - a0 >= 2 * Math.PI - 1e-6) return annulus(r1, r2);
  const o0 = a0 + GAP / r2, o1 = a1 - GAP / r2, i0 = a0 + GAP / r1, i1 = a1 - GAP / r1;
  const [x0, y0] = polar(r2, o0), [x1, y1] = polar(r2, o1);
  const [x2, y2] = polar(r1, i1), [x3, y3] = polar(r1, i0);
  const lo = o1 - o0 > Math.PI ? 1 : 0, li = i1 - i0 > Math.PI ? 1 : 0;
  return `M${x0} ${y0}A${r2} ${r2} 0 ${lo} 1 ${x1} ${y1}L${x2} ${y2}A${r1} ${r1} 0 ${li} 0 ${x3} ${y3}Z`;
}
function el(tag, attrs, html) {
  const e = document.createElementNS(NS, tag);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  if (html != null) e.innerHTML = html;
  return e;
}
function text(x, y, s, cls) {
  const t = el("text", { x, y, class: cls || "" });
  t.textContent = s;
  return t;
}

// ---------- desenho ----------
function render() {
  ring.innerHTML = "";
  const g = geo();
  const n = actions.length;
  if (!n) {
    ring.appendChild(el("path", { d: annulus(g.out1, g.out2), class: "empty", "fill-rule": "evenodd" }));
    return;
  }
  const step = (2 * Math.PI) / n;
  const hoverGroup = hover >= 0 ? actions[hover].group : null;

  if (twoRings) {
    for (const grp of groups) {
      const a0 = (grp.first - 0.5) * step + rot, a1 = (grp.last + 0.5) * step + rot;
      const c = el("g", { class: "cat" + (grp.name === hoverGroup ? " active" : "") });
      c.appendChild(el("path", { d: sector(g.in1, g.in2, a0, a1), "fill-rule": "evenodd" }));
      // Rótulo curvo ao longo do anel, sempre de pé.
      const rm = (g.in1 + g.in2) / 2 - 3.5, mid = (a0 + a1) / 2;
      const span = Math.min((a1 - a0) / 2 - 0.05, 1.3);
      const bottom = Math.cos(mid) < 0;
      const id = "cat" + grp.first;
      const r = bottom ? rm + 7 : rm;
      const [sx, sy] = polar(r, bottom ? mid + span : mid - span);
      const [ex, ey] = polar(r, bottom ? mid - span : mid + span);
      ring.appendChild(el("path", { id, d: `M${sx} ${sy}A${r} ${r} 0 0 ${bottom ? 0 : 1} ${ex} ${ey}`, fill: "none" }));
      const t = el("text", {});
      const tp = el("textPath", { href: "#" + id, startOffset: "50%" });
      tp.textContent = grp.name;
      t.appendChild(tp);
      c.appendChild(t);
      ring.appendChild(c);
    }
  }

  actions.forEach((a, i) => {
    const s = el("g", { class: "seg" + (i === hover ? " hover" : "") + (a.available ? "" : " disabled") });
    s.appendChild(el("path", { d: sector(g.out1, g.out2, (i - 0.5) * step + rot, (i + 0.5) * step + rot), "fill-rule": "evenodd" }));
    const [tx, ty] = n === 1 ? [C, C - (g.out1 + g.out2) / 2] : polar((g.out1 + g.out2) / 2 + 2, i * step + rot);
    const icon = ICONS[ACTION_ICON[a.id] || "file"];
    s.appendChild(el("g", { class: "icon", transform: `translate(${tx - 11} ${ty - 21}) scale(0.92)` }, icon));
    s.appendChild(text(tx, ty + 13, a.label));
    ring.appendChild(s);
  });
}

function setCenter(title, sub, cls) {
  ctitle.textContent = title;
  csub.textContent = sub || "";
  center.className = cls || "";
}
function setThumbIcon(name, badge) {
  thumb.innerHTML = `<svg viewBox="0 0 24 24">${ICONS[name] || ICONS.file}${
    badge ? `<text class="badge" x="12" y="17.5">${badge}</text>` : ""}</svg>`;
}
function setThumbSpinner() { thumb.innerHTML = '<div class="spin"></div>'; }

function idleCenter() {
  if (!currentPaths.length) setThumbIcon("drop");
  setCenter(idle[0], idle[1], idle[2]);
}

// ---------- hit test ----------
// >=0 ação | -2 centro (cancelar) | -3 anel de categorias | -1 fora
function hitTest(pos) {
  if (!pos) return -1;
  const dpr = window.devicePixelRatio || 1;
  const x = pos.x / dpr - C, y = pos.y / dpr - C;
  const d = Math.hypot(x, y);
  const g = geo();
  if (d < g.center) return -2;
  if (d > g.out2 + 4 || !actions.length) return -1;
  if (twoRings && d < g.out1) return -3;
  const step = (2 * Math.PI) / actions.length;
  const a = (((Math.atan2(x, -y) - rot) % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI);
  return Math.round(a / step) % actions.length;
}

function setHover(idx) {
  if (busy || idx === hover) return;
  hover = idx;
  render();
  if (idx >= 0) {
    const a = actions[idx];
    setCenter(idle[0], a.available ? `→ ${a.label}` : a.reason || "Indisponível", a.available ? "" : "err");
  } else if (idx === -3) {
    setCenter(idle[0], "Escolha uma ação no anel externo");
  } else {
    idleCenter();
  }
}

// ---------- ciclo de vida ----------
function hide() {
  clearTimeout(hideTimer);
  root.classList.remove("in");
  actions = []; groups = []; hover = -1; busy = false; currentPaths = [];
  invoke("hide_radial");
}
function hideSoon(ms) { clearTimeout(hideTimer); hideTimer = setTimeout(hide, ms); }

async function onEnter(paths) {
  clearTimeout(hideTimer);
  currentPaths = paths;
  actions = paths.length ? await invoke("list_actions", { paths }) : [];
  groups = [];
  actions.forEach((a, i) => {
    const last = groups[groups.length - 1];
    if (last && last.name === a.group) last.last = i;
    else groups.push({ name: a.group, first: i, last: i });
  });
  twoRings = groups.length > 1 && actions.length > 5;
  rot = twoRings ? -((groups[0].first + groups[0].last) / 2) * ((2 * Math.PI) / actions.length) : 0;
  hover = -1;
  if (!actions.length) idle = ["Sem ações", "Tipo não suportado", "err"];
  else idle = [paths.length > 1 ? `${paths.length} arquivos` : basename(paths[0]), "Escolha uma opção", ""];
  render();
  setThumbIcon("file", ext(paths[0] || ""));
  setCenter(...idle);
  if (paths.length === 1) {
    invoke("thumbnail", { path: paths[0] }).then((url) => {
      if (url && currentPaths[0] === paths[0] && !busy) thumb.innerHTML = `<img src="${url}" alt="">`;
    });
  }
}

async function onDrop(paths, idx) {
  dropped = true;
  const a = actions[idx];
  if (idx < 0 || !a) return hideSoon(120);
  if (!a.available) {
    setCenter("Indisponível", a.reason, "err");
    return hideSoon(1600);
  }
  busy = true;
  setThumbSpinner();
  setCenter("Convertendo…", a.label, "");
  try {
    const outs = await invoke("run_action", { actionId: a.id, paths });
    setThumbIcon(ACTION_ICON[a.id] || "file");
    setCenter("Pronto", outs.length === 1 ? basename(outs[0]) : `${outs.length} arquivos`, "ok");
  } catch (e) {
    setThumbIcon("file");
    setCenter("Erro", String(e).slice(0, 80), "err");
  }
  busy = false;
  hideSoon(1800);
}

T.webview.getCurrentWebview().onDragDropEvent(async (ev) => {
  const p = ev.payload;
  if (busy) return;
  if (p.type === "enter") {
    await onEnter(p.paths || []);
    setHover(hitTest(p.position));
  } else if (p.type === "over") {
    setHover(hitTest(p.position));
  } else if (p.type === "leave") {
    setHover(-1);
  } else if (p.type === "drop") {
    await onDrop(p.paths || currentPaths, hitTest(p.position));
  }
});

T.event.listen("radial-show", (e) => {
  clearTimeout(hideTimer);
  const manual = !!(e.payload && e.payload.manual);
  actions = []; groups = []; twoRings = false; rot = 0; hover = -1; busy = false; dropped = false; currentPaths = [];
  idle = ["ShiftDrop", manual ? "Arraste um arquivo até aqui" : "Solte numa opção", ""];
  render();
  idleCenter();
  root.classList.remove("in"); void root.offsetWidth; root.classList.add("in");
  if (manual) hideSoon(15000);
});

// Modo demonstração (shiftdrop.exe preview <arquivo> --hover N).
T.event.listen("radial-preview", async (e) => {
  await onEnter(e.payload.paths || []);
  setHover(e.payload.hover);
});

T.event.listen("gesture-end", () => {
  setTimeout(() => { if (!dropped && !busy) hide(); }, 600);
});

center.addEventListener("click", () => { if (!busy) hide(); });
window.addEventListener("keydown", (e) => { if (e.key === "Escape" && !busy) hide(); });

render();
idleCenter();
