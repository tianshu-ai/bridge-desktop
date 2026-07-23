// Frontend for the Tauri settings window. Talks to the Rust backend via
// Tauri's invoke() commands: load_config / save_config / start_bridge /
// stop_bridge / bridge_status.
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);

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

async function init() {
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
