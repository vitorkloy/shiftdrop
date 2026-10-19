// Roda do ShiftDrop no estilo Tangerine (geometria e cores do tangerineformac.com).
// Três rodas: Shift = Convert formats, Ctrl = File tools, Alt = File actions.
// Sem botões magnéticos nem cursor customizado: só o setor sob o cursor fica laranja.
const T = window.__TAURI__;
const invoke = T.core.invoke;

// Console do JS → %LOCALAPPDATA%\ShiftDrop\log.txt
const jsLog = (level, args) => {
  try {
    const msg = args.map((a) => (typeof a === "string" ? a : JSON.stringify(a))).join(" ");
    invoke("js_log", { level, msg }).catch(() => {});
  } catch (_) {}
};
for (const level of ["log", "warn", "error"]) {
  const orig = console[level].bind(console);
  console[level] = (...a) => { orig(...a); jsLog(level, a); };
}
window.addEventListener("error", (e) => jsLog("error", [`${e.message} @${e.filename}:${e.lineno}:${e.colno}`]));
window.addEventListener("unhandledrejection", (e) => jsLog("error", ["promise: " + ((e.reason && e.reason.message) || e.reason)]));

const NS = "http://www.w3.org/2000/svg";
const VIEW = 252;           // unidades do viewBox (−126…126)
const PX = 290;             // tamanho da janela em px lógicos
const S = PX / VIEW;        // px por unidade
// Medidas do site (m = {inner, outer, backdrop, gap, corner, labelInner}).
const M = {
  inner: 48 * 0.8 * 1.1 + 8 * 0.8,
  outer: 166 * 0.8 * 0.9,
  backdrop: 166 * 0.8 * 0.9 + 8 * 0.8,
  gap: 5 * 0.8,
  corner: 28 * 0.8 * 0.9,
  labelInner: 50 * 0.8,
};
const MODE_NAME = { convert: "Convert formats", tools: "File tools", actions: "File actions" };

const root = document.getElementById("root");
const svg = document.getElementById("wheel");
const pill = document.getElementById("pill");
const empty = document.getElementById("empty");

let data = null;            // resposta de `wheels`
let mode = "convert";
let items = [];
let hover = -1;
let paths = [];
let busy = false;
let dropped = false;
let token = 0;
let hideTimer = null;

// ---------- geometria (porte direto de F/be/we do site) ----------
function F(e, t) { return (t === 3 ? Math.PI : Math.PI / 2) - (Math.PI * 2 * e) / t; }
function be(r, a) { return [Math.cos(a) * r, -Math.sin(a) * r]; }
function petalPath(e, t) {
  const a = F(e, t), n = (Math.PI * 2) / t, l = a - n / 2, s = a + n / 2;
  const r = (J) => Math.asin(Math.min(0.95, M.gap / 2 / Math.max(J, M.gap / 2 + 0.01)));
  const d = l + r(M.outer), i = s - r(M.outer), c = l + r(M.inner), g = s - r(M.inner);
  const f = Math.min(0.15, (i - d) * 0.24), u = Math.min(0.18, (g - c) * 0.18);
  const v = M.inner / Math.max(0.01, Math.cos(u)), y = v + M.inner * Math.tan(u), O = M.outer - M.corner;
  const w = (J, ang) => be(J, ang).map((b) => b.toFixed(4)).join(" ");
  return `M ${w(y, l + r(y))} L ${w(O, l + r(O))} Q ${w(M.outer, d)} ${w(M.outer, d + f)} ` +
    `A ${M.outer} ${M.outer} 0 ${i - f - d - f > Math.PI ? 1 : 0} 0 ${w(M.outer, i - f)} ` +
    `Q ${w(M.outer, i)} ${w(O, s - r(O))} L ${w(y, s - r(y))} Q ${w(v, s - r(v))} ${w(M.inner, g - u)} ` +
    `A ${M.inner} ${M.inner} 0 ${g - u - c - u > Math.PI ? 1 : 0} 1 ${w(M.inner, c + u)} Q ${w(v, l + r(v))} ${w(y, l + r(y))} Z`;
}

// >=0 setor | -2 centro (cancelar) | -1 fora. Tolerante aos vãos (o drop nunca cai "entre" setores).
function hitTest(pos) {
  if (!pos || !items.length) return pos ? centerOrOut(pos) : -1;
  const dpr = window.devicePixelRatio || 1;
  const x = (pos.x / dpr - PX / 2) / S, y = (pos.y / dpr - PX / 2) / S;
  const dist = Math.hypot(x, y);
  if (dist < M.inner - 2) return -2;
  if (dist > M.backdrop + 3) return -1;
  const n = items.length, l = Math.atan2(-y, x), step = (Math.PI * 2) / n;
  const rr = (((F(0, n) - l) % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2);
  return Math.round(rr / step) % n;
}
function centerOrOut(pos) {
  const dpr = window.devicePixelRatio || 1;
  const d = Math.hypot(pos.x / dpr - PX / 2, pos.y / dpr - PX / 2) / S;
  return d < M.inner - 2 ? -2 : -1;
}

// ---------- desenho ----------
const DEFS = `<defs>
  <radialGradient id="glass-fill" cx="0" cy="0" r="${M.outer}" gradientUnits="userSpaceOnUse">
    <stop offset="0" stop-color="#ffffff" stop-opacity=".8"/>
    <stop offset=".68" stop-color="#f3f6f7" stop-opacity=".58"/>
    <stop offset="1" stop-color="#f4f6f6" stop-opacity=".75"/>
  </radialGradient>
  <filter id="relief" x="-20%" y="-20%" width="140%" height="140%">
    <feComponentTransfer in="SourceAlpha" result="inv"><feFuncA type="table" tableValues="1 0"/></feComponentTransfer>
    <feGaussianBlur in="inv" stdDeviation="2.4" result="ib"/>
    <feOffset in="ib" dy="-1.6" result="io"/>
    <feFlood flood-color="#71808a" flood-opacity=".24"/>
    <feComposite in2="io" operator="in"/>
    <feComposite in2="SourceAlpha" operator="in" result="inner"/>
    <feGaussianBlur in="SourceAlpha" stdDeviation="1.3" result="db"/>
    <feOffset in="db" dy="1.1" result="do"/>
    <feFlood flood-color="#26343a" flood-opacity=".10"/>
    <feComposite in2="do" operator="in" result="drop"/>
    <feMerge><feMergeNode in="drop"/><feMergeNode in="SourceGraphic"/><feMergeNode in="inner"/></feMerge>
  </filter>
  <filter id="hubshadow" x="-30%" y="-30%" width="160%" height="160%">
    <feDropShadow dx="0" dy="1.2" stdDeviation="1.8" flood-color="#26343a" flood-opacity=".14"/>
  </filter>
</defs>`;

function el(tag, attrs, parent) {
  const e = document.createElementNS(NS, tag);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  if (parent) parent.appendChild(e);
  return e;
}

function render() {
  svg.innerHTML = DEFS;
  el("circle", { class: "backdrop", r: M.backdrop - 0.5 }, svg);
  el("circle", { class: "backdrop-rim", r: M.backdrop - 0.5 }, svg);
  el("circle", { class: "hub-ring", r: M.inner - 1 }, svg);
  el("circle", { class: "hub", r: M.inner - 6, filter: "url(#hubshadow)" }, svg);
  const tool = mode !== "convert";
  const n = items.length;
  items.forEach((it, i) => {
    const g = el("g", { class: `target${tool ? " tool" : ""}${i === hover ? " hi" : ""}${it.available ? "" : " off"}` }, svg);
    el("path", { class: "petal", d: petalPath(i, n), filter: i === hover ? "" : "url(#relief)" }, g);
    const [x, y] = be(M.labelInner + (M.outer - M.labelInner) * 0.56, F(i, n));
    if (tool && it.icon) {
      el("image", { class: "icon", href: `icons/${it.icon}.svg`, x: x - 9, y: y - 16.5, width: 18, height: 18 }, g);
    }
    const t = el("text", { class: "wheel-label", x, y: tool ? y + 10.5 : y, "text-anchor": "middle", "dominant-baseline": "middle" }, g);
    t.textContent = it.title;
  });
  empty.hidden = n > 0 || !data;
  if (!n && data) {
    const other = { convert: "Ctrl para ferramentas ou Alt para ações", tools: "Shift para converter ou Alt para ações", actions: "Shift para converter" };
    empty.textContent = `${mode === "convert" ? "Sem conversões" : "Sem ferramentas"} para este formato. Toque ${other[mode]}.`;
  }
  updatePill();
}

function setPill(text, cls = "") {
  pill.className = cls;
  pill.textContent = text;
}

function updatePill() {
  if (busy) return;
  const it = items[hover];
  if (it) {
    if (!it.available) return setPill(it.reason || "Indisponível", "msg err");
    return setPill(it.title, mode === "convert" ? "" : "tool");
  }
  if (!items.length) return setPill("");
  setPill(data && data.format ? data.format : "", "");
}

function setHover(i) {
  if (busy || i === hover) return;
  hover = i;
  render();
}

function bloom() {
  root.classList.remove("in");
  void root.offsetWidth;
  root.classList.add("in");
}

function applyMode(m, animate = true) {
  mode = m;
  items = (data && data[m]) || [];
  hover = -1;
  render();
  if (animate) bloom();
}

// ---------- ciclo de vida ----------
function hide() {
  clearTimeout(hideTimer);
  root.classList.remove("in");
  busy = false; hover = -1; paths = [];
  invoke("hide_radial");
}
function hideSoon(ms) { clearTimeout(hideTimer); hideTimer = setTimeout(hide, ms); }

async function load(p) {
  const my = ++token;
  let w = null;
  try { w = await invoke("wheels", { paths: p }); } catch (e) { console.error("wheels falhou", String(e)); }
  if (my !== token || !w) return;
  data = w;
  paths = p;
  items = data[mode] || [];
  hover = -1;
  render();
  if (p.length) {
    console.log(`menu: roda ${mode} com ${items.length} setores para ${p.length} item(ns) [${w.kind} ${w.format}]: ${items.map((a) => a.id).join(",")}`);
  } else {
    console.log(`menu: ${items.length} setores genéricos desenhados (roda ${mode})`);
  }
}

async function onEnter(p) {
  clearTimeout(hideTimer);
  if (!p.length) {
    console.warn("enter sem caminhos: mantendo setores genéricos");
    if (!data) await load([]);
    return;
  }
  await load(p);
}

async function onDrop(p, idx) {
  dropped = true;
  const it = items[idx];
  console.log(`drop: roda=${mode} setor=${idx} ação=${it ? it.id : "-"} arquivos=${JSON.stringify(p)}`);
  if (idx < 0 || !it) return hideSoon(120);
  if (!p.length) { setPill("O arraste não trouxe o arquivo", "msg err"); return hideSoon(1800); }
  if (!it.available) { setPill(it.reason || "Indisponível", "msg err"); return hideSoon(1700); }
  busy = true;
  hover = idx;
  render();
  pill.className = ""; pill.innerHTML = '<div class="spin"></div>';
  try {
    const outs = await invoke("run_action", { actionId: it.id, paths: p });
    busy = false;
    setPill(mode === "actions" ? "FEITO" : "PRONTO", "ok");
    console.log("conversão ok:", outs);
  } catch (e) {
    busy = false;
    setPill(String(e).slice(0, 70), "msg err");
    console.error("conversão falhou:", String(e));
  }
  hideSoon(1500);
}

// Eventos de arrastar vêm do Rust (catcher.rs/drag.rs), já com os caminhos no "enter".
T.event.listen("sd-drag", async (ev) => {
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
    await onDrop(p.paths && p.paths.length ? p.paths : paths, hitTest(p.position));
  }
});

T.event.listen("radial-show", (e) => {
  clearTimeout(hideTimer);
  const manual = !!(e.payload && e.payload.manual);
  console.log(`radial-show manual=${manual}`);
  token++;
  data = null; paths = []; busy = false; dropped = false; hover = -1;
  mode = "convert";
  items = [];
  render();
  bloom();
  load([]);
  if (manual) hideSoon(15000);
});

// Shift / Ctrl / Alt durante o arraste trocam a roda (vem do gesture.rs).
T.event.listen("wheel-mode", (e) => {
  const m = e.payload && e.payload.mode;
  if (!m || busy || m === mode) return;
  console.log(`menu: roda → ${m} (${MODE_NAME[m]})`);
  applyMode(m);
});

// Modo demonstração (shiftdrop.exe preview <arquivo> --mode tools --hover N).
T.event.listen("radial-preview", async (e) => {
  const m = e.payload.mode || "convert";
  mode = m;
  await load(e.payload.paths || []);
  setHover(e.payload.hover);
});

T.event.listen("sd-cancel", () => { if (!busy) hide(); });

T.event.listen("gesture-end", () => {
  setTimeout(() => { if (!dropped && !busy) hide(); }, 600);
});

window.addEventListener("keydown", (e) => { if (e.key === "Escape" && !busy) hide(); });

render();
console.log("main.js carregado");
