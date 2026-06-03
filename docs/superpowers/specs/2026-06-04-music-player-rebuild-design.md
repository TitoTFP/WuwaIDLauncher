# Music Player Rebuild — Design Spec

**Date:** 2026-06-04
**Status:** Approved (in design phase)
**Scope:** Complete rebuild of the in-launcher music player (HTML + CSS + JS)

---

## 1. Background & Motivation

The current music player has accumulated drift between its HTML, CSS, and JS:

- **HTML (`index.html:427-443`)** contains only the play button, mute button, slider, fill, and label.
- **CSS (`styles-base.css:375-552`)** defines elaborate styles for elements that **do not exist in the HTML**: `.ap-art`, `.ap-viz` (5 animated bars), `.ap-body`, `.ap-meta*`, `.ap-progress`, `.ap-time`, `.ap-track`, `.ap-controls`, `.ap-shuffle/prev/next/repeat`.
- **JS (`script-misc.js:310-475`)** references those missing elements (`apTrack`, `apFill`, `apCur`, `apDur`, `apShuffle`, `apPrev`, `apNext`, `apRepeat`) — the optional chaining makes them no-ops, but the dead references bloat the file.

The user has also been struggling with the volume slider (interaction / drag / click). A complete rebuild that matches the launcher's visual language is the cleanest path forward.

## 2. Goals

- Single source of truth: HTML elements exactly match CSS rules exactly match JS references.
- All controls work reliably, including the volume slider (click, drag, touch).
- Visual style is consistent with the launcher's sci-fi/cyberpunk aesthetic (cyan/gold/red, glassmorphism, `clip-path` corner cuts, JetBrains Mono).
- Minimalis scope: play, mute, volume slider, value label, status header.
- Public contract with the C# bridge stays intact.

## 3. Non-Goals

- No progress bar, no track metadata, no album art, no prev/next/shuffle/repeat.
- No CORS workaround for cross-origin audio (existing limitation accepted).
- No new toast/notification system for audio errors.
- No retry logic for failed audio loads.

## 4. Files Affected

### Removed (cleanly)

| File | Lines | Content |
|---|---|---|
| `index.html` | 427-443 | Old `<div class="audio-player">` block |
| `index.html` | 503-505 | Old `<audio id="bgMusic">` |
| `script-misc.js` | 310-475 | Old `initAudioPlayer()` (158 lines) |
| `script-misc.js` | 477-482 | Old `toggleBgm()` — verified no callers via grep |
| `styles-base.css` | 375-552 | All `.ap-art`, `.ap-viz`, `.ap-body`, `.ap-meta*`, `.ap-progress`, `.ap-time`, `.ap-track`, `.ap-controls`, `.ap-shuffle/prev/next/repeat` rules; old `.ap-vol*` and `.ap-btn*` |

### Added (new implementation)

| File | Approx lines | Content |
|---|---|---|
| `index.html` | ~30 | New `<div class="ap" id="ap">` block + `<audio id="apAudio">` |
| `script-misc.js` | ~110 | New `initAudioPlayer()` |
| `styles-base.css` | ~150 | New `.ap`, `.ap__*` rules + `apPulse` keyframe |

### Untouched

- C# bridge contract: `window.onMediaReady(bgmUrl, videoUrl)` and `window.apSetAudioSource(url)`.
- `script-core.js:18` call to `initAudioPlayer()`.
- `script-home.js:345-362` `onMediaReady` handler.
- All other launcher UI.

### Post-implementation cleanup

- Delete `Resources/Web/bgm.mp3` (8MB test file, should not be shipped).

## 5. HTML Structure

The new block replaces both the old `.audio-player` div and the `<audio>` element.

```html
<div class="ap" id="ap">
  <header class="ap__head">
    <span class="ap__tag">BGM</span>
    <span class="ap__dot" id="apDot"></span>
    <span class="ap__status" id="apStatus">IDLE</span>
  </header>
  <div class="ap__body">
    <button class="ap__play" id="apPlay" title="Putar / Jeda" aria-label="Putar / Jeda">
      <svg class="ap__ico ap__ico--play"  viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M8 5v14l11-7z"/></svg>
      <svg class="ap__ico ap__ico--pause" viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path fill="currentColor" d="M6 19h4V5H6v14zm8-14v14h4V5h-4z"/></svg>
    </button>
    <div class="ap__track" id="apTrack">
      <input type="range" class="ap__range" id="apRange" min="0" max="100" value="35" step="1" aria-label="Volume">
      <div class="ap__fill" id="apFill" style="width:35%"></div>
    </div>
    <button class="ap__mute" id="apMute" title="Bisukan / Aktifkan" aria-label="Bisukan / Aktifkan">
      <svg class="ap__ico" id="apMuteIcon" viewBox="0 0 24 24" width="12" height="12" aria-hidden="true"><path fill="currentColor" d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02z"/></svg>
    </button>
    <span class="ap__num" id="apNum">35</span>
  </div>
</div>
<audio id="apAudio" loop preload="auto"></audio>
```

**Class convention:** flat BEM with `ap__*` (no nested modifiers). The play/pause icon swap is CSS-driven via `.ap--playing` class — no JS `style.display` manipulation.

**Element ID map (old → new):**

| Old | New |
|---|---|
| `bgMusic` (audio) | `apAudio` |
| `audioPlayer` | `ap` |
| `apPlay` | `apPlay` (same) |
| `apVolBtn` | `apMute` |
| `apVolSlider` | `apRange` |
| `apVolFill` | `apFill` |
| `apVolLabel` | `apNum` |
| (none) | `apStatus`, `apDot` |
| `apVolIcon` (inside btn) | `apMuteIcon` (inside btn) |

## 6. CSS Design

All new rules use existing design tokens (`var(--*)`). Width: 240px. Position: fixed bottom-left. Glassmorphism with cyan border glow and `clip-path` corner cut (top-right + bottom-left).

### 6.1 Container

```css
.ap {
    position: fixed;
    left: var(--edge-gap);
    bottom: var(--edge-gap);
    width: 240px;
    z-index: 100;
    padding: 10px 12px;
    background: var(--bg-panel);
    backdrop-filter: blur(20px) saturate(160%);
    -webkit-backdrop-filter: blur(20px) saturate(160%);
    border: 1.5px solid var(--glass-border);
    clip-path: polygon(0 0, calc(100% - 12px) 0, 100% 12px, 100% 100%, 12px 100%, 0 calc(100% - 12px));
    box-shadow:
        0 4px 28px rgba(0, 0, 0, 0.60),
        0 0 20px rgba(0, 240, 255, 0.10),
        inset 0 1px 0 rgba(255, 255, 255, 0.04);
    animation: fadeUp 500ms var(--ease) 600ms both;
    user-select: none;
}
```

### 6.2 Header (status row)

```css
.ap__head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 9px;
    padding-bottom: 8px;
    border-bottom: 1px solid rgba(0, 240, 255, 0.12);
}

.ap__tag {
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 1.5px;
    color: var(--cyan-accent);
    padding: 2px 6px;
    border: 1px solid var(--glass-border);
    background: rgba(0, 240, 255, 0.06);
}

.ap__dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-3);
    flex-shrink: 0;
    margin-left: auto;
    transition: background var(--dur);
}
.ap--playing .ap__dot {
    background: var(--green);
    animation: apPulse 1.4s ease-in-out infinite;
}
.ap--muted .ap__dot {
    background: var(--red);
    animation: none;
}

.ap__status {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 1.5px;
    color: var(--text-3);
}
```

### 6.3 Body (controls row)

```css
.ap__body {
    display: flex;
    align-items: center;
    gap: 8px;
}

.ap__play {
    flex-shrink: 0;
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-gold);
    color: #000;
    border: none;
    cursor: var(--cursor-select);
    clip-path: polygon(6px 0, 100% 0, calc(100% - 6px) 100%, 0 100%);
    box-shadow: 0 0 14px rgba(252, 238, 9, 0.30);
    transition: background var(--dur), box-shadow var(--dur), transform var(--dur);
}
.ap__play:hover {
    background: #fff;
    box-shadow: 0 0 24px rgba(252, 238, 9, 0.50);
    transform: scale(1.07);
}
.ap__play:active { transform: scale(0.95); }

.ap__ico { transition: opacity var(--dur); }
.ap__ico--pause { display: none; }
.ap--playing .ap__ico--play  { display: none; }
.ap--playing .ap__ico--pause { display: block; }

.ap__track {
    flex: 1;
    height: 3px;
    background: rgba(255, 255, 255, 0.10);
    position: relative;
    cursor: var(--cursor-select);
    transition: height var(--dur);
}
.ap__track:hover { height: 5px; }

.ap__fill {
    position: absolute;
    left: 0; top: 0; bottom: 0;
    width: 0%;
    background: var(--cyan-accent);
    box-shadow: 0 0 6px rgba(0, 240, 255, 0.5);
    pointer-events: none;
    transition: width 60ms linear;
}

.ap__range {
    position: absolute;
    inset: -6px 0;
    width: 100%;
    height: calc(100% + 12px);
    margin: 0;
    padding: 0;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    z-index: 2;
    pointer-events: auto;
    touch-action: none;
    outline: none;
    cursor: var(--cursor-select);
}
.ap__range::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 12px;
    height: 12px;
    background: var(--cyan-accent);
    border: none;
    border-radius: 50%;
    box-shadow: 0 0 6px rgba(0, 240, 255, 0.7);
    cursor: var(--cursor-select);
}
.ap__range::-moz-range-thumb {
    width: 12px;
    height: 12px;
    background: var(--cyan-accent);
    border: none;
    border-radius: 50%;
    box-shadow: 0 0 6px rgba(0, 240, 255, 0.7);
    cursor: var(--cursor-select);
}
.ap__range::-webkit-slider-runnable-track { background: transparent; }
.ap__range::-moz-range-track { background: transparent; }

.ap__mute {
    flex-shrink: 0;
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.50);
    cursor: var(--cursor-select);
    transition: color var(--dur), background var(--dur);
}
.ap__mute:hover {
    color: var(--text-1);
    background: var(--bg-hover);
}
.ap--muted .ap__mute { color: var(--red); }

.ap__num {
    font-size: 9px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
    min-width: 22px;
    text-align: right;
    flex-shrink: 0;
}
```

### 6.4 Animations

```css
@keyframes apPulse {
    0%, 100% { opacity: 0.55; }
    50%      { opacity: 1.00; }
}
```

The existing `fadeUp` keyframe (in `styles-base.css:558-561`) is reused for the entry animation.

## 7. JavaScript Behavior

### 7.1 State model

Four states, single source of truth via `setState()`:

| State | When | DOM class | Status text | Dot color |
|---|---|---|---|---|
| `IDLE` | `audio.paused` AND `currentTime === 0` AND NOT muted | (none) | `IDLE` | gray (var(--text-3)) |
| `PLAYING` | `!audio.paused` AND NOT muted | `ap--playing` | `PLAYING` | green (var(--green)), pulsing |
| `PAUSED` | `audio.paused` AND `currentTime > 0` AND NOT muted | (none) | `PAUSED` | gray |
| `MUTED` | `audio.muted === true` | `ap--muted` | `MUTED` | red (var(--red)) |

**Priority on conflict:** `MUTED` > `PLAYING` > `PAUSED` > `IDLE`. Mute always wins.

### 7.2 Function: `initAudioPlayer()`

```js
function initAudioPlayer() {
    const audio   = document.getElementById('apAudio');
    const root    = document.getElementById('ap');
    const btnPlay = document.getElementById('apPlay');
    const btnMute = document.getElementById('apMute');
    const range   = document.getElementById('apRange');
    const fill    = document.getElementById('apFill');
    const num     = document.getElementById('apNum');
    const status  = document.getElementById('apStatus');
    const track   = document.getElementById('apTrack');
    if (!audio || !root) return;

    const savedVol  = parseInt(localStorage.getItem('apVol')  ?? '35', 10);
    const savedMute = localStorage.getItem('apMuted') === '1';
    const initVol   = Math.max(0, Math.min(100, isNaN(savedVol) ? 35 : savedVol));
    audio.volume = initVol / 100;
    audio.muted  = savedMute;
    audio.loop   = true;
    range.value      = initVol;
    fill.style.width  = initVol + '%';
    num.textContent   = initVol;
    setState(savedMute ? 'MUTED' : 'IDLE');

    function setState(s) {
        status.textContent = s;
        root.classList.toggle('ap--playing', s === 'PLAYING');
        root.classList.toggle('ap--muted',   s === 'MUTED');
    }

    function updateMuteIcon(v) {
        const icon = document.getElementById('apMuteIcon');
        if (!icon) return;
        const paths = {
            0:  'M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z',
            lo: 'M18.5 12c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM5 9v6h4l5 5V4L9 9H5z',
            hi: 'M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z'
        };
        icon.firstElementChild.setAttribute('d',
            audio.muted || v === 0 ? paths[0] : v < 50 ? paths.lo : paths.hi);
    }

    function setVolume(v, persist) {
        v = Math.max(0, Math.min(100, parseInt(v, 10) || 0));
        if (+range.value !== v) range.value = v;
        audio.volume = v / 100;
        if (audio.muted && v > 0) audio.muted = false;
        fill.style.width = v + '%';
        num.textContent  = v;
        updateMuteIcon(v);
        if (persist) localStorage.setItem('apVol', v);
        if (audio.muted)         setState('MUTED');
        else if (audio.paused)   setState(audio.currentTime > 0 ? 'PAUSED' : 'IDLE');
        else                     setState('PLAYING');
    }

    btnPlay?.addEventListener('click', () => {
        if (audio.paused) audio.play().then(() => setState('PLAYING')).catch(() => {});
        else             { audio.pause(); setState('PAUSED'); }
    });

    btnMute?.addEventListener('click', () => {
        audio.muted = !audio.muted;
        localStorage.setItem('apMuted', audio.muted ? '1' : '0');
        const v = parseInt(range.value, 10) || 0;
        if (audio.muted) {
            fill.style.width = '0%';
            num.textContent  = '0';
            setState('MUTED');
        } else {
            audio.volume = v / 100;
            fill.style.width = v + '%';
            num.textContent  = v;
            setState(audio.paused ? 'IDLE' : 'PLAYING');
        }
        updateMuteIcon(v);
    });

    range?.addEventListener('input',  () => setVolume(range.value, true));
    range?.addEventListener('change', () => setVolume(range.value, true));

    if (track && range) {
        let dragging = false;
        const getX = (e) => e.clientX ?? (e.touches && e.touches[0].clientX);
        const setFromX = (cx) => {
            const r = track.getBoundingClientRect();
            const x = Math.max(0, Math.min(r.width, cx - r.left));
            setVolume(Math.round((x / r.width) * 100), true);
        };
        const onDown = (e) => { dragging = true; setFromX(getX(e)); e.preventDefault(); };
        const onMove = (e) => { if (dragging && getX(e) != null) setFromX(getX(e)); };
        const onUp   = () => { dragging = false; };
        track.addEventListener('mousedown',  onDown);
        track.addEventListener('touchstart', onDown, { passive: false });
        range.addEventListener('mousedown',  onDown);
        range.addEventListener('touchstart', onDown, { passive: false });
        window.addEventListener('mousemove',  onMove);
        window.addEventListener('touchmove',  onMove, { passive: true });
        window.addEventListener('mouseup',    onUp);
        window.addEventListener('touchend',   onUp);
    }

    audio.addEventListener('play',  () => { if (!audio.muted) setState('PLAYING'); });
    audio.addEventListener('pause', () => { if (!audio.muted) setState(audio.currentTime > 0 ? 'PAUSED' : 'IDLE'); });
    audio.addEventListener('ended', () => setState('IDLE'));
    audio.addEventListener('error', () => setState('IDLE'));

    window.apSetAudioSource = (url) => {
        if (!url) return;
        audio.src = url;
        audio.load();
        audio.addEventListener('canplaythrough', () => {
            audio.play().then(() => setState('PLAYING')).catch(() => {});
        }, { once: true });
        document.addEventListener('click', function onFirstClick() {
            if (audio.paused && audio.src) {
                audio.play().then(() => setState('PLAYING')).catch(() => {});
            }
            document.removeEventListener('click', onFirstClick);
        });
    };
}
```

### 7.3 Persistence

| Key | Type | Range | Notes |
|---|---|---|---|
| `apVol` | string (parsed as int) | 0-100 | Renamed from `apVolume` |
| `apMuted` | string | `'0'` or `'1'` | Unchanged key, new value contract (was also `'0'`/`'1'`) |

### 7.4 Error handling

| Scenario | Response |
|---|---|
| `apAudio` not in DOM | `initAudioPlayer` early-return |
| `apSetAudioSource(undefined)` | No-op |
| Audio file 404 / load error | `audio.error` event → `setState('IDLE')`, `console.error` |
| Autoplay policy block | First-click fallback in `apSetAudioSource` |
| Slider drag outside track bounds | `setFromX` clamps to 0-100 |
| Corrupt localStorage value | `isNaN` check → default 35 |
| Multiple `apSetAudioSource` calls | `audio.src` overwritten; `canplaythrough` uses `{ once: true }` |
| CORS cross-origin audio | Silent (existing limitation; not in scope) |

## 8. Data Flow

```
[C# Bridge]
  ↓ onMediaReady(bgmUrl, videoUrl)
[script-home.js:345-362]
  ↓ window.apSetAudioSource(bgmUrl)
[script-misc.js → initAudioPlayer]
  ↓
<audio#apAudio>.src = url
  ↓
  ├─ canplaythrough → audio.play()
  └─ first click    → audio.play()  (autoplay fallback)
  ↓
DOM updates:
  - #ap.classList  → 'ap--playing' | 'ap--muted' | (none)
  - #apStatus.text → 'PLAYING' | 'PAUSED' | 'MUTED' | 'IDLE'
  - #apFill.width  → vol%
  - #apNum.text    → vol
  - #apRange.value → vol

User interactions:
  - click #apPlay       → audio.play() / pause()
  - click #apMute       → audio.muted toggle
  - input #apRange      → setVolume(v)
  - mousedown #apTrack  → setVolume(calc)
  - drag                → setVolume(calc)  (window mousemove)

Persistence (localStorage):
  - apVol   (0-100)
  - apMuted ('0' | '1')
```

**Public contract — unchanged:**
- `window.onMediaReady(bgmUrl, videoUrl)` — set by `script-home.js:345`
- `window.apSetAudioSource(url)` — exposed by new `initAudioPlayer`
- `initAudioPlayer()` — called by `script-core.js:18`

## 9. Testing Approach

No test framework. Manual via console.

**Smoke test:**

```
□ Reload launcher
□ Player appears bottom-left, fadeUp animation
□ Head: BGM tag (cyan) + gray dot + 'IDLE'
□ Body: gold play + track + mute + '35'
□ Click play → status 'PLAYING', dot green pulsing
□ Console: apSetAudioSource('bgm.mp3')
  □ Audio plays, status PLAYING
□ Drag slider:
  □ Thumb visible (cyan circle with glow)
  □ Fill width updates in real-time
  □ Number updates
  □ Audio volume changes
  □ localStorage.apVol updates
□ Click mute:
  □ Icon switches to X (red)
  □ Dot red
  □ Status MUTED
  □ Audio silent
□ Click mute again:
  □ Audio restores from saved vol
  □ Status returns to PLAYING/IDLE
□ Reload launcher:
  □ Vol + mute state persists
```

**CORS check:**
- `Resources/Web/bgm.mp3` (same-origin) → audio plays
- External `file://` or other-origin → silent (existing limitation)

**Visual integrity checks:**
- No leftover `.audio-player`, `.apVolBtn`, `.apVolSlider` etc. in HTML
- No leftover `.ap-art`, `.ap-viz`, `.ap-meta*`, `.ap-progress`, `.ap-track` etc. in CSS
- No leftover references to `bgMusic`, `apTrack`, `apFill`, `apCur`, `apDur`, `apShuffle`, `apPrev`, `apNext`, `apRepeat` in JS

**Post-impl cleanup:**
- `rm Resources/Web/bgm.mp3`

## 10. Migration / Rollout

Single-step migration. The new code replaces the old in the same commit. No phased rollout — the file boundaries are clean.

If something regresses post-deploy, rollback is a single `git revert`.

## 11. Resolved Questions

- **`toggleBgm` callers:** Grep across the repo shows zero callers (only the definition at `script-misc.js:477`). Safe to remove.

---

**End of spec.**
