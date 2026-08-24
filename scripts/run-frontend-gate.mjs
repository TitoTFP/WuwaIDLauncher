import { cp, lstat, mkdtemp, rm, symlink } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import { spawn } from "node:child_process";
import process from "node:process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const gate = process.argv[2];
const gates = {
  check: { binary: "svelte-check", args: ["--tsconfig", "./tsconfig.json"] },
  build: { binary: "vite", args: ["build"] },
};

if (!gates[gate]) {
  console.error(`Unknown frontend gate: ${gate ?? "(missing)"}`);
  process.exit(2);
}

const executableName = (binary) =>
  process.platform === "win32" ? `${binary}.cmd` : binary;
const run = (executable, args, options = {}) =>
  new Promise((done) => {
    const child = spawn(executable, args, {
      cwd: options.cwd ?? root,
      env: options.env ?? process.env,
      shell: process.platform === "win32",
      stdio: "inherit",
    });
    child.once("error", (error) => {
      console.error(`Failed to run ${executable}: ${error.message}`);
      done(1);
    });
    child.once("exit", (code, signal) => {
      done(code ?? (signal ? 1 : 0));
    });
  });

const { binary, args } = gates[gate];
const localExecutable = join(
  root,
  "node_modules",
  ".bin",
  executableName(binary),
);

if (await pathExists(localExecutable)) {
  process.exitCode = await run(localExecutable, args);
} else {
  const nodeModules = join(root, "node_modules");
  if (await pathExists(nodeModules)) {
    console.error(`Missing ${binary} in an existing node_modules directory`);
    process.exitCode = 1;
  } else {
    const temporaryRoot = await mkdtemp(join(root, ".frontend-validation-"));
    const temporaryNodeModules = join(temporaryRoot, "node_modules");
    const dist = join(root, "dist");
    const hadDist = await pathExists(dist);
    let linked = false;
    let exitCode = 1;

    try {
      await cp(join(root, "package.json"), join(temporaryRoot, "package.json"));
      await cp(
        join(root, "package-lock.json"),
        join(temporaryRoot, "package-lock.json"),
      );
      exitCode = await run(
        process.platform === "win32" ? "npm.cmd" : "npm",
        ["ci", "--no-audit", "--no-fund"],
        {
          cwd: temporaryRoot,
          env: {
            ...process.env,
            npm_config_cache: join(temporaryRoot, ".npm-cache"),
          },
        },
      );

      if (exitCode === 0) {
        await symlink(
          temporaryNodeModules,
          nodeModules,
          process.platform === "win32" ? "junction" : "dir",
        );
        linked = true;
        exitCode = await run(
          join(temporaryNodeModules, ".bin", executableName(binary)),
          args,
        );
      }
    } finally {
      if (linked) {
        await rm(nodeModules, { force: true, recursive: true });
      }
      if (gate === "build" && !hadDist) {
        await rm(dist, { force: true, recursive: true });
      }
      await rm(temporaryRoot, { force: true, recursive: true });
    }

    process.exitCode = exitCode;
  }
}

async function pathExists(path) {
  try {
    await lstat(path);
    return true;
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
}
