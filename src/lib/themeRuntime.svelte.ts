const FALLBACK_PARTICLE_GOLD: readonly [number, number, number] = [212, 176, 108];
const FALLBACK_PARTICLE_CYAN: readonly [number, number, number] = [80, 195, 220];
const DEFAULT_BACKGROUND = "/images/bg-default.jpg";

/**
 * Runtime owner of the active visual theme.
 *
 * A theme is data, not code: an ordered map of CSS custom properties plus an
 * optional verified stylesheet fragment. Applying it is a pure DOM write, so a
 * new game version can ship a new look without a launcher release.
 *
 * Security: the payload is Ed25519-verified in Rust before it ever reaches
 * here. The checks below are defence in depth against a tampered localStorage
 * copy, which is the one input this module reads without verification.
 */

/**
 * Mirrors the backend's media URL builder. Theme assets are served from the
 * verified local cache over the custom protocol, never fetched from a network
 * origin by the webview.
 */
export function mediaAssetUrl(fileName: string): string {
  const isWindows = navigator.userAgent.includes("Windows");
  return isWindows
    ? `http://media.localhost/${fileName}`
    : `media://localhost/${fileName}`;
}

export const THEME_PREFERENCE_KEY = "wuwaid.themePreference";
const BOOT_TOKENS_KEY = "__wuwaidBootTokens";

declare global {
  interface Window {
    __wuwaidBootTokens?: string[];
  }
}

export interface ThemeDescriptor {
  id: string;
  name: string;
}

export interface ActiveTheme {
  id: string;
  name: string;
  tokens: Record<string, string>;
  css: string;
  backgroundUrl: string;
}

export const THEME_STYLE_ELEMENT_ID = "wuwaid-active-theme";
export const THEME_BODY_CLASS_PREFIX = "theme-";
export const THEME_CACHE_KEY = "wuwaid.activeTheme";

const MAX_THEME_TOKENS = 240;
const MAX_TOKEN_VALUE_LENGTH = 160;
const MAX_FRAGMENT_LENGTH = 128 * 1024;
const TOKEN_NAME = /^--[a-z][a-z0-9-]{0,47}$/;
// Mirrors the Rust validator in engine/theme.rs. Keeping the two in step means
// a payload that survives one is never silently reinterpreted by the other.
const FORBIDDEN_VALUE = /[;{}<>\\]|\/\*|@import|expression\(|javascript:|url\(/i;

/** Normalises `231, 211, 148` and `231 211 148` to the canonical space form. */
export function normalizeTokenValue(name: string, value: string): string {
  const trimmed = value.trim();
  if (!name.endsWith("-rgb") || !FORBIDDEN_VALUE.test(trimmed)) return trimmed;
  return trimmed.replace(/\s*,\s*/g, " ");
}

export function sanitizeTokens(raw: Record<string, string>): Record<string, string> {
  const out: Record<string, string> = {};
  let count = 0;
  for (const [name, value] of Object.entries(raw)) {
    if (count >= MAX_THEME_TOKENS) break;
    if (!TOKEN_NAME.test(name) || typeof value !== "string") continue;
    const normalized = normalizeTokenValue(name, value);
    if (
      normalized.length === 0 ||
      normalized.length > MAX_TOKEN_VALUE_LENGTH ||
      FORBIDDEN_VALUE.test(normalized)
    ) {
      continue;
    }
    out[name] = normalized;
    count += 1;
  }
  return out;
}

function readTriplet(
  name: string,
  fallback: readonly [number, number, number],
): [number, number, number] {
  if (typeof document === "undefined") return [...fallback] as [number, number, number];
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  if (!raw) return [...fallback] as [number, number, number];
  const parts = raw
    .replace(/,/g, " ")
    .split(/\s+/)
    .map((part) => Number.parseInt(part, 10));
  if (parts.length !== 3 || parts.some((part) => !Number.isInteger(part))) {
    return [...fallback] as [number, number, number];
  }
  return [parts[0], parts[1], parts[2]];
}

class ThemeRuntime {
  /** Plain URL for the video poster. The CSS token carries `url("…")`, which
   *  the `poster` attribute cannot consume. */
  backgroundUrl(): string {
    return this.background ?? DEFAULT_BACKGROUND;
  }

  /** Descriptor of the applied theme, or null while the bundled theme is active. */
  theme = $state<ActiveTheme | null>(null);
  /** Themes the signed manifest advertised, for the settings selector. */
  available = $state<ThemeDescriptor[]>([]);
  /** Bumped on every apply/clear so canvas painters can repaint. */
  revision = $state(0);

  private appliedTokens: string[] = [];
  private fragment = "";
  private background = $state<string | null>(null);

  get isThemed(): boolean {
    return this.theme !== null;
  }

  particlePalette(): {
    gold: [number, number, number];
    cyan: [number, number, number];
  } {
    return {
      gold: readTriplet("--particle-gold-rgb", FALLBACK_PARTICLE_GOLD),
      cyan: readTriplet("--particle-cyan-rgb", FALLBACK_PARTICLE_CYAN),
    };
  }

  /** Mirrors the preference where the pre-paint bootstrap can read it. */
  setPreference(preference: string): void {
    try {
      if (preference === "general") {
        localStorage.setItem(THEME_PREFERENCE_KEY, "general");
      } else {
        localStorage.removeItem(THEME_PREFERENCE_KEY);
      }
    } catch {
      // A blocked store only costs the pre-paint repaint, not theming.
    }
  }

  apply(theme: ActiveTheme | null): void {
    this.clearDom();
    if (theme) {
      this.applyDom(theme);
      this.background = theme.backgroundUrl || DEFAULT_BACKGROUND;
      this.theme = theme;
      this.cacheTokens(theme);
    } else {
      this.background = null;
      this.theme = null;
      this.clearCache();
    }
    this.revision += 1;
  }

  private applyDom(theme: ActiveTheme): void {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    for (const [name, value] of Object.entries(sanitizeTokens(theme.tokens))) {
      root.style.setProperty(name, value);
      this.appliedTokens.push(name);
    }
    if (theme.backgroundUrl) {
      root.style.setProperty("--bg-image", `url("${theme.backgroundUrl}")`);
      this.appliedTokens.push("--bg-image");
    }
    // The class marks "a theme is active", not "a fragment applied": a
    // tokens-only theme still needs it, and a fragment rejected for size must
    // not leave the theme unmarked while its tokens are in effect.
    document.body.classList.add(
      `${THEME_BODY_CLASS_PREFIX}${theme.id.replace(/[^a-z0-9-]/gi, "-")}`,
    );
    const fragment = theme.css.trim();
    if (!fragment || fragment.length > MAX_FRAGMENT_LENGTH) return;
    this.fragment = fragment;
    const style = document.createElement("style");
    style.id = THEME_STYLE_ELEMENT_ID;
    style.textContent = fragment;
    document.head.appendChild(style);
  }

  private clearDom(): void {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    for (const name of this.appliedTokens) root.style.removeProperty(name);
    this.appliedTokens = [];
    // The pre-paint bootstrap writes inline tokens the runtime never applied
    // itself. Without removing them, switching back to the general theme would
    // leave the boot-painted theme in place until the next restart.
    for (const name of window[BOOT_TOKENS_KEY] ?? []) {
      root.style.removeProperty(name);
    }
    window[BOOT_TOKENS_KEY] = [];
    for (const className of [...root.classList]) {
      if (className.startsWith(THEME_BODY_CLASS_PREFIX)) {
        root.classList.remove(className);
      }
    }
    document.getElementById(THEME_STYLE_ELEMENT_ID)?.remove();
    for (const className of [...document.body.classList]) {
      if (className.startsWith(THEME_BODY_CLASS_PREFIX)) {
        document.body.classList.remove(className);
      }
    }
    this.fragment = "";
  }

  private cacheTokens(theme: ActiveTheme): void {
    try {
      // Tokens only. The stylesheet fragment is re-verified by Rust on every
      // launch, so no unverified copy of it is ever injected from storage.
      localStorage.setItem(
        THEME_CACHE_KEY,
        JSON.stringify({ id: theme.id, tokens: sanitizeTokens(theme.tokens) }),
      );
    } catch {
      // Full or blocked storage must not break theming.
    }
  }

  private clearCache(): void {
    try {
      localStorage.removeItem(THEME_CACHE_KEY);
    } catch {
      // Ignored for the same reason as above.
    }
  }
}

export const themeRuntime = new ThemeRuntime();
