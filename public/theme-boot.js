/*
 * Pre-paint theme bootstrap.
 *
 * Applies the last verified theme's tokens to <html> before Svelte mounts, so
 * opening the launcher never flashes the bundled general theme before the
 * network sync lands. Tokens only: the stylesheet fragment is re-verified by
 * Rust on every launch and is therefore never injected from storage.
 *
 * Everything here is best-effort. Any failure must leave the bundled theme in
 * place rather than block the launcher from opening.
 */
(function bootstrapTheme() {
  var STORAGE_KEY = "wuwaid.activeTheme";
  var PREFERENCE_KEY = "wuwaid.themePreference";
  var BOOT_TOKENS_KEY = "__wuwaidBootTokens";

  // Only the palette layer plus a few named extras is accepted, so a token the
  // launcher never reads cannot be smuggled in through storage. Residual risk:
  // an attacker with local write access can still set a palette token, but that
  // only changes colour — layout, text, and script stay out of reach from here.
  var PALETTE_TOKEN = /^--[a-z0-9-]+-rgb$/;
  var ALLOWED_EXTRAS = ["--mist-grad", "--bg-deep", "--bg-panel"];

  // Same rejection set as engine/theme.rs and themeRuntime.svelte.ts, so a
  // value that one layer refuses is never quietly accepted by another.
  var FORBIDDEN = /[;{}<>\\]|\/\*|@import|expression\(|javascript:|url\(/i;
  var MAX_TOKENS = 240;
  var MAX_VALUE_LENGTH = 160;

  try {
    // An explicit "general" choice must win here too, otherwise the launcher
    // would paint a theme the user just switched off.
    if (window.localStorage.getItem(PREFERENCE_KEY) === "general") return;

    var stored = window.localStorage.getItem(STORAGE_KEY);
    if (!stored) return;
    var parsed = JSON.parse(stored);
    if (!parsed || typeof parsed !== "object" || !parsed.tokens) return;
    if (typeof parsed.id !== "string" || !parsed.id) return;

    var root = document.documentElement;
    var applied = [];
    for (var name in parsed.tokens) {
      if (applied.length >= MAX_TOKENS) break;
      if (!Object.prototype.hasOwnProperty.call(parsed.tokens, name)) continue;
      if (name.length > 48) continue;
      if (!PALETTE_TOKEN.test(name) && ALLOWED_EXTRAS.indexOf(name) === -1) continue;
      var raw = parsed.tokens[name];
      if (typeof raw !== "string") continue;
      var value = raw.trim();
      if (!value || value.length > MAX_VALUE_LENGTH || FORBIDDEN.test(value)) continue;
      if (name.slice(-4) === "-rgb") value = value.replace(/\s*,\s*/g, " ");
      root.style.setProperty(name, value);
      applied.push(name);
    }
    root.classList.add("theme-" + parsed.id.replace(/[^a-z0-9-]/gi, "-"));
    // Hand the applied names to the runtime. Inline styles are invisible to
    // its own bookkeeping, so without this, switching back to the general theme
    // would leave the boot-painted tokens behind until the next restart.
    window[BOOT_TOKENS_KEY] = applied;
  } catch (error) {
    // A blocked or corrupted store must never stop the launcher from opening.
  }
})();
