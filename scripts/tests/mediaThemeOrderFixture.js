// Transport stand-in for the media/theme ordering scenario. It replaces
// @tauri-apps/api/core and @tauri-apps/api/event in the Vite build, so the
// production bridge.ts and launcherState.svelte.ts stay the modules under
// test while the test decides the order the backend events arrive in.
const listeners = new Map();
export const delivered = [];

const settings = {
  gamePath: "",
  installMethod: "resource_mount",
  dx11: false,
  uidMode: "default",
  uidText: "",
  bgmVolume: 0.35,
  bgmEnabled: true,
  themePreference: "auto",
};

const generalTheme = {
  id: "general",
  name: "Tema Umum",
  keyId: "",
  tokens: {},
  css: "",
  backgroundFile: "",
  status: "general",
};

export async function invoke(command, args = {}) {
  switch (command) {
    case "get_app_version":
      return "2.10.0";
    case "get_vh_version":
      return "3.6.1-id.2";
    case "load_settings":
      return { settings, repaired: false, diagnostics: [] };
    case "is_game_running":
      return false;
    case "get_active_theme":
      return generalTheme;
    default:
      return undefined;
  }
}

export async function listen(event, handler) {
  const handlers = listeners.get(event) ?? new Set();
  handlers.add(handler);
  listeners.set(event, handlers);
  return () => handlers.delete(handler);
}

/** Delivers one backend event to every registered listener, synchronously. */
export function deliver(event, payload) {
  for (const handler of listeners.get(event) ?? []) {
    handler({ event, payload });
  }
}

export function registeredEvents() {
  return [...listeners.keys()].sort();
}
