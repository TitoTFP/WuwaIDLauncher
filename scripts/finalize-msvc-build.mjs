import { copyFile, stat } from "node:fs/promises";
import { join } from "node:path";
import process from "node:process";

const releaseDirectory = join(
  process.cwd(),
  "src-tauri",
  "target",
  "x86_64-pc-windows-msvc",
  "release",
);
const cargoExecutable = join(releaseDirectory, "wuwaid-launcher.exe");
const launcherExecutable = join(releaseDirectory, "WuwaIDLauncher.exe");
const cargoArtifact = await stat(cargoExecutable);

if (cargoArtifact.size <= 1024 * 1024) {
  throw new Error(
    `Launcher binary too small for release: ${cargoArtifact.size} bytes`,
  );
}

await copyFile(cargoExecutable, launcherExecutable);
const launcherArtifact = await stat(launcherExecutable);

if (launcherArtifact.size !== cargoArtifact.size) {
  throw new Error(
    `Final launcher size mismatch: ${launcherArtifact.size} vs ${cargoArtifact.size} bytes`,
  );
}

console.log(
  `MSVC launcher: ${launcherExecutable} (${launcherArtifact.size} bytes)`,
);
