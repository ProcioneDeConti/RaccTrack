#!/usr/bin/env node
// Keeps src-tauri/Cargo.toml's [package] version in lockstep with
// package.json — the single source of truth for the app version.
// tauri.conf.json reads package.json directly (its "version" field is a
// path), but Cargo.toml has no equivalent indirection, so this script closes
// the gap. Runs automatically as part of `npm version <bump>`'s lifecycle
// hooks (see the "version" script in package.json), which also stages the
// result so it lands in the same commit npm creates for the bump.

import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";

const { version } = JSON.parse(readFileSync("package.json", "utf8"));
const cargoPath = "src-tauri/Cargo.toml";
const cargoToml = readFileSync(cargoPath, "utf8");

const updated = cargoToml.replace(/^version = "[^"]+"/m, `version = "${version}"`);
if (updated === cargoToml) {
  throw new Error(`Couldn't find a "version = ..." line to update in ${cargoPath}`);
}
writeFileSync(cargoPath, updated);

try {
  execSync("cargo check --quiet", { cwd: "src-tauri", stdio: "inherit" });
} catch {
  console.warn(
    "cargo check failed after the version bump — Cargo.lock may need a manual `cargo check` before committing.",
  );
}

console.log(`Synced src-tauri/Cargo.toml to version ${version}`);
