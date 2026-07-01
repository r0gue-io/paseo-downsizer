// Generates the community-facing SVG charts from the plan numbers.
//   node docs/assets/generate-charts.mjs
// Outputs: validator-schedule.svg, budget-allocation.svg (in this dir).
// Deterministic, no deps. Colours match the Paseo/indigo palette and read on
// both light and dark backgrounds (each chart paints its own light card).

import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const OUT = dirname(fileURLToPath(import.meta.url));

const C = {
  card: "#ffffff",
  border: "#e2e8f0",
  grid: "#eef2f7",
  axis: "#475569",
  muted: "#94a3b8",
  old: "#4f46e5", // indigo — old relay
  oldFill: "#4f46e51a",
  neu: "#60a5fa", // blue — new chain
  neuFill: "#60a5fa26",
  danger: "#ef4444", // shutdown
  grace: "#fff7ed", // warm tint for the grace band
  graceEdge: "#fdba74",
  downsize: "#eef2ff", // indigo tint for the downsize band
  budget: "#0f172a",
  font: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif",
};

// ---- layout ---------------------------------------------------------------
const W = 880;
const H = 460;
const M = { top: 64, right: 40, bottom: 74, left: 64 };
const PW = W - M.left - M.right;
const PH = H - M.top - M.bottom;

function scaler(dMin, dMax, rMin, rMax) {
  return (v) => rMin + ((v - dMin) / (dMax - dMin)) * (rMax - rMin);
}

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;");

function text(x, y, s, opts = {}) {
  const {
    size = 13,
    color = C.axis,
    anchor = "start",
    weight = 400,
    style = "",
  } = opts;
  return `<text x="${x}" y="${y}" font-family="${C.font}" font-size="${size}" fill="${color}" text-anchor="${anchor}" font-weight="${weight}"${style ? ` font-style="${style}"` : ""}>${esc(s)}</text>`;
}

function frame(title, subtitle) {
  return `
  <rect x="0" y="0" width="${W}" height="${H}" rx="14" fill="${C.card}" stroke="${C.border}"/>
  ${text(M.left, 30, title, { size: 19, weight: 700, color: "#0f172a" })}
  ${text(M.left, 50, subtitle, { size: 13, color: C.muted })}`;
}

// Step path from [h,v] points: horizontal at previous value, then drop.
function stepPath(pts, X, Y) {
  let d = `M ${X(pts[0][0])} ${Y(pts[0][1])}`;
  for (let i = 1; i < pts.length; i++) {
    d += ` L ${X(pts[i][0])} ${Y(pts[i - 1][1])} L ${X(pts[i][0])} ${Y(pts[i][1])}`;
  }
  return d;
}
// Filled area under a step line down to the baseline.
function stepArea(pts, X, Y, y0) {
  return `${stepPath(pts, X, Y)} L ${X(pts[pts.length - 1][0])} ${y0} L ${X(pts[0][0])} ${y0} Z`;
}

// =====================================================================
// Chart 1 — validator schedule (precise, from the plan)
// =====================================================================
function validatorSchedule() {
  const tMax = 54;
  const vMax = 160;
  const X = scaler(0, tMax, M.left, M.left + PW);
  const Y = scaler(0, vMax, M.top + PH, M.top);
  const y0 = Y(0);

  // Plan: 152 -> 100 -> 60 -> 40 -> 20 at eras 1..4 (T+6..+24), hold to T+48.
  const steps = [
    [0, 152],
    [6, 100],
    [12, 60],
    [18, 40],
    [24, 20],
    [48, 20],
  ];
  const SHUTDOWN = 48;

  const parts = [];
  // phase bands
  parts.push(
    `<rect x="${X(0)}" y="${M.top}" width="${X(24) - X(0)}" height="${PH}" fill="${C.downsize}"/>`,
  );
  parts.push(
    `<rect x="${X(24)}" y="${M.top}" width="${X(SHUTDOWN) - X(24)}" height="${PH}" fill="${C.grace}"/>`,
  );

  // y gridlines + labels
  for (const v of [0, 20, 40, 60, 80, 100, 120, 140, 160]) {
    parts.push(
      `<line x1="${M.left}" y1="${Y(v)}" x2="${M.left + PW}" y2="${Y(v)}" stroke="${C.grid}"/>`,
    );
    parts.push(text(M.left - 10, Y(v) + 4, v, { anchor: "end", color: C.muted, size: 11 }));
  }
  // x ticks (every 6h)
  for (let h = 0; h <= tMax; h += 6) {
    parts.push(
      `<line x1="${X(h)}" y1="${y0}" x2="${X(h)}" y2="${y0 + 5}" stroke="${C.muted}"/>`,
    );
    parts.push(
      text(X(h), y0 + 20, `+${h}h`, { anchor: "middle", color: C.muted, size: 11 }),
    );
  }

  // area + step line
  parts.push(`<path d="${stepArea(steps, X, Y, y0)}" fill="${C.oldFill}"/>`);
  parts.push(
    `<path d="${stepPath(steps, X, Y)}" fill="none" stroke="${C.old}" stroke-width="2.5"/>`,
  );

  // value dots + labels at each step
  for (const [h, v] of steps.slice(0, 5)) {
    parts.push(`<circle cx="${X(h)}" cy="${Y(v)}" r="3.5" fill="${C.old}"/>`);
    parts.push(
      text(X(h), Y(v) - 10, v, { anchor: "middle", color: C.old, size: 12, weight: 700 }),
    );
  }

  // shutdown: red dashed drop to 0 + marker
  parts.push(
    `<line x1="${X(SHUTDOWN)}" y1="${Y(20)}" x2="${X(SHUTDOWN)}" y2="${y0}" stroke="${C.danger}" stroke-width="2.5" stroke-dasharray="5 4"/>`,
  );
  parts.push(`<circle cx="${X(SHUTDOWN)}" cy="${y0}" r="4" fill="${C.danger}"/>`);
  parts.push(
    text(X(SHUTDOWN) + 8, y0 - 8, "0 — network halts", { color: C.danger, size: 12, weight: 700 }),
  );

  // phase captions
  parts.push(
    text((X(0) + X(24)) / 2, M.top + 18, "DOWNSIZE  ~24h", {
      anchor: "middle", color: C.old, size: 12, weight: 700,
    }),
  );
  parts.push(
    text((X(24) + X(SHUTDOWN)) / 2, M.top + 18, "GRACE  ~24h", {
      anchor: "middle", color: "#c2680f", size: 12, weight: 700,
    }),
  );
  parts.push(
    text(X(SHUTDOWN), M.top + 18, "SHUTDOWN", {
      anchor: "middle", color: C.danger, size: 12, weight: 700,
    }),
  );

  // key milestone dates (CEST)
  const keyDates = [
    [0, "Thu 2 Jul"],
    [24, "Fri 3 Jul"],
    [48, "Sat 4 Jul"],
  ];
  for (const [h, label] of keyDates) {
    parts.push(text(X(h), y0 + 40, label, { anchor: "middle", color: C.axis, size: 11, weight: 700 }));
    parts.push(text(X(h), y0 + 53, "12:00 CEST", { anchor: "middle", color: C.muted, size: 10 }));
  }
  parts.push(
    `<text x="18" y="${M.top + PH / 2}" font-family="${C.font}" font-size="12" fill="${C.axis}" text-anchor="middle" transform="rotate(-90 18 ${M.top + PH / 2})">Active validators</text>`,
  );

  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="Paseo relay validator schedule">
  ${frame("Paseo relay — validator wind-down", "Starts Thu 2 Jul 2026, 12:00 CEST · 152 → 20 over ~24h, ~24h migration window, then coordinated shutdown")}
  ${parts.join("\n  ")}
</svg>`;
}

// =====================================================================
// Chart 2 — dual-chain vs the 80-node budget (illustrative new-chain growth)
// =====================================================================
function budgetAllocation() {
  const tMax = 54;
  const vMax = 170;
  const X = scaler(0, tMax, M.left, M.left + PW);
  const Y = scaler(0, vMax, M.top + PH, M.top);
  const y0 = Y(0);
  const BUDGET = 80;

  // STACKED: old relay (bottom) + new chain (on top). The top edge = total
  // committed nodes, compared against the 80-node budget line.
  // old relay validators (actual plan) — 0 at shutdown (T+48).
  const old = [
    [0, 152],
    [6, 100],
    [12, 60],
    [18, 40],
    [24, 20],
    [48, 20],
    [48, 0],
    [54, 0],
  ];
  // total = old + new (new chain growth ILLUSTRATIVE) — the top of the stack.
  const total = [
    [0, 156],
    [6, 112],
    [12, 80],
    [18, 70],
    [24, 65],
    [36, 72],
    [48, 60],
    [54, 60],
  ];
  const FILL_OLD = "#c7d2fe"; // indigo-200 (bottom band = old relay)
  const FILL_NEW = "#dbeafe"; // blue-100 (top band = new chain)

  const parts = [];

  // y grid + labels
  for (const v of [0, 40, 80, 120, 160]) {
    parts.push(
      `<line x1="${M.left}" y1="${Y(v)}" x2="${M.left + PW}" y2="${Y(v)}" stroke="${C.grid}"/>`,
    );
    parts.push(text(M.left - 10, Y(v) + 4, v, { anchor: "end", color: C.muted, size: 11 }));
  }
  // x ticks
  for (let h = 0; h <= tMax; h += 6) {
    parts.push(
      text(X(h), y0 + 20, `+${h}h`, { anchor: "middle", color: C.muted, size: 11 }),
    );
    parts.push(`<line x1="${X(h)}" y1="${y0}" x2="${X(h)}" y2="${y0 + 5}" stroke="${C.muted}"/>`);
  }

  // Draw the full stack (total) in the NEW colour first, then the OLD band on
  // top — so the blue visible above the indigo is exactly the new chain.
  parts.push(`<path d="${stepArea(total, X, Y, y0)}" fill="${FILL_NEW}"/>`);
  parts.push(`<path d="${stepArea(old, X, Y, y0)}" fill="${FILL_OLD}"/>`);
  parts.push(`<path d="${stepPath(old, X, Y)}" fill="none" stroke="${C.old}" stroke-width="2.5"/>`);
  parts.push(`<path d="${stepPath(total, X, Y)}" fill="none" stroke="#1e293b" stroke-width="2"/>`);

  // 80 budget line (dashed)
  parts.push(
    `<line x1="${M.left}" y1="${Y(BUDGET)}" x2="${M.left + PW}" y2="${Y(BUDGET)}" stroke="${C.danger}" stroke-width="2" stroke-dasharray="7 5"/>`,
  );
  parts.push(text(M.left + PW, Y(BUDGET) - 8, "80-node budget", { anchor: "end", color: C.danger, size: 12, weight: 700 }));

  // "within budget" marker where total crosses under 80 (~T+12h)
  parts.push(`<circle cx="${X(12)}" cy="${Y(80)}" r="4.5" fill="#1e293b"/>`);
  parts.push(text(X(12) + 8, Y(80) + 18, "within budget", { color: "#1e293b", size: 12, weight: 700 }));

  // shutdown marker
  parts.push(`<line x1="${X(48)}" y1="${M.top}" x2="${X(48)}" y2="${y0}" stroke="${C.danger}" stroke-width="1.5" stroke-dasharray="4 4" opacity="0.6"/>`);
  parts.push(text(X(48), M.top - 4, "shutdown", { anchor: "middle", color: C.danger, size: 11, weight: 700 }));

  // legend — top-right, over the empty upper area (data is low on the right)
  const lx = M.left + PW - 236;
  const ly = M.top + 16;
  const legend = [
    [FILL_OLD, C.old, "Old relay validators (actual)"],
    [FILL_NEW, C.neu, "New chain validators (illustrative)"],
    [null, "#1e293b", "Total committed nodes"],
  ];
  legend.forEach((it, i) => {
    const yy = ly + i * 21;
    if (it[0]) {
      parts.push(`<rect x="${lx}" y="${yy - 10}" width="16" height="11" rx="2" fill="${it[0]}" stroke="${it[1]}"/>`);
    } else {
      parts.push(`<line x1="${lx}" y1="${yy - 4}" x2="${lx + 16}" y2="${yy - 4}" stroke="${it[1]}" stroke-width="2"/>`);
    }
    parts.push(text(lx + 24, yy, it[2], { color: C.axis, size: 12 }));
  });

  parts.push(text(M.left, y0 + 40, "Time from start (CEST)", { color: C.axis, size: 12 }));
  parts.push(
    `<text x="18" y="${M.top + PH / 2}" font-family="${C.font}" font-size="12" fill="${C.axis}" text-anchor="middle" transform="rotate(-90 18 ${M.top + PH / 2})">Nodes (validators)</text>`,
  );

  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}" role="img" aria-label="Paseo two-chain budget allocation">
  ${frame("Fitting within budget", "From Thu 2 Jul 2026 12:00 CEST — downsizing the old relay fast pulls total nodes under the 80-node budget, freeing capacity for the new chain")}
  ${parts.join("\n  ")}
</svg>`;
}

writeFileSync(join(OUT, "validator-schedule.svg"), validatorSchedule());
writeFileSync(join(OUT, "budget-allocation.svg"), budgetAllocation());
console.log("wrote validator-schedule.svg and budget-allocation.svg");
