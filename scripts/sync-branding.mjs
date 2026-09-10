import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const checkOnly = process.argv.includes("--check");
const config = readJson("brand.config.json");

for (const field of [
  "displayName",
  "packageName",
  "rustLibName",
  "desktopBinaryName",
  "cliName",
  "bundleIdentifier",
  "storagePrefix",
  "projectDataDirectory",
  "startupRegistryName",
  "cliRegistryPath",
  "cliUninstallKey",
  "pluginNamespace",
  "legacyBundleIdentifier",
  "legacyStoragePrefix",
  "legacyProjectDataDirectory",
  "repository"
]) {
  if (typeof config[field] !== "string" || !config[field].trim()) {
    throw new Error(`brand.config.json field ${field} must be a non-empty string`);
  }
}

if (!/^[a-z0-9-]+$/.test(config.packageName)) {
  throw new Error("packageName must contain only lowercase letters, digits, and hyphens");
}
if (!/^[a-z_][a-z0-9_]*$/.test(config.rustLibName)) {
  throw new Error("rustLibName must be a valid Rust crate identifier");
}
if (config.desktopBinaryName !== config.packageName) {
  throw new Error("desktopBinaryName must match packageName for the default Cargo binary");
}

const packageJson = readJson("package.json");
packageJson.name = config.packageName;
syncJson("package.json", packageJson);

const packageLock = readJson("package-lock.json");
packageLock.name = config.packageName;
if (packageLock.packages?.[""]) {
  packageLock.packages[""].name = config.packageName;
}
syncJson("package-lock.json", packageLock);

const tauriConfig = readJson("src-tauri/tauri.conf.json");
tauriConfig.productName = config.displayName;
tauriConfig.identifier = config.bundleIdentifier;
for (const window of tauriConfig.app?.windows ?? []) {
  window.title = config.displayName;
}
syncJson("src-tauri/tauri.conf.json", tauriConfig);

const cargoPath = "src-tauri/Cargo.toml";
let cargo = readText(cargoPath);
cargo = replaceTomlValue(cargo, "package", "name", config.packageName);
cargo = replaceTomlValue(cargo, "package", "authors", `["${config.displayName}"]`, false);
cargo = replaceTomlValue(cargo, "package", "default-run", config.desktopBinaryName);
cargo = replaceTomlValue(cargo, "lib", "name", config.rustLibName);
syncText(cargoPath, cargo);

for (const sourcePath of ["src-tauri/src/main.rs", "src-tauri/src/bin/nano.rs"]) {
  const source = readText(sourcePath);
  const expected = source.replace(/\b[a-z_][a-z0-9_]*_lib::/g, `${config.rustLibName}::`);
  syncText(sourcePath, expected);
}

console.log(checkOnly ? "Brand configuration is in sync." : "Brand configuration synchronized.");

function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

function readText(relativePath) {
  return readFileSync(join(root, relativePath), "utf8");
}

function syncJson(relativePath, value) {
  const path = join(root, relativePath);
  const current = JSON.parse(readFileSync(path, "utf8"));
  if (JSON.stringify(current) === JSON.stringify(value)) {
    return;
  }
  if (checkOnly) {
    throw new Error(`${relativePath} is out of sync; run npm run brand:sync`);
  }
  const currentText = readFileSync(path, "utf8");
  const eol = currentText.includes("\r\n") ? "\r\n" : "\n";
  writeFileSync(path, `${JSON.stringify(value, null, 2).replace(/\n/g, eol)}${eol}`);
}

function syncText(relativePath, expected) {
  const path = join(root, relativePath);
  const current = readFileSync(path, "utf8");
  const normalizedCurrent = current.replace(/\r\n/g, "\n");
  const normalizedExpected = expected.replace(/\r\n/g, "\n");
  if (normalizedCurrent === normalizedExpected) {
    return;
  }
  if (checkOnly) {
    throw new Error(`${relativePath} is out of sync; run npm run brand:sync`);
  }
  const output = current.includes("\r\n") ? normalizedExpected.replace(/\n/g, "\r\n") : normalizedExpected;
  writeFileSync(path, output);
}

function replaceTomlValue(source, section, key, value, quote = true) {
  const sectionPattern = new RegExp(`(\\[${escapeRegExp(section)}\\][\\s\\S]*?^${escapeRegExp(key)}\\s*=\\s*)([^\\r\\n]+)`, "m");
  if (!sectionPattern.test(source)) {
    throw new Error(`Could not find ${section}.${key} in src-tauri/Cargo.toml`);
  }
  return source.replace(sectionPattern, `$1${quote ? `"${value}"` : value}`);
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
