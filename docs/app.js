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

function renderChart(data) {
  const scored = data.entries.filter((e) => e.score != null);
  const labels = scored.map((e) => `#${e.id}`);
  const scores = scored.map((e) => e.score);

  const ctx = $("#scoreChart").getContext("2d");
  const grad = ctx.createLinearGradient(0, 0, 0, 320);
  grad.addColorStop(0, "rgba(92, 200, 255, 0.30)");
  grad.addColorStop(1, "rgba(92, 200, 255, 0.00)");

  new Chart(ctx, {
    type: "line",
    data: {
      labels,
      datasets: [
        {
          label: "SCORE (compressed bytes)",
          data: scores,
          borderColor: "#5cc8ff",
          backgroundColor: grad,
          fill: true,
          tension: 0.32,
          borderWidth: 2.5,
          pointRadius: scored.map((e) => (e.isRecord ? 6 : 4)),
          pointHoverRadius: 8,
          pointBackgroundColor: scored.map((e) => (e.isRecord ? "#3ddc97" : "#5cc8ff")),
          pointBorderColor: "#0a0e14",
          pointBorderWidth: 2,
        },
      ],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      interaction: { mode: "index", intersect: false },
      plugins: {
        legend: { display: false },
        tooltip: {
          backgroundColor: "#0c1119",
          borderColor: "#243044",
          borderWidth: 1,
          titleColor: "#e6edf6",
          bodyColor: "#cdd8e8",
          padding: 12,
          callbacks: {
            title: (items) => {
              const e = scored[items[0].dataIndex];
              return `#${e.id} · ${e.author}`;
            },
            label: (item) => {
              const e = scored[item.dataIndex];
              return [`SCORE: ${fmt(e.score)}`, `Δ: ${e.delta}`, `vs zstd: ${e.vsZstd}`];
            },
          },
        },
      },
      scales: {
        x: {
          grid: { color: "rgba(36, 48, 68, 0.6)" },
          ticks: { color: "#8b9bb2" },
        },
        y: {
          grid: { color: "rgba(36, 48, 68, 0.6)" },
          ticks: { color: "#8b9bb2", callback: (v) => fmt(v) },
          title: { display: true, text: "total compressed bytes", color: "#8b9bb2" },
        },
      },
    },
  });
}

function renderGrid(data) {
  const repo = data.repo || "10d9e/cm";
  const total = data.entries.length;
  $("#entryCount").textContent = `${total} ${total === 1 ? "entry" : "entries"}`;

  // newest first
  const rows = [...data.entries].reverse();
  const body = rows
    .map((e) => {
      const user = (e.author || "").replace(/^@/, "");
      const avatar = user
        ? `https://github.com/${encodeURIComponent(user)}.png?size=80`
        : "";
      const profile = user ? `https://github.com/${encodeURIComponent(user)}` : "#";
      const deltaClass = e.isRecord ? "good" : "flat";
      const commitUrl = `https://github.com/${repo}/commit/${e.commit}`;
      const entryUrl = e.entryPath ? `https://github.com/${repo}/blob/main/${e.entryPath}` : "";
      const entryLink = entryUrl
        ? `<a href="${entryUrl}" target="_blank" rel="noopener" title="Full entry">entry</a>`
        : "";
      return `
      <tr class="${e.isRecord ? "record" : ""}">
        <td class="c-id">#${e.id}</td>
        <td class="c-author">
          <img class="avatar" src="${avatar}" alt="" loading="lazy"
               onerror="this.style.visibility='hidden'" />
          <a href="${profile}" target="_blank" rel="noopener">${escapeHtml(e.author)}</a>
        </td>
        <td class="c-score">${fmt(e.score)}</td>
        <td class="c-delta"><span class="badge ${deltaClass}">${escapeHtml(e.delta)}</span></td>
        <td class="c-zstd">${escapeHtml(e.vsZstd)}</td>
        <td class="c-note"><div class="note">${renderNote(e.note)}</div></td>
        <td class="c-links">
          <a class="sha" href="${commitUrl}" target="_blank" rel="noopener" title="Commit">${escapeHtml(e.commit)}</a>
          ${entryLink}
        </td>
      </tr>`;
    })
    .join("");

  $("#grid").innerHTML = `
    <thead>
      <tr>
        <th class="c-id">#</th>
        <th class="c-author">Committer</th>
        <th class="c-score">SCORE</th>
        <th class="c-delta">Δ vs record</th>
        <th class="c-zstd">vs zstd</th>
        <th class="c-note">Comment</th>
        <th class="c-links">Links</th>
      </tr>
    </thead>
    <tbody>${body}</tbody>`;
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

    renderStats(data);
    renderChart(data);
    renderGrid(data);
  } catch (err) {
    document.querySelector("main").innerHTML =
      `<div class="error">Could not load leaderboard data.<br><small>${escapeHtml(String(err))}</small></div>`;
  }
}

main();
