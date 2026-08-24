import { spawn } from "node:child_process";
import { access } from "node:fs/promises";
import { join } from "node:path";
import process from "node:process";

const binary = process.platform === "win32" ? "tauri.cmd" : "tauri";
const executable = join(process.cwd(), "node_modules", ".bin", binary);
const args = process.argv.slice(2);
const command = args[0];

if ((command === "build" || command === "dev") && !hasFeatureFlag(args)) {
  args.push("--features", "app");
}

try {
  await access(executable);
} catch {
  console.error(
    "Tauri CLI is unavailable; run npm ci before invoking npm run tauri.",
  );
  process.exit(1);
}

const child = spawn(executable, args, {
  cwd: process.cwd(),
  env: process.env,
  shell: process.platform === "win32",
  stdio: "inherit",
});

child.once("error", (error) => {
  console.error(`Failed to run Tauri CLI: ${error.message}`);
  process.exitCode = 1;
});
child.once("exit", (code, signal) => {
  process.exitCode = code ?? (signal ? 1 : 0);
});

function hasFeatureFlag(argumentsList) {
  return argumentsList.some(
    (argument) => argument === "--features" || argument === "-f",
  );
}
