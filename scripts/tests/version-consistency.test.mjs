import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);

async function read(relativePath) {
  return readFile(path.join(repoRoot, relativePath), "utf8");
}

function firstMatch(text, pattern, label) {
  const match = text.match(pattern);
  assert.ok(match, `${label} is missing`);
  return match[1];
}

test("release metadata and frontend fallback use one version", async () => {
  const packageJson = JSON.parse(await read("package.json"));
  const packageLock = JSON.parse(await read("package-lock.json"));
  const cargoToml = await read("src-tauri/Cargo.toml");
  const cargoLock = await read("src-tauri/Cargo.lock");
  const tauriConfig = JSON.parse(await read("src-tauri/tauri.conf.json"));
  const readme = await read("README.md");
  const releaseNotes = await read(
    `.github/release-notes/v${packageJson.version}.md`,
  );
  const launcherState = await read("src/lib/launcherState.svelte.ts");
  const version = packageJson.version;

  assert.equal(packageLock.version, version, "package-lock version mismatch");
  assert.equal(
    packageLock.packages?.[""]?.version,
    version,
    "package-lock root version mismatch",
  );
  assert.equal(
    firstMatch(cargoToml, /^version\s*=\s*"([^"]+)"/m, "Cargo.toml version"),
    version,
  );
  assert.equal(
    firstMatch(
      cargoLock,
      /\[\[package\]\]\s+name\s*=\s*"wuwaid-launcher"\s+version\s*=\s*"([^"]+)"/,
      "Cargo.lock root package",
    ),
    version,
  );
  assert.equal(tauriConfig.version, version, "Tauri config version mismatch");
  assert.match(readme, new RegExp(`Version-${version.replaceAll(".", "\\.")}`));
  assert.match(
    releaseNotes,
    new RegExp(`^# WuwaID Launcher v${version}$`, "m"),
  );
  assert.equal(
    firstMatch(
      launcherState,
      /appVersion:\s*string\s*=\s*\$state<string>\("([^"]+)"\)/,
      "frontend fallback version",
    ),
    version,
  );

  console.log(`version-consistency: ${version}`);
});
