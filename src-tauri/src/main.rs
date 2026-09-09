// Tauri desktop app for Tianshu Bridge — multi-profile.
//
// A native tray app (Windows / macOS / Linux): each profile gets its own
// status line in the tray menu + start/stop. Config lives at
// ~/.tianshu-bridge/config.json (same as CLI local-bridge v0.12+).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(unix)]
extern crate libc;

use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, State,
};

static ICON_STOPPED: &[u8] = include_bytes!("../icons/tray/stopped.png");
static ICON_RUNNING: &[u8] = include_bytes!("../icons/tray/running.png");
static ICON_RUNNING_2: &[u8] = include_bytes!("../icons/tray/running-2.png");
static ICON_RUNNING_3: &[u8] = include_bytes!("../icons/tray/running-3.png");
static ICON_RUNNING_4: &[u8] = include_bytes!("../icons/tray/running-4.png");
static ICON_RUNNING_5: &[u8] = include_bytes!("../icons/tray/running-5.png");
// Active frames (no count badge — for 0 or 1 profile)
static ICON_ACTIVE_1: &[u8] = include_bytes!("../icons/tray/active-1.png");
static ICON_ACTIVE_2: &[u8] = include_bytes!("../icons/tray/active-2.png");
static ICON_ACTIVE_3: &[u8] = include_bytes!("../icons/tray/active-3.png");
// Active frames with count badges (2-5 profiles)
static ICON_ACTIVE_1_C2: &[u8] = include_bytes!("../icons/tray/active-1-c2.png");
static ICON_ACTIVE_2_C2: &[u8] = include_bytes!("../icons/tray/active-2-c2.png");
static ICON_ACTIVE_3_C2: &[u8] = include_bytes!("../icons/tray/active-3-c2.png");
static ICON_ACTIVE_1_C3: &[u8] = include_bytes!("../icons/tray/active-1-c3.png");
static ICON_ACTIVE_2_C3: &[u8] = include_bytes!("../icons/tray/active-2-c3.png");
static ICON_ACTIVE_3_C3: &[u8] = include_bytes!("../icons/tray/active-3-c3.png");
static ICON_ACTIVE_1_C4: &[u8] = include_bytes!("../icons/tray/active-1-c4.png");
static ICON_ACTIVE_2_C4: &[u8] = include_bytes!("../icons/tray/active-2-c4.png");
static ICON_ACTIVE_3_C4: &[u8] = include_bytes!("../icons/tray/active-3-c4.png");
static ICON_ACTIVE_1_C5: &[u8] = include_bytes!("../icons/tray/active-1-c5.png");
static ICON_ACTIVE_2_C5: &[u8] = include_bytes!("../icons/tray/active-2-c5.png");
static ICON_ACTIVE_3_C5: &[u8] = include_bytes!("../icons/tray/active-3-c5.png");

/// Pick the idle icon for a given connected profile count.
fn icon_for_count(n: usize) -> &'static [u8] {
    match n {
        0 => ICON_STOPPED,
        1 => ICON_RUNNING,    // no badge for 1
        2 => ICON_RUNNING_2,
        3 => ICON_RUNNING_3,
        4 => ICON_RUNNING_4,
        _ => ICON_RUNNING_5,
    }
}

/// Pick the active (pulsing) icon for a given frame and profile count.
fn icon_active(frame: usize, count: usize) -> &'static [u8] {
    let f = frame % 3;
    match count {
        0 | 1 => [ICON_ACTIVE_1, ICON_ACTIVE_2, ICON_ACTIVE_3][f],
        2 => [ICON_ACTIVE_1_C2, ICON_ACTIVE_2_C2, ICON_ACTIVE_3_C2][f],
        3 => [ICON_ACTIVE_1_C3, ICON_ACTIVE_2_C3, ICON_ACTIVE_3_C3][f],
        4 => [ICON_ACTIVE_1_C4, ICON_ACTIVE_2_C4, ICON_ACTIVE_3_C4][f],
        _ => [ICON_ACTIVE_1_C5, ICON_ACTIVE_2_C5, ICON_ACTIVE_3_C5][f],
    }
}

// ─── config ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
struct BridgeProfile {
    #[serde(default = "gen_id")]
    id: String,
    #[serde(default = "default_name")]
    name: String,
    #[serde(default = "default_server")]
    server: String,
    #[serde(default)]
    token: String,
    #[serde(default)]
    device: String,
    #[serde(default = "default_true")]
    auto_start: bool,
    #[serde(default = "default_true")]
    browser: bool,
    #[serde(default = "default_engine")]
    engine: String,
    #[serde(default)]
    headless: bool,
    #[serde(default)]
    shell: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct MultiConfig {
    #[serde(default)]
    profiles: Vec<BridgeProfile>,
}

// Legacy single-profile format (for migration)
#[derive(Deserialize)]
struct LegacyConfig {
    server: Option<String>,
    token: Option<String>,
    browser: Option<bool>,
    engine: Option<String>,
    headless: Option<bool>,
    shell: Option<bool>,
    device: Option<String>,
}

fn gen_id() -> String {
    format!(
        "p_{:x}_{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
        rand_u32()
    )
}
fn rand_u32() -> u32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    std::thread::current().id().hash(&mut h);
    h.finish() as u32
}
fn default_name() -> String { "Default".into() }
fn default_server() -> String { "ws://localhost:3110/ws".into() }
fn default_true() -> bool { true }
fn default_engine() -> String { "own".into() }

impl Default for BridgeProfile {
    fn default() -> Self {
        BridgeProfile {
            id: gen_id(),
            name: default_name(),
            server: default_server(),
            token: String::new(),
            device: String::new(),
            auto_start: true,
            browser: true,
            engine: default_engine(),
            headless: false,
            shell: false,
        }
    }
}

fn config_dir() -> std::path::PathBuf {
    dirs_home().join(".tianshu-bridge")
}
fn config_path() -> std::path::PathBuf {
    config_dir().join("config.json")
}
fn log_path_for(profile_id: &str) -> std::path::PathBuf {
    config_dir().join(format!("bridge-{}.log", profile_id))
}
fn log_path() -> std::path::PathBuf {
    config_dir().join("bridge.log")
}

fn dirs_home() -> std::path::PathBuf {
    #[cfg(windows)]
    { if let Ok(p) = std::env::var("USERPROFILE") { return std::path::PathBuf::from(p); } }
    if let Ok(p) = std::env::var("HOME") { return std::path::PathBuf::from(p); }
    std::path::PathBuf::from(".")
}

fn load_config_file() -> MultiConfig {
    let path = config_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return MultiConfig { profiles: vec![] },
    };

    // Try new multi-profile format first
    if let Ok(cfg) = serde_json::from_str::<MultiConfig>(&content) {
        if !cfg.profiles.is_empty() {
            return cfg;
        }
    }

    // Try legacy single-profile format → migrate
    if let Ok(legacy) = serde_json::from_str::<LegacyConfig>(&content) {
        if let Some(server) = legacy.server {
            let profile = BridgeProfile {
                id: gen_id(),
                name: "Default".into(),
                server,
                token: legacy.token.unwrap_or_default(),
                device: legacy.device.unwrap_or_default(),
                auto_start: true,
                browser: legacy.browser.unwrap_or(true),
                engine: legacy.engine.unwrap_or_else(default_engine),
                headless: legacy.headless.unwrap_or(false),
                shell: legacy.shell.unwrap_or(false),
            };
            let cfg = MultiConfig { profiles: vec![profile] };
            let _ = save_config_file(&cfg);
            return cfg;
        }
    }

    MultiConfig { profiles: vec![] }
}

fn save_config_file(cfg: &MultiConfig) -> Result<(), String> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(config_path(), json).map_err(|e| e.to_string())
}

// ─── trace logging ──────────────────────────────────────────────────

fn trace_log(msg: &str) {
    use std::io::Write;
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true).append(true).open(dir.join("tray.log"))
    {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "[{}] {}", ts, msg);
    }
}

/// Callback for tool activity events parsed from bridge stdout.
type ActivityCallback = Arc<dyn Fn(i32) + Send + Sync>;

fn timestamped_pipe(pipe: impl std::io::Read, path: &std::path::Path, on_activity: Option<ActivityCallback>) {
    use std::io::{BufRead, BufReader, Write};
    let reader = BufReader::new(pipe);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.is_empty() { continue; }

        // Detect tool_activity JSON lines from the bridge child
        if line.starts_with(r#"{"type":"tool_activity"#) {
            if let Some(ref cb) = on_activity {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    if let Some(active) = v.get("active").and_then(|a| a.as_i64()) {
                        cb(active as i32);
                    }
                }
            }
            continue; // Don't log activity lines
        }

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true).open(path)
        {
            let _ = writeln!(f, "[{}] {}", ts, line);
        }
    }
}

// ─── bridge state (multi-profile) ───────────────────────────────────

#[derive(Default)]
struct BridgeState {
    children: Mutex<HashMap<String, Child>>,
    /// Epoch millis of last activity heartbeat from any bridge child.
    /// 0 = idle. Checked by a 500ms timer to toggle the tray icon.
    last_activity_ts: Arc<AtomicU64>,
}

impl BridgeState {
    fn is_running(&self, profile_id: &str) -> bool {
        let mut guard = self.children.lock().unwrap();
        if let Some(child) = guard.get_mut(profile_id) {
            match child.try_wait() {
                Ok(Some(_)) => { guard.remove(profile_id); false }
                Ok(None) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    fn any_running(&self) -> bool {
        let mut guard = self.children.lock().unwrap();
        let mut dead = vec![];
        for (id, child) in guard.iter_mut() {
            if let Ok(Some(_)) = child.try_wait() { dead.push(id.clone()); }
        }
        for id in dead { guard.remove(&id); }
        !guard.is_empty()
    }

    fn running_ids(&self) -> Vec<String> {
        let mut guard = self.children.lock().unwrap();
        let mut dead = vec![];
        for (id, child) in guard.iter_mut() {
            if let Ok(Some(_)) = child.try_wait() { dead.push(id.clone()); }
        }
        for id in &dead { guard.remove(id); }
        guard.keys().cloned().collect()
    }

    fn start(&self, profile: &BridgeProfile, app: &tauri::AppHandle) -> Result<(), String> {
        trace_log(&format!("start({}) server={}", profile.name, profile.server));
        self.stop(&profile.id, Some(app));

        let mut args: Vec<String> = vec!["--server".into(), profile.server.clone()];
        if !profile.token.is_empty() { args.push("--token".into()); args.push(profile.token.clone()); }
        if profile.browser {
            if profile.engine == "stealth" { args.push("--browser-engine".into()); args.push("stealth".into()); }
            if profile.headless { args.push("--headless".into()); }
        } else {
            args.push("--no-browser".into());
        }
        if profile.shell { args.push("--shell".into()); }
        if !profile.device.is_empty() { args.push("--device".into()); args.push(profile.device.clone()); }

        let (cmd, mut pre_args) = bridge_command(app)?;
        pre_args.extend(args);
        trace_log(&format!("spawning: {} {:?}", cmd, pre_args));

        let mut command = Command::new(&cmd);
        command.args(&pre_args);
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        { use std::os::windows::process::CommandExt; command.creation_flags(0x0800_0000); }

        let mut child = command.spawn().map_err(|e| format!("spawn failed: {e}"))?;
        trace_log(&format!("spawned pid={:?} for {}", child.id(), profile.name));

        let log = log_path_for(&profile.id);
        // Activity heartbeat: bridge sends active=1 every 2s while a tool
        // is running, and active=0 on completion. We store the last
        // heartbeat timestamp; a 4s timeout means "no longer active".
        let last_active = Arc::clone(&self.last_activity_ts);
        let activity_cb: ActivityCallback = Arc::new(move |active: i32| {
            use std::sync::atomic::Ordering;
            if active > 0 {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64).unwrap_or(0);
                last_active.store(now, Ordering::Relaxed);
            } else {
                // Tool done — clear immediately
                last_active.store(0, Ordering::Relaxed);
            }
        });
        if let Some(stdout) = child.stdout.take() {
            let p = log.clone();
            let cb = Some(Arc::clone(&activity_cb));
            std::thread::spawn(move || timestamped_pipe(stdout, &p, cb));
        }
        if let Some(stderr) = child.stderr.take() {
            let p = log.clone();
            std::thread::spawn(move || timestamped_pipe(stderr, &p, None));
        }

        self.children.lock().unwrap().insert(profile.id.clone(), child);
        Ok(())
    }

    fn stop(&self, profile_id: &str, app: Option<&tauri::AppHandle>) {
        let mut guard = self.children.lock().unwrap();
        if let Some(mut child) = guard.remove(profile_id) {
            #[cfg(unix)]
            { unsafe { libc::kill(child.id() as i32, libc::SIGTERM); } }
            #[cfg(windows)]
            { let _ = child.kill(); }
            std::thread::sleep(std::time::Duration::from_millis(500));
            if let Ok(None) = child.try_wait() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        drop(guard);
        // Reset icon after stop — ensures we never get stuck on active icon
        if let Some(app) = app {
            update_tray_icon_inner(self, app);
        }
    }

    fn stop_all(&self, app: Option<&tauri::AppHandle>) {
        let ids: Vec<String> = self.children.lock().unwrap().keys().cloned().collect();
        for id in ids { self.stop(&id, None); }
        // Reset icon once after all stopped
        if let Some(app) = app {
            update_tray_icon_inner(self, app);
        }
    }
}

fn bridge_command(app: &tauri::AppHandle) -> Result<(String, Vec<String>), String> {
    if let Ok(entry) = std::env::var("BRIDGE_ENTRY") {
        let node = std::env::var("BRIDGE_NODE").unwrap_or_else(|_| "node".into());
        return Ok((node, vec![entry]));
    }
    let node = std::env::current_exe()
        .map(|p| p.with_file_name(node_bin_name()))
        .map_err(|e| e.to_string())?;
    let entry = app.path()
        .resolve("resources/bridge/index.js", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("bridge payload not found: {e}"))?;
    Ok((node.to_string_lossy().to_string(), vec![entry.to_string_lossy().to_string()]))
}

fn node_bin_name() -> &'static str {
    #[cfg(windows)] { "node.exe" }
    #[cfg(not(windows))] { "node" }
}

// ─── invoke commands ────────────────────────────────────────────────

#[tauri::command]
fn load_config() -> MultiConfig {
    load_config_file()
}

#[tauri::command]
fn save_config(cfg: MultiConfig) -> Result<(), String> {
    save_config_file(&cfg)
}

#[tauri::command]
fn start_profile(id: String, state: State<BridgeState>, app: tauri::AppHandle) -> Result<(), String> {
    let cfg = load_config_file();
    let profile = cfg.profiles.iter().find(|p| p.id == id)
        .ok_or_else(|| format!("profile not found: {id}"))?;
    state.start(profile, &app)?;
    let _ = app.emit("bridge-status-changed", ());
    update_tray_icon_inner(&state, &app);
    Ok(())
}

#[tauri::command]
fn stop_profile(id: String, state: State<BridgeState>, app: tauri::AppHandle) -> Result<(), String> {
    state.stop(&id, Some(&app));
    let _ = app.emit("bridge-status-changed", ());
    Ok(())
}

#[tauri::command]
fn start_all(state: State<BridgeState>, app: tauri::AppHandle) -> Result<(), String> {
    let cfg = load_config_file();
    for p in &cfg.profiles { let _ = state.start(p, &app); }
    let _ = app.emit("bridge-status-changed", ());
    update_tray_icon_inner(&state, &app);
    Ok(())
}

#[tauri::command]
fn stop_all(state: State<BridgeState>, app: tauri::AppHandle) -> Result<(), String> {
    state.stop_all(Some(&app));
    let _ = app.emit("bridge-status-changed", ());
    Ok(())
}

#[derive(Serialize)]
struct ProfileStatusInfo {
    id: String,
    name: String,
    server: String,
    running: bool,
}

#[tauri::command]
fn get_status(state: State<BridgeState>) -> Vec<ProfileStatusInfo> {
    let cfg = load_config_file();
    cfg.profiles.iter().map(|p| ProfileStatusInfo {
        id: p.id.clone(),
        name: p.name.clone(),
        server: p.server.clone(),
        running: state.is_running(&p.id),
    }).collect()
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
}

fn update_tray_icon_inner(state: &BridgeState, app: &tauri::AppHandle) {
    let count = state.running_ids().len();
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(img) = tauri::image::Image::from_bytes(icon_for_count(count)) {
            let _ = tray.set_icon(Some(img));
        }
        let _ = tray.set_tooltip(Some(&format!(
            "Tianshu Bridge: {}",
            if count == 0 { "stopped".to_string() } else { format!("{count} connected") }
        )));
    }
}

/// Update tray icon to reflect how many profiles are running.
#[tauri::command]
fn update_tray_icon(state: State<BridgeState>, app: tauri::AppHandle) {
    let count = state.running_ids().len();
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(img) = tauri::image::Image::from_bytes(icon_for_count(count)) {
            let _ = tray.set_icon(Some(img));
        }
        let _ = tray.set_tooltip(Some(&format!(
            "Tianshu Bridge: {}",
            if count == 0 { "stopped".to_string() } else { format!("{count} connected") }
        )));
    }
}

#[derive(Clone, Serialize)]
struct LogEntry { ts: u64, source: String, text: String }

fn parse_log_line(line: &str) -> (u64, &str) {
    if line.starts_with('[') {
        if let Some(end) = line.find(']') {
            if let Ok(ts) = line[1..end].parse::<u64>() {
                return (ts, line[end + 1..].trim_start());
            }
        }
    }
    (0, line)
}

#[tauri::command]
fn read_logs(max_lines: Option<usize>) -> Vec<LogEntry> {
    let limit = max_lines.unwrap_or(500);
    let dir = config_dir();
    let mut entries = Vec::new();
    // Read tray.log + all bridge-*.log files
    for entry in std::fs::read_dir(&dir).into_iter().flatten() {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".log") { continue; }
        let source = name.trim_end_matches(".log").to_string();
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            for line in content.lines() {
                let (ts, text) = parse_log_line(line);
                entries.push(LogEntry { ts, source: source.clone(), text: text.to_string() });
            }
        }
    }
    entries.sort_by(|a, b| b.ts.cmp(&a.ts));
    entries.truncate(limit);
    entries
}

#[tauri::command]
fn clear_logs() {
    let dir = config_dir();
    for entry in std::fs::read_dir(&dir).into_iter().flatten() {
        let Ok(entry) = entry else { continue };
        if entry.file_name().to_string_lossy().ends_with(".log") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// ─── tray setup ─────────────────────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(BridgeState::default())
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            start_profile,
            stop_profile,
            start_all,
            stop_all,
            get_status,
            hide_window,
            update_tray_icon,
            read_logs,
            clear_logs,
        ])
        .setup(|app| {
            let cfg = load_config_file();
            let profile_count = cfg.profiles.len();

            // Build tray menu
            let status_i = MenuItem::with_id(app, "status",
                &format!("{} profile(s) configured", profile_count), false, None::<&str>)?;
            let settings_i = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let start_all_i = MenuItem::with_id(app, "start_all", "Start All", true, None::<&str>)?;
            let stop_all_i = MenuItem::with_id(app, "stop_all", "Stop All", true, None::<&str>)?;
            let logs_i = MenuItem::with_id(app, "logs", "View Logs…", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let sep3 = PredefinedMenuItem::separator(app)?;

            let menu = Menu::with_items(app, &[
                &status_i, &sep,
                &start_all_i, &stop_all_i, &sep2,
                &settings_i, &logs_i, &sep3,
                &quit_i,
            ])?;

            let icon_stopped = tauri::image::Image::from_bytes(ICON_STOPPED)
                .unwrap_or_else(|_| app.default_window_icon().unwrap().clone());

            let _tray = TrayIconBuilder::with_id("main")
                .icon(icon_stopped)
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "logs" => {
                        if let Some(w) = app.get_webview_window("logs") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "start_all" => {
                        let state = app.state::<BridgeState>();
                        let cfg = load_config_file();
                        for p in &cfg.profiles { let _ = state.start(p, app); }
                        let _ = app.emit("bridge-status-changed", ());
                        update_tray_icon_inner(&state, app);
                    }
                    "stop_all" => {
                        let state = app.state::<BridgeState>();
                        state.stop_all(Some(app));
                        let _ = app.emit("bridge-status-changed", ());
                    }
                    "quit" => {
                        let state = app.state::<BridgeState>();
                        state.stop_all(None);
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Hide settings window on start
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }

            // Auto-start profiles
            let state = app.state::<BridgeState>();
            for p in &cfg.profiles {
                if p.auto_start {
                    let _ = state.start(p, app.handle());
                }
            }
            // Update icon after auto-starts
            update_tray_icon_inner(&state, app.handle());

            // Poll activity heartbeat every 500ms to toggle tray icon.
            // If last_activity_ts is >0 and within 4s → show active icon.
            // If >4s stale or 0 → show normal count icon.
            let poll_app = app.handle().clone();
            let poll_ts = Arc::clone(&state.last_activity_ts);
            let was_active = Arc::new(std::sync::atomic::AtomicBool::new(false));
            std::thread::spawn(move || {
                const TIMEOUT_MS: u64 = 4000;
                let mut frame: usize = 0;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let ts = poll_ts.load(Ordering::Relaxed);
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64).unwrap_or(0);
                    let active = ts > 0 && (now - ts) < TIMEOUT_MS;
                    let prev = was_active.load(Ordering::Relaxed);

                    let s = poll_app.state::<BridgeState>();
                    let count = s.running_ids().len();

                    if active {
                        // Animate: cycle through frames every 500ms
                        if let Some(tray) = poll_app.tray_by_id("main") {
                            let icon_data = icon_active(frame, count);
                            if let Ok(img) = tauri::image::Image::from_bytes(icon_data) {
                                let _ = tray.set_icon(Some(img));
                            }
                        }
                        frame += 1;
                        was_active.store(true, Ordering::Relaxed);
                    } else if prev {
                        // Just became idle — restore static icon
                        if let Some(tray) = poll_app.tray_by_id("main") {
                            if let Ok(img) = tauri::image::Image::from_bytes(icon_for_count(count)) {
                                let _ = tray.set_icon(Some(img));
                            }
                        }
                        frame = 0;
                        was_active.store(false, Ordering::Relaxed);
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
