// Menu radial do ShiftDrop. Sem efeitos magnéticos nem cursor customizado:
// só realce do setor sob o cursor.
const T = window.__TAURI__;
const invoke = T.core.invoke;
const NS = "http://www.w3.org/2000/svg";
const C = 170, R1 = 54, R2 = 164;

const ring = document.getElementById("ring");
const center = document.getElementById("center");
const ctitle = document.getElementById("ctitle");
const csub = document.getElementById("csub");

let actions = [];
let hover = -1;
let busy = false;
let dropped = false;
let entered = false;
let hideTimer = null;
let defaultCenter = ["ShiftDrop", "Solte numa opção", ""];

const basename = (p) => String(p).split(/[\\/]/).pop();

function setCenter(title, sub, cls) {
  ctitle.textContent = title;
  csub.textContent = sub || "";
  center.className = cls || "";
}

function hide() {
  clearTimeout(hideTimer);
  actions = []; hover = -1; busy = false; entered = false;
  render();
  invoke("hide_radial");
}
function hideSoon(ms) { clearTimeout(hideTimer); hideTimer = setTimeout(hide, ms); }

function polar(r, a) { return [C + r * Math.sin(a), C - r * Math.cos(a)]; }

function arcPath(a0, a1) {
  const [x0, y0] = polar(R2, a0), [x1, y1] = polar(R2, a1);
  const [x2, y2] = polar(R1, a1), [x3, y3] = polar(R1, a0);
  const large = a1 - a0 > Math.PI ? 1 : 0;
  return `M${x0} ${y0} A${R2} ${R2} 0 ${large} 1 ${x1} ${y1} L${x2} ${y2} A${R1} ${R1} 0 ${large} 0 ${x3} ${y3} Z`;
}
function annulus() {
  return `M${C - R2} ${C} a${R2} ${R2} 0 1 0 ${2 * R2} 0 a${R2} ${R2} 0 1 0 ${-2 * R2} 0 Z ` +
         `M${C - R1} ${C} a${R1} ${R1} 0 1 0 ${2 * R1} 0 a${R1} ${R1} 0 1 0 ${-2 * R1} 0 Z`;
}

function el(tag, attrs, text) {
  const e = document.createElementNS(NS, tag);
  for (const k in attrs) e.setAttribute(k, attrs[k]);
  if (text != null) e.textContent = text;
  return e;
}

function render() {
  ring.innerHTML = "";
  const n = actions.length;
  if (!n) {
    ring.appendChild(el("path", { d: annulus(), class: "placeholder" }));
    return;
  }
  const step = (2 * Math.PI) / n;
  const gap = n > 1 ? 0.015 : 0;
  actions.forEach((a, i) => {
    const g = el("g", { class: "sector" + (i === hover ? " hover" : "") + (a.available ? "" : " disabled") });
    const d = n === 1 ? annulus() : arcPath(i * step - step / 2 + gap, i * step + step / 2 - gap);
    g.appendChild(el("path", { d, "fill-rule": "evenodd" }));
    const [tx, ty] = n === 1 ? [C, C - (R1 + R2) / 2] : polar((R1 + R2) / 2, i * step);
    g.appendChild(el("text", { x: tx, y: ty - 10, class: "ico" }, a.icon));
    g.appendChild(el("text", { x: tx, y: ty + 12 }, a.label));
    ring.appendChild(g);
  });
}

// Índice do setor na posição (px físicos relativos à janela).
// -2 = centro (cancelar), -1 = fora.
function hitTest(pos) {
  const dpr = window.devicePixelRatio || 1;
  const x = pos.x / dpr - C, y = pos.y / dpr - C;
  const d = Math.hypot(x, y);
  if (d < R1) return -2;
  if (d > R2 + 6 || !actions.length) return -1;
  const step = (2 * Math.PI) / actions.length;
  const a = (Math.atan2(x, -y) + 2 * Math.PI) % (2 * Math.PI);
  return Math.round(a / step) % actions.length;
}

function updateHover(pos) {
  if (busy || !pos) return;
  const idx = hitTest(pos);
  if (idx === hover) return;
  hover = idx;
  render();
  if (idx >= 0) {
    const a = actions[idx];
    setCenter(a.label, a.available ? "Solte para converter" : a.reason || "Indisponível", a.available ? "" : "err");
  } else {
    setCenter(...defaultCenter);
  }
}

T.webview.getCurrentWebview().onDragDropEvent(async (ev) => {
  const p = ev.payload;
  if (busy) return;
  if (p.type === "enter") {
    clearTimeout(hideTimer);
    entered = true;
    const paths = p.paths || [];
    actions = paths.length ? await invoke("list_actions", { paths }) : [];
    hover = -1;
    if (!actions.length) {
      defaultCenter = ["Sem ações", "Tipo de arquivo não suportado", "err"];
    } else {
      defaultCenter = [paths.length > 1 ? `${paths.length} arquivos` : basename(paths[0]), "Escolha uma opção", ""];
    }
    render();
    setCenter(...defaultCenter);
    updateHover(p.position);
  } else if (p.type === "over") {
    updateHover(p.position);
  } else if (p.type === "leave") {
    hover = -1;
    render();
    setCenter(...defaultCenter);
  } else if (p.type === "drop") {
    dropped = true;
    const idx = hitTest(p.position);
    const a = actions[idx];
    if (idx < 0 || !a) return hideSoon(120);
    if (!a.available) {
      setCenter("Indisponível", a.reason, "err");
      return hideSoon(1600);
    }
    busy = true;
    setCenter("Convertendo…", a.label, "");
    try {
      const outs = await invoke("run_action", { actionId: a.id, paths: p.paths });
      setCenter("Pronto ✓", outs.length === 1 ? basename(outs[0]) : `${outs.length} arquivos`, "ok");
    } catch (e) {
      setCenter("Erro", String(e).slice(0, 90), "err");
    }
    busy = false;
    hideSoon(1800);
  }
});

T.event.listen("radial-show", (e) => {
  clearTimeout(hideTimer);
  const manual = !!(e.payload && e.payload.manual);
  actions = []; hover = -1; busy = false; dropped = false; entered = false;
  defaultCenter = ["ShiftDrop", manual ? "Arraste um arquivo até aqui" : "Solte numa opção", ""];
  render();
  setCenter(...defaultCenter);
  if (manual) hideSoon(15000);
});

// Botão do mouse solto: se não houve drop em nós, esconde.
T.event.listen("gesture-end", () => {
  setTimeout(() => { if (!dropped && !busy) hide(); }, 600);
});

center.addEventListener("click", () => { if (!busy) hide(); });
window.addEventListener("keydown", (e) => { if (e.key === "Escape" && !busy) hide(); });

render();
