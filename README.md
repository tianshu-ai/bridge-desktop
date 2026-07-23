# Tianshu Bridge — Desktop

A native desktop tray app for [Tianshu Bridge](https://github.com/tianshu-ai/local-bridge),
built with [Tauri](https://tauri.app). Download an installer, double-click,
and manage the bridge from a tray icon + settings window — **no CLI, no
Node install** required.

## What it is

- **Tray app** (Windows / macOS / Linux): Start / Stop / Settings / Quit.
- **Settings window** (native WebView): server URL, token, browser
  engine (own / stealth), headless, shell, device.
- **Self-contained**: the app bundles a Node runtime (Tauri sidecar) and
  the `@tianshu-ai/local-bridge` payload, so it runs the bridge itself.
  Users install nothing else.
- Reads/writes `~/.tianshu-bridge/config.json` — the same file the CLI
  and the legacy macOS Swift app use.

## Download

Grab the installer for your OS from the
[Releases](https://github.com/tianshu-ai/bridge-desktop/releases) page:

| OS | File |
| --- | --- |
| Windows | `.msi` / `.exe` |
| macOS (Apple Silicon) | `..._aarch64.dmg` |
| macOS (Intel) | `..._x64.dmg` |
| Linux | `.AppImage` |

## Develop

```bash
npm install
# fetch the bridge payload + a Node sidecar for this platform:
NODE_MIRROR=https://npmmirror.com/mirrors/node \
  node scripts/prepare-payload.mjs --node-version v22.14.0
npm run dev          # tauri dev
npm run build        # tauri build → installers under src-tauri/target/.../bundle
```

Dev without rebundling the payload: point at a bridge checkout's dist —

```bash
BRIDGE_ENTRY=/path/to/local-bridge/dist/index.js npm run dev
```

## How it works

```
Tauri (Rust)  ── tray + settings window + process mgmt
     │
     └─ spawns:  <bundled node>  <bundled bridge>/index.js  --server … --token …
                      │                    │
              MacOS/node (sidecar)   Resources/resources/bridge (dist + node_modules)
```

Browser tools still spawn `@playwright/mcp` / `cloakbrowser-mcp` on
demand (they're Node MCP servers) using the bundled Node.

## CI

`.github/workflows/release.yml` builds on `windows-latest`,
`macos-latest` (arm64), `macos-13` (x64), and `ubuntu-22.04`, runs
`prepare-payload.mjs` per platform, `tauri build`, and attaches the
installers to a GitHub Release on `v*` tags.
