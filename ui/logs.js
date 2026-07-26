// Logs viewer — standalone window invoked from the tray "View Logs" menu.
const tauri = window.__TAURI__;
const invoke = tauri?.core?.invoke;

const $ = (id) => document.getElementById(id);
let entries = [];

async function loadLogs() {
  try {
    entries = await invoke("read_logs", { maxLines: 1000 });
  } catch {
    entries = [];
  }
  render();
}

function render() {
  const container = $("logContainer");
  const query = ($("search").value || "").toLowerCase();
  const filtered = query
    ? entries.filter(
        (e) => e.text.toLowerCase().includes(query) || e.source.includes(query),
      )
    : entries;

  $("count").textContent = query
    ? `${filtered.length} / ${entries.length}`
    : `${entries.length} entries`;

  if (!filtered.length) {
    container.innerHTML = `<div class="empty">${entries.length ? "No matches" : "No logs yet"}</div>`;
    return;
  }

  container.innerHTML = filtered
    .map((e) => {
      const time = e.ts ? formatTs(e.ts) : "—";
      const text = query ? highlight(esc(e.text), query) : esc(e.text);
      return `<div class="log-entry">
        <span class="ts">${time}</span>
        <span class="src ${e.source}">${e.source}</span>
        <span class="msg">${text}</span>
      </div>`;
    })
    .join("");
}

function formatTs(epoch) {
  const d = new Date(epoch * 1000);
  const mo = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${mo}-${dd} ${hh}:${mm}:${ss}`;
}

function esc(s) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function highlight(html, q) {
  // Case-insensitive highlight on already-escaped HTML.
  const re = new RegExp(`(${q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi");
  return html.replace(re, '<span class="highlight">$1</span>');
}

$("search").addEventListener("input", render);
$("refresh").addEventListener("click", loadLogs);
$("clear").addEventListener("click", async () => {
  await invoke("clear_logs");
  entries = [];
  render();
});

// Auto-refresh every 3 seconds while window is visible.
setInterval(() => {
  if (!document.hidden) loadLogs();
}, 3000);

loadLogs();
