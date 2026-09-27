# Dynamic Theming

The launcher ships a **general theme** and adopts a **published theme** for the
current game version. A new look is a data change in the asset manifest, not a
launcher release.

## The two themes

There are always two:

- **General theme** — compiled into the launcher, neutral blue-slate, and the
  fallback whenever nothing verified is cached. It refers to no Wuthering
  Waves version's art direction, so it stays correct across 3.6, 3.7, and
  anything after. Users can pin it from Settings.
- **The published theme** — the current game version's look, fetched and
  verified from the asset manifest. `Web/Theme/wuwa-3-6/` ships Wuthering
  Waves 3.6 in mist-aqua, jade, and lantern. Publishing a new version means
  adding a directory and flipping `"active"`, nothing more.

Both carry their own background, so withdrawing the published theme returns a
neutral backdrop rather than the withdrawn look's artwork.

Everything the launcher fetches lives in this repository, under `Web/`:

```
Web/assets.json          manifest: media assets + the theme block
Web/assets.json.sig      detached Ed25519 signature over the manifest bytes
Web/Audio/bgm.mp3        git-lfs
Web/Video/bg-video.mp4   git-lfs
Web/Theme/<id>/bg.jpg    background image
Web/Theme/<id>/theme.css optional stylesheet fragment
```

Everything is served from `raw.githubusercontent.com/…` as ordinary git objects.
git-lfs was tried and rejected: this is a public fork, and GitHub refuses LFS
objects above roughly 5 MB there, while an 11 MB ordinary blob is accepted. The
16 MB of media therefore lives in this repository's history, which is well under
GitHub's 100 MB per-file limit and invisible to anyone who downloads a release.

## Trust model

The launcher verifies `assets.json.sig` against the Ed25519 public keys
compiled into the binary (`TRUSTED_SIGNING_KEYS` in
`src-tauri/src/engine/theme.rs`). The theme block is honoured **only** when:

- the signature verifies against a trusted key, and
- the signature covers the exact manifest bytes that were fetched.

Anything else — no signature, unknown key, bad signature, oversized payload,
hash mismatch, network failure — leaves whatever theme this launcher last
verified in place, and says so in Settings. The bundled general theme is what
ships when nothing verified is cached. Media downloads keep their existing
sha256-from-manifest trust model and are unaffected by any of this, so an
unsigned manifest costs you theming, not functionality.

### Withdrawing a theme

Set `"active": false` and the launcher drops the cached theme and returns to the
general theme on its next launch. That is the only kill switch a shipped theme
has — there is no other way to pull a look back without releasing the
launcher, so use it.

`active` is tri-state, and the distinction is deliberate. `"active": true`
publishes. `"active": false` withdraws. Omitting it means the manifest never
said which theme is live, which keeps the current theme — so a draft that
forgets the field fails to appear rather than silently disappearing from every
install. A missing `theme` block behaves the same way.

Key removal is enforced the same way. The key id that signed a theme is
recorded in the cache, and a cache whose key is no longer in the keyring is
discarded on read. Removing a key therefore revokes every theme it signed, on
launchers that never sync again. That does require a launcher release, since
the keyring is compiled in.

Before the first public release, replace the old key directly: no released
launcher trusts it, so keep only the new public key in `TRUSTED_SIGNING_KEYS`.
After a key has shipped, rotate in stages: add the incoming public key to the
second slot, release a launcher that trusts both, then sign with the new key.
Remove the old key in a later launcher release; removing a trusted key revokes
the themes it signed for launchers that do not sync again.

Generate each key at a fresh path and give it a unique id. The command refuses
to overwrite existing key files and records the id beside the key:

```bash
node scripts/sign-manifest.mjs --generate \
  --key scripts/keys/web-manifest-2026-03.key.pem \
  --key-id wuwa-web-2026-03
node scripts/sign-manifest.mjs --in Web/assets.json \
  --key scripts/keys/web-manifest-2026-03.key.pem \
  --key-id wuwa-web-2026-03
```

Add the printed public key to `TRUSTED_SIGNING_KEYS` before shipping a launcher
that should accept the new signature. Never commit the private key.

### A manifest with no theme block

Silence is not a withdrawal. A manifest that omits the `theme` block entirely —
including the manifests published before this feature existed — leaves the last
verified theme in place, labelled as such in Settings. A theme block withdraws
only when it says `"active": false`; one that never set `active` is a draft
that has not been published, and behaves the same way. Without that
distinction, publishing one manifest that happened to lack the block would
silently reset every launcher's look.

### Cache resets

`reset_webview_cache` clears the whole cache directory, which includes the
verified theme. It re-syncs immediately, so a reachable manifest restores the
theme within seconds; offline, the launcher falls back to the general theme
until the next sync. That is the expected behaviour of a cache reset, but it is
worth knowing before reaching for it while troubleshooting something else.

## Authoring a theme

1. Create `Web/Theme/<id>/` under the launcher repository. `<id>` is lowercase
   letters, digits, and dashes.
2. Write the `theme` block in `Web/assets.json`:

```jsonc
{
  "assets": [ /* unchanged: bgm.mp3, bg-video.mp4 */ ],
  "theme": {
    "id": "wuwa-3-6",
    "name": "Wuthering Waves 3.6",
    "active": true,
    "tokens": {
      "--gold-rgb": "231 211 148",
      "--ink-rgb": "7 26 30"
    },
    "themeCss": {
      "name": "theme.css",
      "url": "https://raw.githubusercontent.com/TitoTFP/WuwaIDLauncher/refs/heads/main/Web/Theme/wuwa-3-6/theme.css",
      "sha256": "<sha256 of theme.css>"
    },
    "background": {
      "name": "bg.jpg",
      "url": "https://raw.githubusercontent.com/TitoTFP/WuwaIDLauncher/refs/heads/main/Web/Theme/wuwa-3-6/bg.jpg",
      "sha256": "<sha256 of bg.jpg>"
    }
  }
}
```

3. Sign the manifest:

```bash
node scripts/sign-manifest.mjs --in Web/assets.json
```

`assets.json.sig` is written next to it. Commit both.

To publish, flip `"active": false` to `true`. That single line is what makes
every launcher pick the new look on its next launch.

## The token contract

Every colour in the launcher resolves through a `--*-rgb` triplet defined in
`src/styles/styles-base.css`, and alpha tints are written as
`rgb(var(--ink-rgb) / 0.84)`. Overriding a triplet therefore moves every shade
derived from it, across every stylesheet and every Svelte component.

`sign-manifest.mjs` normalises `\r\n` to `\n` before signing, because the
signature covers the exact bytes GitHub serves. Add `*.json text eol=lf` to
`.gitattributes` in the asset repository so a Windows checkout cannot commit
CRLF.

A theme only needs to override the triplets it actually wants to change; the
rest keep the general theme's values. The full list, with the values that ship with
the general theme, is in the palette layer of `styles-base.css`:

| Group | Tokens |
| --- | --- |
| Core palette | `--ink-rgb` `--aqua-rgb` `--cyan-rgb` `--jade-rgb` `--text-rgb` `--gold-rgb` `--line-rgb` `--slate-rgb` |
| Gradient stops | `--grad-1-rgb` `--grad-2-rgb` `--grad-3-rgb` |
| Ink and surface darks | `--ink-soft-rgb` `--ink-mid-rgb` `--ink-mid-2-rgb` `--ink-deep-rgb` `--ink-deepest-rgb` `--panel-solid-rgb` `--jade-deep-rgb` `--shadow-rgb` `--ink-black-rgb` `--ink-black-2-rgb` `--black-rgb` |
| Legacy surfaces | `--navy-rgb` `--navy-scrim-rgb` `--navy-deep-rgb` |
| Accent shades | `--gold-bright-rgb` `--gold-raw-rgb` `--gold-deep-rgb` |
| Text and near-whites | `--text-soft-rgb` `--text-mute-rgb` `--white-rgb` `--near-white-rgb` `--near-white-2-rgb` `--near-white-3-rgb` `--warm-white-rgb` `--silver-rgb` |
| State colours | `--red-rgb` `--red-strong-rgb` `--red-soft-rgb` `--red-hot-rgb` `--red-deep-rgb` `--green-rgb` `--green-tint-rgb` `--green-tint-2-rgb` `--blue-tint-rgb` `--blue-tint-2-rgb` `--pink-rgb` `--pink-tint-rgb` `--peach-rgb` |
| Canvas particles | `--particle-gold-rgb` `--particle-cyan-rgb` |

Rules the launcher enforces on a theme:

- token names match `--[a-z][a-z0-9-]*`, at most 240 per theme;
- values are at most 160 characters and may not contain `; { } < > \`, a
  comment opener, `url(`, `@import`, `expression(`, or `javascript:`;
- triplet values may be written `231 211 148` or `231, 211, 148`; commas are
  normalised for `-rgb` tokens.

The semantic layer (`--fg`, `--fg-accent`, `--accent`, `--accent-2`, `--success`,
`--panel`, `--bg-deep`, `--line`, `--line-strong`, `--grad`) names roles rather
than hues, and can be overridden directly; overriding the palette is what
repaints the whole launcher. The pre-paint bootstrap honours exactly these
names plus the triplets, so a theme does not flash the general theme on start.

### The background image

The background comes from the theme's `background` asset entry, not from a
token. `--bg-image` cannot be set through `tokens`: a value containing `url(`
is rejected, and that would take the whole theme down with it.

`bg.jpg` is the **poster and fallback**, not the primary backdrop. The synced
`bg-video.mp4` paints over it, so the image is what you see while the video
loads, when the video is blocked, and when media is unavailable entirely. A
theme that ships both should expect its image to be hidden behind the video
during normal play.

The launcher only activates a theme whose `bg.jpg` downloaded and passed its
sha256 check, and it drops a cached theme whose image has gone missing, so a
failed image cannot leave a blank backdrop.

## The stylesheet fragment

`theme.css` is the escape hatch for anything the tokens cannot express. It is
verified by sha256 like every other asset and rejected if it contains
`@import`, `expression(`, `javascript:`, or `</style`. It must also be 128 KiB
or smaller; that ceiling is checked once the file has downloaded rather than
mid-transfer, so an oversized fragment costs a download before it is refused.

Write it scoped to the theme so it composes predictably:

```css
body.theme-wuwa-3-6 .start-btn {
    border-width: 2px !important;
}
```

Svelte compiles component styles to `.card.svelte-hash` (specificity 0,2,0),
so a plain `.card` rule in the fragment will lose. Use `!important`, or drive
the change through tokens, which have no specificity contest.

## Startup behaviour

1. `public/theme-boot.js` runs before Svelte mounts and re-applies the last
   verified theme's **tokens** from `localStorage`, so the launcher never
   flashes the general theme on the way to a themed one.
2. `get_active_theme` returns the last verified theme from the cache, and the
   webview applies the full payload (tokens, fragment, background).
3. `check_and_sync_media` fetches the manifest, verifies the signature, and
   activates the new theme if it differs.

The fragment is never restored from storage — it is re-verified on every
launch. A tampered `localStorage` entry cannot inject a declaration: token
names and values are validated before anything is written to the DOM.

Users can pin the general theme in **Settings → Tema Tampilan**; that choice
always wins over the remote theme.

## Local testing

```bash
node scripts/sign-manifest.mjs --generate      # once, writes scripts/keys/ (git-ignored)
WUWAID_ASSETS_URL=http://127.0.0.1:8080/assets.json npm run tauri dev
```

`WUWAID_ASSETS_URL` points the launcher at a local manifest. Serve
`assets.json`, `assets.json.sig`, and the theme directory over loopback; the
media URL validator already permits `http://localhost` for fixtures. The
manifest itself is still signature-checked regardless of host — loopback only
exempts the asset URLs it points at.

The private key in `scripts/keys/` is local-only and never committed. Its
public half must be present in `TRUSTED_SIGNING_KEYS` for the launcher to
accept what it signs.
