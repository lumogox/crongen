import { readFileSync } from "node:fs";

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function readCargoVersion(path) {
  const cargoToml = readFileSync(path, "utf8");
  const match = cargoToml.match(/^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);

  if (!match) {
    throw new Error(`Unable to find [package].version in ${path}`);
  }

  return match[1];
}

const tagName = process.argv[2]?.trim() ?? "";
const packageVersion = readJson("package.json").version;
const tauriVersion = readJson("src-tauri/tauri.conf.json").version;
const cargoVersion = readCargoVersion("src-tauri/Cargo.toml");

const issues = [];

if (packageVersion !== tauriVersion) {
  issues.push(`package.json version ${packageVersion} does not match src-tauri/tauri.conf.json version ${tauriVersion}.`);
}

if (cargoVersion !== tauriVersion) {
  issues.push(`src-tauri/Cargo.toml version ${cargoVersion} does not match src-tauri/tauri.conf.json version ${tauriVersion}.`);
}

if (tagName) {
  if (!/^v\d+\.\d+\.\d+$/.test(tagName)) {
    issues.push(`Git tag ${tagName} must match the stable release format vX.Y.Z.`);
  } else {
    const tagVersion = tagName.slice(1);

    if (tagVersion !== tauriVersion) {
      issues.push(`Git tag ${tagName} does not match src-tauri/tauri.conf.json version ${tauriVersion}.`);
    }
  }
}

if (issues.length > 0) {
  console.error("Release version validation failed:");
  for (const issue of issues) {
    console.error(`- ${issue}`);
  }
  process.exit(1);
}

console.log(
  tagName
    ? `Release version validation passed for ${tagName} (${tauriVersion}).`
    : `Release version validation passed for ${tauriVersion}.`
);
