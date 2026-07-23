#!/usr/bin/env node
// Prepare the bundled payload for the desktop app:
//   1. resources/bridge/  ← the @tianshu-ai/local-bridge CLI's compiled
//      dist + its production node_modules (the bridge logic we run via
//      the Node sidecar).
//   2. src-tauri/binaries/node-<target-triple>[.exe]  ← a Node runtime
//      sidecar, so the app is self-contained (user installs nothing).
//
// Tauri's externalBin convention requires the sidecar file to be named
// `<name>-<rustc target triple>` (e.g. node-x86_64-pc-windows-msvc.exe).
//
// Usage:
//   node scripts/prepare-payload.mjs [--bridge-version <semver|latest>]
//                                    [--node-version v22.x.y]
//
// In CI each OS runner runs this for its own platform before `tauri build`.

import { execSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import https from "node:https";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "..");
const srcTauri = path.join(root, "src-tauri");

const args = process.argv.slice(2);
function argOf(name, def) {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : def;
}
const BRIDGE_VERSION = argOf("--bridge-version", "latest");
const NODE_VERSION = argOf("--node-version", process.version); // default: this Node

// ─── target triple (matches rustc / Tauri externalBin naming) ───────

function rustTargetTriple() {
  // Allow explicit override (CI sets this).
  if (process.env.TARGET_TRIPLE) return process.env.TARGET_TRIPLE;
  try {
    const out = execSync("rustc -vV", { encoding: "utf8" });
    const m = out.match(/host:\s*(\S+)/);
    if (m) return m[1];
  } catch {
    /* fall through */
  }
  // Best-effort fallback from node's platform/arch.
  const p = process.platform;
  const a = process.arch;
  if (p === "darwin") return a === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";
  if (p === "win32") return "x86_64-pc-windows-msvc";
  return a === "arm64" ? "aarch64-unknown-linux-gnu" : "x86_64-unknown-linux-gnu";
}

// ─── 1. bridge dist ─────────────────────────────────────────────────

function prepareBridge() {
  const dest = path.join(srcTauri, "resources", "bridge");
  fs.rmSync(dest, { recursive: true, force: true });
  fs.mkdirSync(dest, { recursive: true });

  // Install the published bridge package into a temp dir, then copy its
  // dist + node_modules into resources/bridge. Using the published npm
  // package keeps the desktop repo decoupled from the CLI source.
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "bridge-payload-"));
  const spec = `@tianshu-ai/local-bridge@${BRIDGE_VERSION}`;
  console.log(`[payload] installing ${spec} …`);
  fs.writeFileSync(path.join(tmp, "package.json"), JSON.stringify({ name: "x", private: true }));
  execSync(`npm install --omit=dev --no-audit --no-fund ${spec}`, {
    cwd: tmp,
    stdio: "inherit",
  });
  const pkgDir = path.join(tmp, "node_modules", "@tianshu-ai", "local-bridge");
  // Copy dist + the package's own node_modules (production deps).
  cpDir(path.join(pkgDir, "dist"), path.join(dest, "dist"));
  // Entry shim so the sidecar can run `node index.js`.
  fs.writeFileSync(
    path.join(dest, "index.js"),
    `import "./dist/index.js";\n`,
  );
  // Production node_modules live at the temp root's node_modules.
  cpDir(path.join(tmp, "node_modules"), path.join(dest, "node_modules"));
  // package.json with "type":"module" so the ESM entry runs.
  fs.writeFileSync(
    path.join(dest, "package.json"),
    JSON.stringify({ name: "tianshu-bridge-payload", private: true, type: "module" }, null, 2),
  );
  console.log(`[payload] bridge → ${dest}`);
}

function cpDir(src, dst) {
  if (!fs.existsSync(src)) return;
  fs.cpSync(src, dst, { recursive: true });
}

// ─── 2. node sidecar ────────────────────────────────────────────────

async function prepareNode() {
  const triple = rustTargetTriple();
  const binDir = path.join(srcTauri, "binaries");
  fs.mkdirSync(binDir, { recursive: true });
  const ext = process.platform === "win32" ? ".exe" : "";
  const destBin = path.join(binDir, `node-${triple}${ext}`);

  const ver = NODE_VERSION.startsWith("v") ? NODE_VERSION : `v${NODE_VERSION}`;
  const { url, inner } = nodeDownload(ver);
  console.log(`[payload] fetching Node ${ver} from ${url}`);
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "node-dl-"));
  const archive = path.join(tmp, path.basename(url));
  await download(url, archive);
  // Extract just the node binary.
  if (url.endsWith(".zip")) {
    execSync(`unzip -o -q ${quote(archive)} -d ${quote(tmp)}`);
  } else {
    execSync(`tar -xf ${quote(archive)} -C ${quote(tmp)}`);
  }
  const extracted = path.join(tmp, inner);
  fs.copyFileSync(extracted, destBin);
  if (ext === "") fs.chmodSync(destBin, 0o755);
  console.log(`[payload] node sidecar → ${destBin}`);
}

// Node dist mirror. Defaults to the official dist; set NODE_MIRROR to a
// faster one (e.g. https://npmmirror.com/mirrors/node for China CI/dev).
function nodeBase() {
  const m = (process.env.NODE_MIRROR || "https://nodejs.org/dist").replace(/\/+$/, "");
  return m;
}

function nodeDownload(ver) {
  const base = nodeBase();
  const p = process.platform;
  const a = process.arch;
  if (p === "win32") {
    const arch = a === "arm64" ? "arm64" : "x64";
    return {
      url: `${base}/${ver}/node-${ver}-win-${arch}.zip`,
      inner: `node-${ver}-win-${arch}/node.exe`,
    };
  }
  if (p === "darwin") {
    const arch = a === "arm64" ? "arm64" : "x64";
    return {
      url: `${base}/${ver}/node-${ver}-darwin-${arch}.tar.gz`,
      inner: `node-${ver}-darwin-${arch}/bin/node`,
    };
  }
  const arch = a === "arm64" ? "arm64" : "x64";
  return {
    url: `${base}/${ver}/node-${ver}-linux-${arch}.tar.xz`,
    inner: `node-${ver}-linux-${arch}/bin/node`,
  };
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    https
      .get(url, (res) => {
        if (res.statusCode && res.statusCode >= 300 && res.headers.location) {
          file.close();
          download(res.headers.location, dest).then(resolve, reject);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Error(`HTTP ${res.statusCode} for ${url}`));
          return;
        }
        res.pipe(file);
        file.on("finish", () => file.close(() => resolve()));
      })
      .on("error", reject);
  });
}

function quote(s) {
  return `'${s.replace(/'/g, `'\\''`)}'`;
}

// ─── run ────────────────────────────────────────────────────────────

(async () => {
  prepareBridge();
  await prepareNode();
  console.log("[payload] done.");
})().catch((e) => {
  console.error("[payload] failed:", e);
  process.exit(1);
});
