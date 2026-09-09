// Multi-profile settings UI for Tianshu Bridge desktop app.
const tauri = window.__TAURI__;
const invoke = tauri?.core?.invoke;
const listen = tauri?.event?.listen;
const readClipboard = tauri?.clipboardManager?.readText;
const $ = (id) => document.getElementById(id);

let config = { profiles: [] };
let statuses = {}; // id → running bool

function flash(text, level = "info") {
  const t = $("toast");
  if (!t) return;
  t.textContent = text;
  t.className = `toast ${level} visible`;
  clearTimeout(flash._t);
  flash._t = setTimeout(() => t.classList.remove("visible"), 2500);
}

function genId() {
  return "p_" + Date.now().toString(36) + "_" + Math.random().toString(36).slice(2, 8);
}

function renderProfiles() {
  const list = $("profileList");
  list.innerHTML = "";
  if (config.profiles.length === 0) {
    list.innerHTML = '<div style="color:#64748b;text-align:center;padding:20px;">No profiles. Add a server above.</div>';
    return;
  }
  for (const p of config.profiles) {
    const running = statuses[p.id] || false;
    const card = document.createElement("div");
    card.className = `profile-card${running ? " running" : ""}`;
    card.innerHTML = `
      <div class="profile-header">
        <div class="status-dot${running ? " running" : ""}"></div>
        <div class="profile-name">${esc(p.name)}</div>
        <span style="font-size:10px;color:#64748b">${running ? "Connected" : "Stopped"}</span>
      </div>
      <div class="profile-server">${esc(p.server)}</div>
      <div class="profile-actions">
        ${running
          ? `<button class="btn red" data-action="stop" data-id="${p.id}">Stop</button>`
          : `<button class="btn green" data-action="start" data-id="${p.id}">Start</button>`
        }
        <button class="btn" data-action="edit" data-id="${p.id}">Edit</button>
        <button class="btn danger" data-action="delete" data-id="${p.id}">Delete</button>
      </div>
      <div class="edit-form" id="edit-${p.id}">
        <div class="form-row"><label>Name</label><input type="text" data-field="name" value="${esc(p.name)}"></div>
        <div class="form-row"><label>Server</label><input type="text" data-field="server" value="${esc(p.server)}"></div>
        <div class="form-row"><label>Token</label><input type="text" data-field="token" value="${esc(p.token || "")}"></div>
        <div class="form-row"><label>Device</label><input type="text" data-field="device" value="${esc(p.device || "")}"></div>
        <div class="form-row"><label>Engine</label>
          <select data-field="engine">
            <option value="own"${p.engine !== "stealth" ? " selected" : ""}>own (system Chrome)</option>
            <option value="stealth"${p.engine === "stealth" ? " selected" : ""}>stealth (CloakBrowser)</option>
          </select>
        </div>
        <div class="form-row"><label></label>
          <div style="display:flex;gap:12px;flex-wrap:wrap">
            <label class="checkbox-row"><input type="checkbox" data-field="browser" ${p.browser !== false ? "checked" : ""}> Browser</label>
            <label class="checkbox-row"><input type="checkbox" data-field="headless" ${p.headless ? "checked" : ""}> Headless</label>
            <label class="checkbox-row"><input type="checkbox" data-field="shell" ${p.shell ? "checked" : ""}> Shell</label>
            <label class="checkbox-row"><input type="checkbox" data-field="autoStart" ${p.auto_start !== false ? "checked" : ""}> Auto-start</label>
          </div>
        </div>
        <div style="display:flex;gap:6px;justify-content:flex-end;margin-top:8px">
          <button class="btn primary" data-action="save-edit" data-id="${p.id}">Save</button>
          <button class="btn" data-action="cancel-edit" data-id="${p.id}">Cancel</button>
        </div>
      </div>
    `;
    list.appendChild(card);
  }
}

function esc(s) { return String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/"/g, "&quot;"); }

async function refreshStatus() {
  if (!invoke) return;
  try {
    const s = await invoke("get_status");
    statuses = {};
    for (const entry of s) statuses[entry.id] = entry.running;
    renderProfiles();
  } catch {}
}

async function loadAndRender() {
  if (!invoke) return;
  try {
    config = await invoke("load_config");
    if (!config.profiles) config.profiles = [];
  } catch {}
  await refreshStatus();
}

// Event delegation
document.addEventListener("click", async (e) => {
  const btn = e.target.closest("[data-action]");
  if (!btn) return;
  const action = btn.dataset.action;
  const id = btn.dataset.id;

  if (action === "start") {
    try { await invoke("start_profile", { id }); flash("Started"); } catch (e) { flash(String(e), "error"); }
    await refreshStatus();
  }
  if (action === "stop") {
    try { await invoke("stop_profile", { id }); flash("Stopped"); } catch (e) { flash(String(e), "error"); }
    await refreshStatus();
  }
  if (action === "edit") {
    const form = $(`edit-${id}`);
    if (form) form.classList.toggle("visible");
  }
  if (action === "cancel-edit") {
    const form = $(`edit-${id}`);
    if (form) form.classList.remove("visible");
  }
  if (action === "save-edit") {
    const form = $(`edit-${id}`);
    if (!form) return;
    const p = config.profiles.find((p) => p.id === id);
    if (!p) return;
    p.name = form.querySelector('[data-field="name"]').value;
    p.server = form.querySelector('[data-field="server"]').value;
    p.token = form.querySelector('[data-field="token"]').value;
    p.device = form.querySelector('[data-field="device"]').value;
    p.engine = form.querySelector('[data-field="engine"]').value;
    p.browser = form.querySelector('[data-field="browser"]').checked;
    p.headless = form.querySelector('[data-field="headless"]').checked;
    p.shell = form.querySelector('[data-field="shell"]').checked;
    p.auto_start = form.querySelector('[data-field="autoStart"]').checked;
    try { await invoke("save_config", { cfg: config }); flash("Saved ✓"); } catch (e) { flash(String(e), "error"); }
    renderProfiles();
  }
  if (action === "delete") {
    config.profiles = config.profiles.filter((p) => p.id !== id);
    try {
      await invoke("stop_profile", { id }).catch(() => {});
      await invoke("save_config", { cfg: config });
      flash("Deleted");
    } catch (e) { flash(String(e), "error"); }
    await refreshStatus();
  }
});

$("addBtn")?.addEventListener("click", async () => {
  const server = $("newServer")?.value.trim();
  if (!server) { flash("Enter a server URL", "error"); return; }
  let name;
  try { name = new URL(server).hostname; } catch { name = server; }
  config.profiles.push({
    id: genId(),
    name,
    server,
    token: "",
    device: "",
    auto_start: true,
    browser: true,
    engine: "own",
    headless: false,
    shell: false,
  });
  try { await invoke("save_config", { cfg: config }); flash("Added " + name); } catch (e) { flash(String(e), "error"); }
  $("newServer").value = "";
  renderProfiles();
});

$("pasteBtn")?.addEventListener("click", async () => {
  let raw = "";
  try { raw = readClipboard ? await readClipboard() : await navigator.clipboard.readText(); } catch { flash("Clipboard unavailable", "error"); return; }
  // Parse tsbridge://configure?server=...&token=... or JSON
  let parsed = null;
  if (/^tsbridge:\/\//i.test(raw)) {
    try { const u = new URL(raw); parsed = { server: u.searchParams.get("server"), token: u.searchParams.get("token") }; } catch {}
  } else if (raw.trim().startsWith("{")) {
    try { parsed = JSON.parse(raw); } catch {}
  } else if (/^wss?:\/\//i.test(raw.trim())) {
    const parts = raw.trim().split(/\s+/);
    parsed = { server: parts[0], token: parts[1] || "" };
  }
  if (!parsed?.server) { flash("Couldn't parse — expected tsbridge URL, JSON, or wss://", "error"); return; }
  let name; try { name = new URL(parsed.server).hostname; } catch { name = parsed.server; }
  config.profiles.push({
    id: genId(), name, server: parsed.server, token: parsed.token || "",
    device: parsed.device || "", auto_start: true,
    browser: parsed.browser ?? true, engine: parsed.engine || "own",
    headless: parsed.headless || false, shell: parsed.shell || false,
  });
  try { await invoke("save_config", { cfg: config }); flash("Added " + name + " from clipboard"); } catch (e) { flash(String(e), "error"); }
  renderProfiles();
});

$("hideBtn")?.addEventListener("click", () => invoke?.("hide_window"));

// Listen for status changes from the Rust backend
listen?.("bridge-status-changed", () => refreshStatus());

// Poll status periodically
setInterval(refreshStatus, 3000);

loadAndRender();
