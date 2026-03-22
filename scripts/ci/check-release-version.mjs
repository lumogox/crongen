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

const releaseInput = process.argv[2]?.trim() ?? "";
const packageVersion = readJson("package.json").version;
const tauriVersion = readJson("src-tauri/tauri.conf.json").version;
const cargoVersion = readCargoVersion("src-tauri/Cargo.toml");

const issues = [];
const stableVersionPattern = /^\d+\.\d+\.\d+$/;

let expectedVersion = "";

if (releaseInput) {
  if (/^v\d+\.\d+\.\d+$/.test(releaseInput)) {
    expectedVersion = releaseInput.slice(1);
  } else if (stableVersionPattern.test(releaseInput)) {
    expectedVersion = releaseInput;
  } else {
    issues.push(
      `Release input ${releaseInput} must match the stable release format X.Y.Z or vX.Y.Z.`
    );
  }
}

if (!stableVersionPattern.test(tauriVersion)) {
  issues.push(`src-tauri/tauri.conf.json version ${tauriVersion} must match the stable release format X.Y.Z.`);
}

if (!releaseInput) {
  if (packageVersion !== tauriVersion) {
    issues.push(
      `package.json version ${packageVersion} does not match src-tauri/tauri.conf.json version ${tauriVersion}.`
    );
  }

  if (cargoVersion !== tauriVersion) {
    issues.push(
      `src-tauri/Cargo.toml version ${cargoVersion} does not match src-tauri/tauri.conf.json version ${tauriVersion}.`
    );
  }

  if (!stableVersionPattern.test(packageVersion)) {
    issues.push(`package.json version ${packageVersion} must match the stable release format X.Y.Z.`);
  }

  if (!stableVersionPattern.test(cargoVersion)) {
    issues.push(`src-tauri/Cargo.toml version ${cargoVersion} must match the stable release format X.Y.Z.`);
  }
} else if (expectedVersion && !stableVersionPattern.test(expectedVersion)) {
  issues.push(`Resolved release version ${expectedVersion} must match the stable release format X.Y.Z.`);
}

if (issues.length > 0) {
  console.error("Release version validation failed:");
  for (const issue of issues) {
    console.error(`- ${issue}`);
  }
  process.exit(1);
}

if (releaseInput) {
  if (releaseInput.startsWith("v")) {
    console.log(`Release input validation passed for tag ${releaseInput}.`);
  } else {
    console.log(`Release input validation passed for version ${releaseInput}.`);
  }
} else {
  console.log(`Release version validation passed for ${tauriVersion}.`);
}
