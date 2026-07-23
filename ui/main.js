// Frontend for the Tauri settings window. Talks to the Rust backend via
// Tauri's invoke() commands: load_config / save_config / start_bridge /
// stop_bridge / bridge_status.
const tauri = window.__TAURI__;
const invoke = tauri?.core?.invoke;
const listen = tauri?.event?.listen;

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

function renderStatus(running) {
  $("dot").className = "dot" + (running ? " on" : "");
  $("statusText").textContent = running ? "Connected" : "Stopped";
  $("startBtn").disabled = running;
  $("stopBtn").disabled = !running;
}

async function refreshStatus() {
  try {
    const running = await invoke("bridge_status");
    renderStatus(running);
  } catch (e) {
    /* ignore */
  }
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
  await refreshStatus();

  $("saveBtn").addEventListener("click", async () => {
    await invoke("save_config", { cfg: readForm() });
    $("saveBtn").textContent = "Saved ✓";
    setTimeout(() => ($("saveBtn").textContent = "Save"), 1200);
  });
  $("startBtn").addEventListener("click", async () => {
    // Save first so Start uses the latest values.
    await invoke("save_config", { cfg: readForm() });
    await invoke("start_bridge");
    await refreshStatus();
  });
  $("stopBtn").addEventListener("click", async () => {
    await invoke("stop_bridge");
    await refreshStatus();
  });
  $("hideBtn").addEventListener("click", async () => {
    await invoke("hide_window");
  });

  // Backend pushes status changes (e.g. bridge child exited).
  try {
    await listen("bridge-status", (ev) => renderStatus(!!ev.payload));
  } catch (e) {
    /* ignore */
  }
}

init();
