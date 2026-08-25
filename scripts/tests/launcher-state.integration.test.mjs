import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";
import { build, normalizePath } from "vite";

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);
const tauriFixturePath = path.resolve(
  repoRoot,
  "scripts/tests/launcherStateTauriFixture.js",
);
const tauriFixtureId = normalizePath(tauriFixturePath);
const scenarioPath = path.resolve(
  repoRoot,
  "scripts/tests/launcher-state.integration.ts",
);

test("real Tauri bridge routes backend status events during a legacy-path method switch", async () => {
  const outDir = await mkdtemp(
    path.join(os.tmpdir(), "wuwaid-launcher-state-"),
  );
  const previousStateShim = globalThis.__testState;
  const previousWindow = globalThis.window;
  const previousFixtureRoot = process.env.WUWAID_FIXTURE_ROOT;
  globalThis.__testState = (value) => value;
  globalThis.window = globalThis;
  process.env.WUWAID_FIXTURE_ROOT = repoRoot;

  try {
    await build({
      root: repoRoot,
      logLevel: "error",
      plugins: [
        {
          name: "tauri-ipc-fixture",
          enforce: "pre",
          resolveId(source, importer) {
            if (
              source === "@tauri-apps/api/core" ||
              source === "@tauri-apps/api/event" ||
              (source === "./launcherStateTauriFixture.js" &&
                importer &&
                normalizePath(importer) === normalizePath(scenarioPath))
            ) {
              return tauriFixtureId;
            }
            return null;
          },
        },
      ],
      define: {
        $state: "globalThis.__testState",
        "process.env.WUWAID_FIXTURE_ROOT": JSON.stringify(repoRoot),
      },

      build: {
        target: "esnext",
        outDir,
        emptyOutDir: true,
        minify: false,
        rollupOptions: {
          external: ["node:child_process", "node:readline"],
          input: scenarioPath,
          output: {
            format: "es",
            inlineDynamicImports: true,
            entryFileNames: "launcher-state-scenario.mjs",
          },
        },
      },
    });

    await import(
      `${pathToFileURL(path.join(outDir, "launcher-state-scenario.mjs"))}?cacheBust=${Date.now()}`
    );
    assert.ok(globalThis.__launcherStateScenario instanceof Promise);
    await globalThis.__launcherStateScenario;
    assert.ok(true, "frontend-to-backend scenario completed");
  } finally {
    if (previousStateShim === undefined) delete globalThis.__testState;
    else globalThis.__testState = previousStateShim;
    if (previousWindow === undefined) delete globalThis.window;
    else globalThis.window = previousWindow;
    if (previousFixtureRoot === undefined)
      delete process.env.WUWAID_FIXTURE_ROOT;
    else process.env.WUWAID_FIXTURE_ROOT = previousFixtureRoot;
    delete globalThis.__launcherStateScenario;
    await rm(outDir, { recursive: true, force: true });
  }
});
