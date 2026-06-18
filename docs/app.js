"use strict";

const $ = (sel) => document.querySelector(sel);

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => (
    { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]
  ));
}

// Minimal, safe markdown: escape first, then inline code + bold. Unwrap hard
// line wraps but keep list items (lines starting with "N." or "-") on new lines.
function renderNote(text) {
  if (!text) return "";
  let html = escapeHtml(text);
  html = html.replace(/`([^`]+)`/g, "<code>$1</code>");
  html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  html = html
    .split("\n")
    .reduce((acc, line) => {
      const isItem = /^\s*(\d+\.|[-*])\s/.test(line);
      if (acc.length && !isItem) {
        acc[acc.length - 1] += " " + line.trim();
      } else {
        acc.push(line.trim());
      }
      return acc;
    }, [])
    .join("<br>");
  return html;
}

function fmt(n) {
  return n == null ? "—" : n.toLocaleString("en-US");
}

// WORK = deterministic wasm-fuel complexity (executed operators). Large integers,
// so render compactly (G/M). Older entries predate the metric and show "—".
function fmtWork(n) {
  if (n == null) return "—";
  if (n >= 1e9) return (n / 1e9).toFixed(2) + "G";
  if (n >= 1e6) return (n / 1e6).toFixed(1) + "M";
  return n.toLocaleString("en-US");
}

function scoreLeaderCrown(title = "Current SCORE leader") {
  return `<span class="score-crown" title="${title}" aria-label="${title}"><svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor" aria-hidden="true"><path d="M2 19h20v2H2v-2zm2.4-8.2 2.1 2.1 3.5-6.3 3.5 6.3 2.1-2.1L20.6 19H3.4l1-8.2zM5.2 17h13.6l-.9-7.4-1.6 1.6L12 5.5 9.7 11.2 8.1 9.6 5.2 17z"/></svg></span>`;
}

const BASELINE_COLORS = {
  zstd22: "rgba(255, 255, 255, 0.22)",
  xz9e: "rgba(255, 255, 255, 0.16)",
  brotli11: "rgba(147, 197, 253, 0.45)",
  lpaq1_9: "rgba(192, 132, 252, 0.5)",
  zpaq5: "rgba(251, 191, 36, 0.55)",
};

function baselineDatasets(labels, baselines) {
  if (!baselines?.length) return [];
  const n = labels.length;
  return baselines.map((b) => ({
    label: b.label,
    data: Array(n).fill(b.total),
    borderColor: BASELINE_COLORS[b.id] || "rgba(255, 255, 255, 0.2)",
    borderDash: [5, 4],
    borderWidth: 1,
    pointRadius: 0,
    pointHoverRadius: 0,
    fill: false,
    tension: 0,
    order: 2,
  }));
}

function statCard(label, value, opts = {}) {
  const cls = opts.good ? "value good" : "value";
  const sub = opts.sub ? `<div class="sub">${opts.sub}</div>` : "";
  return `<div class="stat"><div class="label">${label}</div><div class="${cls}">${value}</div>${sub}</div>`;
}

function renderStats(data) {
  const scored = data.entries.filter((e) => e.score != null);
  const record = data.record ? data.record.score : null;
  const baseline = data.baseline;
  const improvement = baseline != null && record != null ? baseline - record : null;
  const pct = improvement != null ? ((improvement / baseline) * 100).toFixed(2) : null;
  const latest = scored[scored.length - 1] || {};

  $("#stats").innerHTML = [
    statCard("Current record", fmt(record), {
      good: true,
      sub: data.record ? `${data.record.author} · #${data.record.id}` : "",
    }),
    statCard("Baseline", fmt(baseline), { sub: "entry #0001" }),
    statCard("Total improvement", improvement != null ? `−${fmt(improvement)}` : "—", {
      good: improvement != null,
      sub: pct != null ? `${pct}% smaller` : "",
    }),
    statCard("vs zstd −22", latest.vsZstd || "—", { sub: "smaller is a win" }),
  ].join("");
}

function runningRecordFrontier(entries) {
  let bestScore = Infinity;
  let bestWork = Infinity;
  return entries.map((e) => {
    if (e.score == null) return null;
    const work = e.work ?? Number.POSITIVE_INFINITY;
    if (e.score < bestScore || (e.score === bestScore && work < bestWork)) {
      bestScore = e.score;
      bestWork = work;
    }
    return bestScore === Infinity ? null : bestScore;
  });
}

function submissionPointStyle(e) {
  if (e.isRecord) {
    return { radius: 5, bg: "#4ade80", border: "#000" };
  }
  if (e.isNonWinning) {
    return { radius: 4, bg: "#fbbf24", border: "#000" };
  }
  return { radius: 3, bg: "rgba(255, 255, 255, 0.55)", border: "#000" };
}

function renderChart(data) {
  const scored = data.entries.filter((e) => e.score != null);
  const labels = scored.map((e) => `#${e.id}`);
  const scores = scored.map((e) => e.score);
  const frontier = runningRecordFrontier(scored);
  const baselines = data.baselines || [];
  const styles = scored.map(submissionPointStyle);

  const ctx = $("#scoreChart").getContext("2d");
  const grad = ctx.createLinearGradient(0, 0, 0, 320);
  grad.addColorStop(0, "rgba(74, 222, 128, 0.10)");
  grad.addColorStop(1, "rgba(74, 222, 128, 0.00)");

  new Chart(ctx, {
    type: "line",
    data: {
      labels,
      datasets: [
        {
          label: "Best SCORE so far",
          data: frontier,
          borderColor: "rgba(74, 222, 128, 0.85)",
          backgroundColor: grad,
          fill: true,
          stepped: "before",
          tension: 0,
          borderWidth: 1.5,
          pointRadius: 0,
          pointHoverRadius: 0,
          order: 1,
        },
        {
          label: "Submissions",
          data: scores,
          borderColor: "transparent",
          backgroundColor: "transparent",
          showLine: false,
          pointRadius: styles.map((s) => s.radius),
          pointHoverRadius: 7,
          pointBackgroundColor: styles.map((s) => s.bg),
          pointBorderColor: styles.map((s) => s.border),
          pointBorderWidth: 2,
          order: 0,
        },
        ...baselineDatasets(labels, baselines),
      ],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      interaction: { mode: "index", intersect: false },
      plugins: {
        legend: {
          display: true,
          position: "bottom",
          labels: {
            color: "rgba(255, 255, 255, 0.45)",
            font: { family: "'DM Mono', monospace", size: 9 },
            boxWidth: 14,
            boxHeight: 1,
            padding: 14,
            usePointStyle: false,
            filter: (item) => item.text !== "Submissions",
          },
        },
        tooltip: {
          backgroundColor: "rgba(0, 0, 0, 0.88)",
          borderColor: "rgba(255, 255, 255, 0.12)",
          borderWidth: 1,
          titleColor: "#fff",
          bodyColor: "rgba(255, 255, 255, 0.68)",
          titleFont: { family: "'JetBrains Mono', monospace", size: 11 },
          bodyFont: { family: "'DM Mono', monospace", size: 10 },
          padding: 12,
          filter: (item) => item.datasetIndex === 1,
          callbacks: {
            title: (items) => {
              const e = scored[items[0].dataIndex];
              return `#${e.id} · ${e.author}`;
            },
            label: (item) => {
              const e = scored[item.dataIndex];
              const lines = [
                `SCORE: ${fmt(e.score)}`,
                `Δ: ${e.delta}`,
                `vs zstd: ${e.vsZstd}`,
              ];
              if (e.scoreRank != null) {
                lines.push(`SCORE rank: #${e.scoreRank} of ${scored.length}`);
              }
              if (e.isNonWinning) {
                lines.push("Non-winning (WORK/speed focus — not on record frontier)");
              }
              return lines;
            },
          },
        },
      },
      scales: {
        x: {
          grid: { color: "rgba(255, 255, 255, 0.05)" },
          ticks: {
            color: "rgba(255, 255, 255, 0.28)",
            font: { family: "'DM Mono', monospace", size: 9 },
          },
          border: { color: "rgba(255, 255, 255, 0.07)" },
        },
        y: {
          grid: { color: "rgba(255, 255, 255, 0.05)" },
          ticks: {
            color: "rgba(255, 255, 255, 0.28)",
            font: { family: "'DM Mono', monospace", size: 9 },
            callback: (v) => fmt(v),
          },
          border: { color: "rgba(255, 255, 255, 0.07)" },
          title: {
            display: true,
            text: "total compressed bytes",
            color: "rgba(255, 255, 255, 0.22)",
            font: { family: "'DM Mono', monospace", size: 9 },
          },
        },
      },
    },
  });
}

function compactDelta(e) {
  if (!e.delta || e.delta.includes("baseline")) return "—";
  if (e.deltaValue != null) {
    return e.isRecord ? `${e.deltaValue} ★` : String(e.deltaValue);
  }
  return e.delta.replace(/\s*\([^)]*\)/, "").trim();
}

let ENTRIES_BY_ID = {};
let LEADER_ID = null;

function renderGrid(data) {
  const total = data.entries.length;
  const leaderId = data.record?.id ?? null;
  LEADER_ID = leaderId;
  $("#entryCount").textContent = `${total} ${total === 1 ? "entry" : "entries"}`;
  ENTRIES_BY_ID = Object.fromEntries(data.entries.map((e) => [e.id, e]));

  // newest first
  const rows = [...data.entries].reverse();
  const body = rows
    .map((e) => {
      const user = (e.author || "").replace(/^@/, "");
      const avatar = user
        ? `https://github.com/${encodeURIComponent(user)}.png?size=80`
        : "";
      const deltaClass = e.isRecord ? "good" : e.isNonWinning ? "warn" : "flat";
      const rowClass = [
        e.isRecord ? "record" : "",
        e.isNonWinning ? "non-winning" : "",
        e.id === leaderId ? "score-leader" : "",
      ].filter(Boolean).join(" ");
      const scoreCell = e.id === leaderId
        ? `<span class="c-score-leader">${scoreLeaderCrown()}${fmt(e.score)}</span>`
        : fmt(e.score);
      return `
      <tr class="${rowClass}" data-id="${e.id}" tabindex="0" role="button"
          aria-label="View details for entry ${e.id}">
        <td class="c-id">#${e.id}</td>
        <td class="c-author">
          <img class="avatar" src="${avatar}" alt="" loading="lazy"
               onerror="this.style.visibility='hidden'" />
          <span class="aname">${escapeHtml(e.author)}</span>
        </td>
        <td class="c-model">${escapeHtml(e.model || "—")}</td>
        <td class="c-score">${scoreCell}</td>
        <td class="c-delta"><span class="badge ${deltaClass}">${escapeHtml(compactDelta(e))}</span></td>
        <td class="c-zstd">${escapeHtml(e.vsZstd)}</td>
        <td class="c-work" title="${e.work != null ? e.work + " wasm operators (deterministic, lower is faster)" : "not measured"}">${fmtWork(e.work)}</td>
        <td class="c-memcost" title="${e.memcost != null ? e.memcost + " — deterministic cache-miss penalty (memory traffic); lower is friendlier to memory" : "not measured"}">${fmtWork(e.memcost)}</td>
        <td class="c-lines diag" title="${e.lines != null ? e.lines + " distinct 64B cache lines touched (deterministic, non-scoring diagnostic); lower is friendlier to memory" : "not measured"}">${fmtWork(e.lines)}</td>
        <td class="c-heappeak diag" title="${e.heapPeak != null ? e.heapPeak + " bytes peak live reserved heap over the full corpus (deterministic, non-scoring diagnostic); lower is leaner" : "not measured"}">${fmtWork(e.heapPeak)}</td>
        <td class="c-heapchurn diag" title="${e.heapChurn != null ? e.heapChurn + " bytes init-free heap requested in steady state (deterministic, non-scoring diagnostic); lower is leaner" : "not measured"}">${fmtWork(e.heapChurn)}</td>
        <td class="c-open"><span class="open-btn">View ↗</span></td>
      </tr>`;
    })
    .join("");

  $("#grid").innerHTML = `
    <colgroup>
      <col class="w-id" /><col class="w-author" /><col class="w-model" /><col class="w-score" />
      <col class="w-delta" /><col class="w-zstd" /><col class="w-work" /><col class="w-memcost" />
      <col class="w-lines" /><col class="w-heappeak" /><col class="w-heapchurn" /><col class="w-open" />
    </colgroup>
    <thead>
      <tr>
        <th class="c-id">#</th>
        <th class="c-author">Committer</th>
        <th class="c-model">Model</th>
        <th class="c-score">SCORE</th>
        <th class="c-delta">Δ</th>
        <th class="c-zstd">vs zstd</th>
        <th class="c-work" title="Deterministic complexity — wasm fuel (executed operators); lower is faster. Breaks exact SCORE ties: equal bytes, lower WORK wins.">WORK</th>
        <th class="c-memcost" title="Deterministic memory-traffic cost — weighted cache-miss penalty from a fixed cache model over the wasm access trace; lower is friendlier to memory (tracks cache latency, which WORK cannot).">MEMCOST</th>
        <th class="c-lines diag" title="Diagnostic (non-scoring) — distinct 64B cache lines touched on the same init-free differencing as MEMCOST; associativity-free. Lower is friendlier to memory.">LINES</th>
        <th class="c-heappeak diag" title="Diagnostic (non-scoring) — peak live reserved heap over the full corpus (deterministic; sums requested sizes). Lower is leaner.">HEAP_PEAK</th>
        <th class="c-heapchurn diag" title="Diagnostic (non-scoring) — init-free heap bytes requested in steady state via the heap-tracking shim. Lower is leaner.">HEAP_CHURN</th>
        <th class="c-open"></th>
      </tr>
    </thead>
    <tbody>${body}</tbody>`;

  const open = (el) => {
    const id = el.getAttribute("data-id");
    if (id) openDialog(ENTRIES_BY_ID[id], data.repo || "10d9e/cm");
  };
  $("#grid").querySelectorAll("tbody tr").forEach((tr) => {
    tr.addEventListener("click", () => open(tr));
    tr.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" || ev.key === " ") {
        ev.preventDefault();
        open(tr);
      }
    });
  });
}

function dialogSection(title, html) {
  if (!html) return "";
  return `<section class="d-sec"><h3>${title}</h3>${html}</section>`;
}

function setupDialog(dlg, { onClose } = {}) {
  dlg.addEventListener("click", (ev) => {
    if (ev.target === dlg) dlg.close();
  });
  dlg.addEventListener("close", () => {
    if (onClose) onClose();
  });
}

function bindDialogClose(root, dlg) {
  const closeBtn = root.querySelector("[data-close]");
  if (closeBtn) closeBtn.addEventListener("click", () => dlg.close());
}

function showDialog(dlg) {
  if (typeof dlg.showModal === "function") dlg.showModal();
  else dlg.setAttribute("open", "");
  dlg.scrollTop = 0;
}

function openInstructions(repo) {
  const base = `https://github.com/${repo}`;
  $("#instructionsSkill").href = `${base}/blob/main/.agents/skills/autocm/SKILL.md`;
  $("#instructionsReadme").href = `${base}/blob/main/AUTORESEARCH.md`;
  $("#instructionsContrib").href = `${base}/blob/main/CONTRIBUTING.md`;
  showDialog($("#instructionsDialog"));
  try { localStorage.setItem("cm-instructions-seen", "1"); } catch (_) {}
}

function openDialog(e, repo) {
  if (!e) return;
  const user = (e.author || "").replace(/^@/, "");
  const avatar = user ? `https://github.com/${encodeURIComponent(user)}.png?size=120` : "";
  const profile = user ? `https://github.com/${encodeURIComponent(user)}` : "#";
  const commitUrl = `https://github.com/${repo}/commit/${e.commit}`;
  const entryUrl = e.entryPath ? `https://github.com/${repo}/blob/main/${e.entryPath}` : "";
  const deltaClass = e.isRecord ? "good" : e.isNonWinning ? "warn" : "flat";

  $("#dialogInner").innerHTML = `
    <button class="dialog-close" aria-label="Close" data-close>×</button>
    <header class="dialog-head">
      <img class="d-avatar" src="${avatar}" alt="" onerror="this.style.visibility='hidden'" />
      <div class="d-head-text">
        <div class="d-title">Entry #${e.id}
          ${e.isRecord ? '<span class="badge good">record</span>' : ""}
          ${e.isNonWinning ? '<span class="badge warn">non-winning</span>' : ""}
        </div>
        <div class="d-sub">
          <a href="${profile}" target="_blank" rel="noopener">${escapeHtml(e.author)}</a>
          · ${escapeHtml(e.date)}${e.model ? ` · ${escapeHtml(e.model)}` : ""}
        </div>
      </div>
    </header>

    <div class="d-metrics">
      ${e.model ? `<div class="d-metric"><span class="m-label">Model</span><span class="m-value">${escapeHtml(e.model)}</span></div>` : ""}
      <div class="d-metric"><span class="m-label">SCORE</span><span class="m-value">${e.id === LEADER_ID ? `${scoreLeaderCrown()} ` : ""}${fmt(e.score)}${e.scoreRank != null ? ` <span class="m-sub">(#${e.scoreRank} of ${Object.keys(ENTRIES_BY_ID).length})</span>` : ""}</span></div>
      <div class="d-metric"><span class="m-label">Δ vs record</span><span class="m-value"><span class="badge ${deltaClass}">${escapeHtml(e.delta)}</span></span></div>
      <div class="d-metric"><span class="m-label">vs zstd −22</span><span class="m-value">${escapeHtml(e.vsZstd)}</span></div>
      ${e.work != null ? `<div class="d-metric"><span class="m-label">WORK</span><span class="m-value" title="deterministic wasm fuel — executed operators; lower is faster">${fmt(e.work)}</span></div>` : ""}
      ${e.memcost != null ? `<div class="d-metric"><span class="m-label">MEMCOST</span><span class="m-value" title="deterministic cache-miss penalty (memory traffic); lower is friendlier to memory">${fmt(e.memcost)}</span></div>` : ""}
      ${e.lines != null ? `<div class="d-metric"><span class="m-label">LINES</span><span class="m-value" title="distinct 64B cache lines touched (non-scoring diagnostic); lower is friendlier to memory">${fmt(e.lines)}</span></div>` : ""}
      ${e.heapPeak != null ? `<div class="d-metric"><span class="m-label">HEAP_PEAK</span><span class="m-value" title="peak live reserved heap over the full corpus (non-scoring diagnostic); lower is leaner">${fmt(e.heapPeak)}</span></div>` : ""}
      ${e.heapChurn != null ? `<div class="d-metric"><span class="m-label">HEAP_CHURN</span><span class="m-value" title="init-free heap bytes requested in steady state (non-scoring diagnostic); lower is leaner">${fmt(e.heapChurn)}</span></div>` : ""}
      <div class="d-metric"><span class="m-label">commit</span><span class="m-value"><a class="sha" href="${commitUrl}" target="_blank" rel="noopener">${escapeHtml(e.commit)}</a></span></div>
    </div>

    ${dialogSection("Approach", `<div class="note">${renderNote(e.approach)}</div>`)}
    ${dialogSection("Iteration notes", `<div class="note">${renderNote(e.iterationNotes)}</div>`)}
    ${dialogSection("Eval snapshot", e.evalSnapshot ? `<pre class="snapshot">${escapeHtml(e.evalSnapshot)}</pre>` : "")}

    <footer class="dialog-foot">
      ${entryUrl ? `<a href="${entryUrl}" target="_blank" rel="noopener">Open full entry on GitHub →</a>` : ""}
    </footer>`;

  const dlg = $("#entryDialog");
  bindDialogClose($("#dialogInner"), dlg);
  showDialog(dlg);
  if (history.replaceState) history.replaceState(null, "", `#${e.id}`);
}

async function main() {
  try {
    const res = await fetch("./data/leaderboard.json", { cache: "no-cache" });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();

    const repo = data.repo || "10d9e/cm";
    $("#repoLink").href = `https://github.com/${repo}`;
    if (data.generatedAt) {
      $("#generatedAt").textContent = `Updated ${new Date(data.generatedAt).toLocaleString()}`;
    }

    setupDialog($("#entryDialog"), {
      onClose: () => {
        if (history.replaceState) history.replaceState(null, "", location.pathname + location.search);
      },
    });
    setupDialog($("#instructionsDialog"));
    bindDialogClose($("#instructionsDialog"), $("#instructionsDialog"));
    $("#instructionsBtn").addEventListener("click", () => openInstructions(repo));

    renderStats(data);
    renderChart(data);
    renderGrid(data);

    // Deep link: #<entryId> opens that solution directly.
    const hashId = location.hash.replace(/^#/, "");
    if (hashId && ENTRIES_BY_ID[hashId]) {
      openDialog(ENTRIES_BY_ID[hashId], repo);
    } else {
      let seen = false;
      try { seen = localStorage.getItem("cm-instructions-seen") === "1"; } catch (_) {}
      if (!seen) openInstructions(repo);
    }
  } catch (err) {
    document.querySelector("main").innerHTML =
      `<div class="error">Could not load leaderboard data.<br><small>${escapeHtml(String(err))}</small></div>`;
  }
}

main();
