import { appState } from "../../src/lib/launcherState.svelte";
import { themeRuntime } from "../../src/lib/themeRuntime.svelte";
import { deliver, registeredEvents } from "./mediaThemeOrderFixture.js";

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const MEDIA_READY = {
  bgmUrl: "media://localhost/bgm.mp3",
  videoUrl: "media://localhost/bg-video.mp4",
};

function signedTheme(id: string) {
  return {
    id,
    name: `Tema ${id}`,
    keyId: "fixture-key",
    tokens: { "--particle-gold-rgb": "231, 211, 148" },
    css: "",
    backgroundFile: "bg.jpg",
    status: "signed",
  };
}

/**
 * A launcher that has painted the bundled theme, followed by the `checking`
 * status the backend sends at the start of every sync — which is what clears
 * the previous run's media URLs.
 */
async function beginSync(order: string) {
  await appState.init();
  assert(
    ["onMediaReady", "onMediaStatus", "onThemeReady"].every((name) =>
      registeredEvents().includes(name),
    ),
    `${order}: the media and theme listeners were not registered: ` +
      JSON.stringify(registeredEvents()),
  );
  assert(!themeRuntime.isThemed, `${order}: a theme was painted before any event`);
  assert(
    appState.remoteTheme === null && appState.themeStatus === "general",
    `${order}: the launcher did not start on the general theme`,
  );
  deliver("onMediaStatus", { status: "checking", message: "Memeriksa aset media..." });
  assert(
    appState.mediaStatus === "checking" &&
      appState.bgmUrl === "" &&
      appState.videoUrl === "",
    `${order}: a new media sync did not clear the previous media state`,
  );
}

function assertMediaReady(order: string) {
  assert(
    appState.mediaStatus === "ready",
    `${order}: media status was ${JSON.stringify(appState.mediaStatus)}`,
  );
  assert(
    appState.bgmUrl === MEDIA_READY.bgmUrl &&
      appState.videoUrl === MEDIA_READY.videoUrl,
    `${order}: media URLs were not committed`,
  );
}

function assertTheme(order: string, id: string) {
  assert(
    appState.themeStatus === "signed" && appState.remoteTheme?.id === id,
    `${order}: theme was not applied (status ${appState.themeStatus}, id ${appState.remoteTheme?.id})`,
  );
  assert(
    themeRuntime.isThemed && themeRuntime.theme?.id === id,
    `${order}: the theme runtime did not paint ${id}`,
  );
  assert(
    themeRuntime.backgroundUrl() === "media://localhost/bg.jpg",
    `${order}: the theme background was ${themeRuntime.backgroundUrl()}, ` +
      "not the verified-cache media origin",
  );
}

/**
 * The backend collects the manifest signature after the media sync, so
 * `onThemeReady` can now arrive after `onMediaReady`. Each order starts from a
 * launcher that has seen neither event, and neither may depend on the other
 * having come first.
 */
export async function runMediaThemeOrderScenario() {
  // The order the launcher used to guarantee: theme, then media.
  await beginSync("theme-first");
  deliver("onThemeReady", signedTheme("theme-before-media"));
  assertTheme("theme-first", "theme-before-media");
  deliver("onMediaReady", MEDIA_READY);
  deliver("onMediaStatus", { status: "ready", message: "" });
  assertMediaReady("theme-first");
  assertTheme("theme-first", "theme-before-media");
  appState.dispose();

  // The order a stalled signature produces: media first, theme last.
  await beginSync("media-first");
  deliver("onMediaReady", MEDIA_READY);
  deliver("onMediaStatus", { status: "ready", message: "" });
  assertMediaReady("media-first");
  assert(
    !themeRuntime.isThemed && appState.remoteTheme === null,
    "media-first: a theme was applied before any theme event arrived",
  );
  deliver("onThemeReady", signedTheme("theme-after-media"));
  assertTheme("media-first", "theme-after-media");
  assertMediaReady("media-first");
  appState.dispose();
}

// The compiled scenario exposes its promise here so the node:test wrapper can
// await the async body after the module has been imported.
const scope = globalThis as typeof globalThis & {
  __mediaThemeOrderScenario?: Promise<void>;
};
scope.__mediaThemeOrderScenario = runMediaThemeOrderScenario();
