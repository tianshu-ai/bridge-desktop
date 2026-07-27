// Frontend for the Tauri settings window. This window is config-only:
// start/stop happen from the tray menu. Talks to the Rust backend via
// Tauri's invoke() commands: load_config / save_config.
const tauri = window.__TAURI__;
const invoke = tauri?.core?.invoke;

const $ = (id) => document.getElementById(id);

if (!invoke) {
  // Should not happen once withGlobalTauri is on, but fail loud instead
  // of every button silently doing nothing.
  document.addEventListener("DOMContentLoaded", () => {
    const b = document.createElement("div");
    b.style.cssText = "padding:10px;color:#b91c1c;font-size:12px";
    b.textContent = "Tauri API unavailable (window.__TAURI__ missing).";
    document.body.prepend(b);
  });
}

function readForm() {
  return {
    server: $("server").value.trim(),
    token: $("token").value,
    device: $("device").value.trim(),
    browser: $("browser").checked,
    engine: $("engine").value,
    headless: $("headless").checked,
    shell: $("shell").checked,
  };
}

function writeForm(cfg) {
  $("server").value = cfg.server ?? "";
  $("token").value = cfg.token ?? "";
  $("device").value = cfg.device ?? "";
  $("browser").checked = !!cfg.browser;
  $("engine").value = cfg.engine === "stealth" ? "stealth" : "own";
  $("headless").checked = !!cfg.headless;
  $("shell").checked = !!cfg.shell;
}

function flash(text) {
  const m = $("msg");
  if (!m) return;
  m.textContent = text;
  clearTimeout(flash._t);
  flash._t = setTimeout(() => (m.textContent = ""), 2500);
}

// Parse a pasted config into a partial form object. Accepts:
//   - a `tsbridge://configure?server=…&token=…&…` deep link
//     (the format the Tianshu panel copies)
//   - a `tsbridge --server X --token Y --shell --browser-engine stealth` line
//   - a JSON object { server, token, ... }
//   - a bare "wss://host/ws <token>" pair
function parseConfig(raw) {
  const text = (raw || "").trim();
  if (!text) return null;

  // tsbridge://configure?… deep link (Tianshu panel format).
  if (/^tsbridge:\/\//i.test(text)) {
    let q;
    try {
      // Normalise to a parseable URL; the query is what matters.
      const u = new URL(text);
      q = u.searchParams;
    } catch {
      // Fallback: grab the query string manually.
      const qs = text.split("?")[1] || "";
      q = new URLSearchParams(qs);
    }
    const out = {};
    const bool = (v) => v === "1" || v === "true" || v === "yes";
    if (q.has("server")) out.server = q.get("server");
    if (q.has("token")) out.token = q.get("token");
    if (q.has("device")) out.device = q.get("device");
    if (q.has("engine")) out.engine = q.get("engine");
    if (q.has("browser")) out.browser = bool(q.get("browser"));
    if (q.has("headless")) out.headless = bool(q.get("headless"));
    if (q.has("shell")) out.shell = bool(q.get("shell"));
    return out;
  }

  // JSON?
  if (text.startsWith("{")) {
    try {
      const o = JSON.parse(text);
      return o && typeof o === "object" ? o : null;
    } catch {
      /* fall through */
    }
  }

  // Command-line flags?
  if (text.includes("--server") || text.includes("--token")) {
    const toks = text.match(/"[^"]*"|'[^']*'|\S+/g) || [];
    const strip = (s) => s.replace(/^['"]|['"]$/g, "");
    const out = {};
    for (let i = 0; i < toks.length; i++) {
      const t = toks[i];
      const next = () => strip(toks[++i] ?? "");
      if (t === "--server") out.server = next();
      else if (t === "--token") out.token = next();
      else if (t === "--device") out.device = next();
      else if (t === "--browser-engine") out.engine = next();
      else if (t === "--headless") out.headless = true;
      else if (t === "--shell") out.shell = true;
      else if (t === "--no-browser") out.browser = false;
    }
    // Presence of --server without --no-browser implies browser on.
    if (out.browser === undefined) out.browser = true;
    return out;
  }

  // Bare "server [token]".
  const parts = text.split(/\s+/);
  if (/^wss?:\/\//i.test(parts[0])) {
    return { server: parts[0], token: parts[1] || "" };
  }
  return null;
}

// Merge a parsed partial onto the current form (only overwrite provided
// fields).
function applyParsed(p) {
  const cur = readForm();
  const merged = {
    server: p.server ?? cur.server,
    token: p.token ?? cur.token,
    device: p.device ?? cur.device,
    browser: p.browser ?? cur.browser,
    engine: p.engine ?? cur.engine,
    headless: p.headless ?? cur.headless,
    shell: p.shell ?? cur.shell,
  };
  writeForm(merged);
}

// Tauri's WebView doesn't wire up the standard edit shortcuts by default
// (notably Ctrl/Cmd+V paste on Windows), so implement them for text
// inputs manually. Covers cut / copy / paste / select-all.
function installEditingShortcuts() {
  document.addEventListener("keydown", async (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) return;
    const el = document.activeElement;
    const isInput =
      el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA");
    if (!isInput) return;
    const key = e.key.toLowerCase();
    try {
      if (key === "v") {
        e.preventDefault();
        const text = await navigator.clipboard.readText();
        const s = el.selectionStart ?? el.value.length;
        const en = el.selectionEnd ?? el.value.length;
        el.value = el.value.slice(0, s) + text + el.value.slice(en);
        const pos = s + text.length;
        el.setSelectionRange(pos, pos);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      } else if (key === "c" || key === "x") {
        const s = el.selectionStart ?? 0;
        const en = el.selectionEnd ?? 0;
        const sel = el.value.slice(s, en);
        if (sel) {
          e.preventDefault();
          await navigator.clipboard.writeText(sel);
          if (key === "x") {
            el.value = el.value.slice(0, s) + el.value.slice(en);
            el.setSelectionRange(s, s);
            el.dispatchEvent(new Event("input", { bubbles: true }));
          }
        }
      } else if (key === "a") {
        e.preventDefault();
        el.select();
      }
    } catch (err) {
      /* clipboard perms / no selection — ignore */
    }
  });
}

async function init() {
  installEditingShortcuts();
  try {
    const cfg = await invoke("load_config");
    writeForm(cfg);
  } catch (e) {
    console.error("load_config failed", e);
  }

  $("pasteBtn").addEventListener("click", async () => {
    let raw = "";
    try {
      raw = await navigator.clipboard.readText();
    } catch {
      flash("Clipboard unavailable");
      return;
    }
    const parsed = parseConfig(raw);
    if (!parsed || (!parsed.server && !parsed.token)) {
      flash("Couldn't parse clipboard — expected a tsbridge command, JSON, or wss:// URL");
      return;
    }
    applyParsed(parsed);
    // Auto-save immediately after paste.
    await invoke("save_config", { cfg: readForm() });
    flash("Config saved \u2713");
    setTimeout(() => invoke("hide_window"), 800);
  });

  $("saveBtn").addEventListener("click", async () => {
    await invoke("save_config", { cfg: readForm() });
    $("saveBtn").textContent = "Saved \u2713";
    setTimeout(() => invoke("hide_window"), 800);
  });
  $("hideBtn").addEventListener("click", async () => {
    await invoke("hide_window");
  });

}

init();
