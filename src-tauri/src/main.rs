// Tauri desktop app for Tianshu Bridge.
//
// A native tray app (Windows / macOS / Linux) wrapping the `tsbridge`
// CLI: a tray icon with Start/Stop/Settings/Quit, and a settings window
// (the WebView UI in ../ui). It reads/writes the SAME config.json the CLI
// tray + Swift app use (~/.tianshu-bridge/config.json), spawns `tsbridge
// --server …` as a child, and reflects run state in the tray + UI.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::{Child, Command};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, State,
};

// ─── config (mirrors app/TianshuBridge.swift + tray.ts) ─────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
struct BridgeConfig {
    #[serde(default = "default_server")]
    server: String,
    #[serde(default)]
    token: String,
    #[serde(default = "default_true")]
    browser: bool,
    #[serde(default = "default_engine")]
    engine: String,
    #[serde(default)]
    headless: bool,
    #[serde(default)]
    shell: bool,
    #[serde(default)]
    device: String,
}

fn default_server() -> String {
    "ws://localhost:3110/ws".into()
}
fn default_true() -> bool {
    true
}
fn default_engine() -> String {
    "own".into()
}

impl Default for BridgeConfig {
    fn default() -> Self {
        BridgeConfig {
            server: default_server(),
            token: String::new(),
            browser: true,
            engine: default_engine(),
            headless: false,
            shell: false,
            device: String::new(),
        }
    }
}

fn config_dir() -> std::path::PathBuf {
    let home = dirs_home();
    home.join(".tianshu-bridge")
}
fn config_path() -> std::path::PathBuf {
    config_dir().join("config.json")
}
fn log_path() -> std::path::PathBuf {
    config_dir().join("bridge.log")
}

/// Cross-platform home dir without pulling the `dirs` crate.
fn dirs_home() -> std::path::PathBuf {
    #[cfg(windows)]
    {
        if let Ok(p) = std::env::var("USERPROFILE") {
            return std::path::PathBuf::from(p);
        }
    }
    if let Ok(p) = std::env::var("HOME") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from(".")
}

fn load_config_file() -> BridgeConfig {
    match std::fs::read_to_string(config_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => BridgeConfig::default(),
    }
}

fn save_config_file(cfg: &BridgeConfig) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(config_path(), json).map_err(|e| e.to_string())
}

// ─── bridge child process ───────────────────────────────────────────

#[derive(Default)]
struct BridgeState {
    child: Mutex<Option<Child>>,
}

impl BridgeState {
    fn running(&self) -> bool {
        let mut guard = self.child.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    *guard = None; // exited
                    false
                }
                Ok(None) => true, // still running
                Err(_) => false,
            }
        } else {
            false
        }
    }

    fn start(&self, cfg: &BridgeConfig, app: &tauri::AppHandle) -> Result<(), String> {
        self.stop();
        let mut args: Vec<String> = vec!["--server".into(), cfg.server.clone()];
        if !cfg.token.is_empty() {
            args.push("--token".into());
            args.push(cfg.token.clone());
        }
        if cfg.browser {
            if cfg.engine == "stealth" {
                args.push("--browser-engine".into());
                args.push("stealth".into());
            }
            if cfg.headless {
                args.push("--headless".into());
            }
        } else {
            args.push("--no-browser".into());
        }
        if cfg.shell {
            args.push("--shell".into());
        }
        if !cfg.device.is_empty() {
            args.push("--device".into());
            args.push(cfg.device.clone());
        }

        // Log bridge stdout/stderr to ~/.tianshu-bridge/bridge.log.
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path());

        let (cmd, mut pre_args) = bridge_command(app)?;
        // pre_args already contains the bridge entry .js when using the
        // Node sidecar; append the bridge CLI flags after it.
        pre_args.extend(args);
        let mut command = Command::new(cmd);
        command.args(&pre_args);
        if let Ok(f) = log {
            let f2 = f.try_clone().ok();
            command.stdout(std::process::Stdio::from(f));
            if let Some(f2) = f2 {
                command.stderr(std::process::Stdio::from(f2));
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: don't pop a console for the bridge child.
            command.creation_flags(0x0800_0000);
        }
        let child = command.spawn().map_err(|e| format!("spawn failed: {e}"))?;
        *self.child.lock().unwrap() = Some(child);
        Ok(())
    }

    fn stop(&self) {
        let mut guard = self.child.lock().unwrap();
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Resolve how to launch the bundled bridge.
///
/// The app is self-contained: it ships a Node runtime as a Tauri sidecar
/// binary and the bridge's compiled JS as a bundled resource. We run
/// `<node-sidecar> <resources>/bridge/index.js`. No global `tsbridge`,
/// no npm install by the user.
///
/// Dev override: set BRIDGE_ENTRY (+ optional BRIDGE_NODE) to point at a
/// checkout's dist so you can iterate without rebundling.
fn bridge_command(app: &tauri::AppHandle) -> Result<(String, Vec<String>), String> {
    // Dev override.
    if let Ok(entry) = std::env::var("BRIDGE_ENTRY") {
        let node = std::env::var("BRIDGE_NODE").unwrap_or_else(|_| "node".into());
        return Ok((node, vec![entry]));
    }

    // Node sidecar: Tauri places externalBin next to the app executable,
    // named plainly (`node` / `node.exe`) after stripping the triple.
    let node = std::env::current_exe()
        .map(|p| p.with_file_name(node_bin_name()))
        .map_err(|e| e.to_string())?;

    // Bridge entry JS: bundled resource resources/bridge/index.js.
    let entry = app
        .path()
        .resolve("resources/bridge/index.js", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("bridge payload not found: {e}"))?;

    let node_str = node.to_string_lossy().to_string();
    let entry_str = entry.to_string_lossy().to_string();
    Ok((node_str, vec![entry_str]))
}

fn node_bin_name() -> &'static str {
    #[cfg(windows)]
    {
        "node.exe"
    }
    #[cfg(not(windows))]
    {
        "node"
    }
}

// ─── invoke commands (called from the UI) ───────────────────────────

#[tauri::command]
fn load_config() -> BridgeConfig {
    load_config_file()
}

#[tauri::command]
fn save_config(cfg: BridgeConfig) -> Result<(), String> {
    save_config_file(&cfg)
}

#[tauri::command]
fn start_bridge(state: State<BridgeState>, app: tauri::AppHandle) -> Result<(), String> {
    let cfg = load_config_file();
    state.start(&cfg, &app)?;
    let _ = app.emit("bridge-status", true);
    Ok(())
}

#[tauri::command]
fn stop_bridge(state: State<BridgeState>, app: tauri::AppHandle) -> Result<(), String> {
    state.stop();
    let _ = app.emit("bridge-status", false);
    Ok(())
}

#[tauri::command]
fn bridge_status(state: State<BridgeState>) -> bool {
    state.running()
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

// ─── app setup: tray + window ───────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(BridgeState::default())
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            start_bridge,
            stop_bridge,
            bridge_status,
            hide_window,
        ])
        .setup(|app| {
            // Tray menu: Settings / Start / Stop / Quit.
            let settings_i = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let start_i = MenuItem::with_id(app, "start", "Start", true, None::<&str>)?;
            let stop_i = MenuItem::with_id(app, "stop", "Stop", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_i, &start_i, &stop_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "start" => {
                        let state = app.state::<BridgeState>();
                        let cfg = load_config_file();
                        let _ = state.start(&cfg, app);
                        let _ = app.emit("bridge-status", true);
                    }
                    "stop" => {
                        let state = app.state::<BridgeState>();
                        state.stop();
                        let _ = app.emit("bridge-status", false);
                    }
                    "quit" => {
                        let state = app.state::<BridgeState>();
                        state.stop();
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Start hidden: the app lives in the tray. The window shows
            // only when the user picks Settings.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
            Ok(())
        })
        // Closing the settings window hides it instead of quitting.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
