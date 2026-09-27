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
const transportId = normalizePath(
  path.resolve(repoRoot, "scripts/tests/mediaThemeOrderFixture.js"),
);
const scenarioPath = path.resolve(
  repoRoot,
  "scripts/tests/media-theme-ordering.scenario.ts",
);

test("launcher state survives onThemeReady before and after onMediaReady", async () => {
  const outDir = await mkdtemp(path.join(os.tmpdir(), "wuwaid-media-order-"));
  const previousStateShim = globalThis.__testState;
  const previousWindow = globalThis.window;
  const previousTauriInternals = globalThis.__TAURI_INTERNALS__;
  const previousLocalStorage = globalThis.localStorage;
  const store = new Map();
  globalThis.__testState = (value) => value;
  globalThis.window = globalThis;
  globalThis.__TAURI_INTERNALS__ = {};
  globalThis.localStorage = {
    getItem: (key) => store.get(key) ?? null,
    setItem: (key, value) => store.set(key, String(value)),
    removeItem: (key) => store.delete(key),
  };

  try {
    await build({
      root: repoRoot,
      logLevel: "error",
      plugins: [
        {
          name: "tauri-media-transport",
          enforce: "pre",
          resolveId(source) {
            // Production bridge.ts talks to @tauri-apps/api; the scenario
            // decides when each backend event is delivered.
            if (
              source === "@tauri-apps/api/core" ||
              source === "@tauri-apps/api/event"
            ) {
              return transportId;
            }
            return null;
          },
        },
      ],
      define: {
        $state: "globalThis.__testState",
      },
      build: {
        target: "esnext",
        outDir,
        emptyOutDir: true,
        minify: false,
        rollupOptions: {
          input: scenarioPath,
          output: {
            format: "es",
            inlineDynamicImports: true,
            entryFileNames: "media-theme-order.mjs",
          },
        },
      },
    });

    await import(
      `${pathToFileURL(path.join(outDir, "media-theme-order.mjs"))}?cacheBust=${Date.now()}`
    );
    const scenario = globalThis.__mediaThemeOrderScenario;
    assert.ok(scenario instanceof Promise, "scenario did not start");
    await scenario;
  } finally {
    if (previousStateShim === undefined) delete globalThis.__testState;
    else globalThis.__testState = previousStateShim;
    if (previousWindow === undefined) delete globalThis.window;
    else globalThis.window = previousWindow;
    if (previousTauriInternals === undefined)
      delete globalThis.__TAURI_INTERNALS__;
    else globalThis.__TAURI_INTERNALS__ = previousTauriInternals;
    if (previousLocalStorage === undefined) delete globalThis.localStorage;
    else globalThis.localStorage = previousLocalStorage;
    delete globalThis.__mediaThemeOrderScenario;
    await rm(outDir, { recursive: true, force: true });
  }
});
