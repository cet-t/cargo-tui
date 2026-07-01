#!/usr/bin/env node
// Wrapper that resolves the platform-specific binary and executes it.
const { execFileSync } = require("child_process");
const path = require("path");

const PLATFORMS = {
  "win32-x64":    { pkg: "cargo-tui-win32-x64",    bin: "cargo-tui.exe" },
  "linux-x64":    { pkg: "cargo-tui-linux-x64",    bin: "cargo-tui"     },
  "darwin-x64":   { pkg: "cargo-tui-darwin-x64",   bin: "cargo-tui"     },
  "darwin-arm64": { pkg: "cargo-tui-darwin-arm64",  bin: "cargo-tui"     },
};

const key = `${process.platform}-${process.arch}`;
const entry = PLATFORMS[key];

if (!entry) {
  console.error(`[cargo-tui] Unsupported platform: ${key}`);
  process.exit(1);
}

let binPath;
try {
  binPath = require.resolve(`${entry.pkg}/bin/${entry.bin}`);
} catch {
  console.error(
    `[cargo-tui] Platform binary package "${entry.pkg}" is not installed.\n` +
    `Run: bun add ${entry.pkg}`
  );
  process.exit(1);
}

try {
  execFileSync(binPath, process.argv.slice(2), { stdio: "inherit" });
} catch (e) {
  process.exit(e.status ?? 1);
}
