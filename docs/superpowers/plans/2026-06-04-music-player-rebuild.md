# Music Player Rebuild Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the drifting music player (mismatched HTML/CSS/JS) with a clean, launcher-style minimal player that has working play/pause, mute, and volume slider.

**Architecture:** Single-pass migration. Remove all old player code in one commit, then add the new HTML → CSS → JS in three atomic commits. No test framework exists in the project — manual visual verification via reload + console after each step.

**Tech Stack:** Plain HTML, CSS (with custom design tokens `var(--*)`), vanilla JavaScript, WebView2 (Chromium). localStorage for persistence.

**Reference spec:** `docs/superpowers/specs/2026-06-04-music-player-rebuild-design.md`

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `Resources/Web/index.html` | Modify | Add `<div class="ap" id="ap">` block and `<audio id="apAudio">` |
| `Resources/Web/styles-base.css` | Modify | Remove old `.ap-*` rules (lines 375-552), add new `.ap` and `.ap__*` rules |
| `Resources/Web/script-misc.js` | Modify | Remove old `initAudioPlayer()` (lines 310-475) and `toggleBgm()` (lines 477-482), add new `initAudioPlayer()` |
| `Resources/Web/bgm.mp3` | Delete | 8MB test file, should not ship |

---

## Task 1: Remove old music player code (CSS + HTML + JS)

**Files:**
- Modify: `Resources/Web/styles-base.css` (remove lines 375-552)
- Modify: `Resources/Web/index.html` (remove lines 427-443 and 503-505)
- Modify: `Resources/Web/script-misc.js` (remove lines 310-482)

- [ ] **Step 1: Remove old CSS rules from `styles-base.css`**

Open `Resources/Web/styles-base.css` in the editor. The block to remove is from line 375 (`.ap-art {`) through line 552 (`.ap-btn--active { color: var(--accent-gold) !important; }`). The block includes all `.ap-art`, `.ap-viz`, `.ap-body`, `.ap-meta__*`, `.ap-progress`, `.ap-time`, `.ap-track*`, `.ap-controls`, `.ap-vol*`, `.ap-btn*` rules.

Find this exact starting block (with surrounding context to be safe):

```css
.ap-art {
    position: relative;
    flex-shrink: 0;
    width: 62px; height: 62px;
```

And end at the last rule before the keyframes:

```css
.ap-btn--active { color: var(--accent-gold) !important; }
```

Delete from `.ap-art {` (inclusive) through `.ap-btn--active { color: var(--accent-gold) !important; }` (inclusive). Leave `@keyframes apBar`, `@keyframes fadeUp` (which are used by the new player — actually `apBar` is for `.ap-viz` which we removed, so `apBar` can also go; `fadeUp` is reused).

Keep `fadeUp`, delete `apBar`. After deletion, you should have a blank gap between `.audio-player { ... }` (the container) and the next CSS section (which is whatever follows line 552).

Verify the file still parses: use a CSS linter if available, or just visually inspect — the surrounding rules should still be intact.

- [ ] **Step 2: Remove old HTML block from `index.html`**

Open `Resources/Web/index.html`. Delete two blocks:

Block A (lines 427-443): the entire `<div class="audio-player" id="audioPlayer">` block including the comment lines around it. The block starts with:

```html
    <div class="audio-player" id="audioPlayer">
        <button class="ap-btn ap-btn--play" id="apPlay" title="Putar / Jeda">
```

and ends with:

```html
        </div>
    </div>
```

Delete it. Leave a single blank line in its place.

Block B (lines 503-505): the entire `<audio id="bgMusic" loop preload="auto">` element:

```html
<audio id="bgMusic" loop preload="auto">
        
    </audio>
```

Delete it. Leave a single blank line in its place.

- [ ] **Step 3: Remove old JS from `script-misc.js`**

Open `Resources/Web/script-misc.js`. Delete two functions:

Function A: `initAudioPlayer()` — from line 310 (`function initAudioPlayer() {`) through line 475 (the closing `}` of that function). The function ends right before `function toggleBgm(on) {` on line 477.

Function B: `toggleBgm()` — from line 477 (`function toggleBgm(on) {`) through line 482 (its closing `}`).

To be safe, delete from `function initAudioPlayer() {` (line 310) through the closing `}` of `toggleBgm` on line 482 inclusive. Leave a single blank line.

- [ ] **Step 4: Verify removal is complete and parseable**

Run:
```bash
node --check Resources/Web/script-misc.js
```
Expected: exits 0 with no output (JS parses).

Then:
```bash
grep -n "audio-player\|apVolBtn\|apVolSlider\|apVolFill\|apVolLabel\|bgMusic\|toggleBgm\|initAudioPlayer" Resources/Web/script-misc.js Resources/Web/styles-base.css Resources/Web/index.html
```
Expected: zero matches in all three files. (Old class names and IDs fully gone.)

- [ ] **Step 5: Commit the removal**

```bash
git add Resources/Web/script-misc.js Resources/Web/styles-base.css Resources/Web/index.html
git commit -m "chore(player): remove old music player code"
```

---

## Task 2: Add new HTML structure

**Files:**
- Modify: `Resources/Web/index.html` (insert in the same location where the old block was)

- [ ] **Step 1: Add the new `<div class="ap">` and `<audio>` blocks**

Open `Resources/Web/index.html`. Find the location where the old `.audio-player` block was (around line 427). The surrounding context is:

Before:
```html
    <div id="pagePerformance" class="page-overlay" style="display:none;">
```

After (in the original file, between the pagePerformance block and the audio-player block).

Insert the new HTML block at the location of the old `.audio-player` block (search for the `</div>` that closed the pagePerformance block, then add a blank line and the new block). The new block:

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

Note: 4-space indent (the file's tab/space style) — match what was used in the old block (it was 4 spaces).

- [ ] **Step 2: Verify HTML**

Run:
```bash
grep -n 'class="ap"\|id="ap"\|id="apAudio"\|id="apPlay"\|id="apMute"\|id="apRange"\|id="apFill"\|id="apNum"\|id="apStatus"\|id="apDot"\|id="apMuteIcon"\|id="apTrack"' Resources/Web/index.html
```
Expected: 12 matches, one for each new element ID and the root class.

- [ ] **Step 3: Commit the HTML**

```bash
git add Resources/Web/index.html
git commit -m "feat(player): add new HTML structure"
```

---

## Task 3: Add new CSS

**Files:**
- Modify: `Resources/Web/styles-base.css` (insert new rules after the `.audio-player` rule, or at end of file)

- [ ] **Step 1: Add the new CSS rules**

Open `Resources/Web/styles-base.css`. Find the existing `.audio-player` rule (around line 353). Replace it with the new `.ap` rule (renamed class), and add the rest of the new rules right after it. If `.audio-player` was already removed in Task 1, add the new block at the end of the file (before any final `@media` or comment block — anywhere in the file is fine).

Delete the old `.audio-player { ... }` block (it's now unused), then insert this new block in its place (or at end of file if `.audio-player` was already deleted):

```css
@keyframes apPulse {
    0%, 100% { opacity: 0.55; }
    50%      { opacity: 1.00; }
}

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

- [ ] **Step 2: Verify CSS**

Run:
```bash
grep -c "^\.ap" Resources/Web/styles-base.css
```
Expected: at least 14 lines starting with `.ap` (root, head, tag, dot, status, body, play, ico, track, fill, range, mute, num — 13 selectors, but some have multiple rules with `.ap--*` prefix so a few more).

Run:
```bash
grep -n "audio-player\|ap-art\|ap-viz\|ap-meta\|ap-progress\|ap-track\b\|ap-controls\|ap-shuffle\|ap-prev\|ap-next\|ap-repeat" Resources/Web/styles-base.css
```
Expected: zero matches. All old class selectors gone.

Open the file in a CSS validator or browser devtools to confirm no syntax errors. At minimum, visually inspect for unclosed braces.

- [ ] **Step 3: Commit the CSS**

```bash
git add Resources/Web/styles-base.css
git commit -m "feat(player): add new CSS styles"
```

---

## Task 4: Add new JavaScript controller

**Files:**
- Modify: `Resources/Web/script-misc.js` (insert new `initAudioPlayer()` after the existing `initToast()` or wherever fits)

- [ ] **Step 1: Find the right insertion point in `script-misc.js`**

Open `Resources/Web/script-misc.js`. Find the area where the old `initAudioPlayer` used to be (around line 310 — but it's now empty after Task 1 removed it). The location should be near other `init*` functions.

Look for the line `function initToasts()` or similar (the function called from `script-core.js`). Insert the new `initAudioPlayer()` right after it, with a blank line separator.

- [ ] **Step 2: Add the new `initAudioPlayer()` function**

Insert this code at the insertion point:

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

- [ ] **Step 3: Verify JS syntax**

Run:
```bash
node --check Resources/Web/script-misc.js
```
Expected: exits 0 with no output.

- [ ] **Step 4: Verify no stale references**

Run:
```bash
grep -n "bgMusic\|toggleBgm\|apVolBtn\|apVolSlider\|apVolFill\|apVolLabel\|apTrack\|apCur\|apDur\|apShuffle\|apPrev\|apNext\|apRepeat" Resources/Web/script-misc.js
```
Expected: zero matches.

- [ ] **Step 5: Verify `initAudioPlayer()` is still called**

Run:
```bash
grep -n "initAudioPlayer" Resources/Web/script-core.js Resources/Web/script-misc.js
```
Expected: 2 matches — one in `script-core.js` calling it, one in `script-misc.js` defining it. (The call from `script-core.js:18` was not removed — confirmed it should remain.)

- [ ] **Step 6: Commit the JS**

```bash
git add Resources/Web/script-misc.js
git commit -m "feat(player): add new JS controller"
```

---

## Task 5: Manual smoke test in launcher

**Files:** None modified — this is a verification task.

- [ ] **Step 1: Restart the launcher**

Close any running instance. Re-launch the application. (Or in WebView2 dev mode, press Ctrl+R / Ctrl+F5 to hard reload.)

- [ ] **Step 2: Verify initial render**

Visually confirm:
- Player appears in the **bottom-left** corner with the launcher's signature clip-path corner cuts.
- Fade-up animation plays (slides in from below).
- Header shows: cyan `BGM` tag (left) + status text (next to it) + small dot (right) — wait, the spec has dot on the right with `margin-left: auto`. Verify dot is on the right and tag is on the left.
- Body shows: gold play button (left) + thin track (middle) + small mute icon (next) + `35` number (right).

Expected status text on first load: `IDLE`. Expected dot color: gray.

- [ ] **Step 3: Test play + audio source**

Open browser devtools (F12) or use the WebView2 inspector. Run in the console:

```js
apSetAudioSource('bgm.mp3')
```

If `bgm.mp3` is still in `Resources/Web/` (it was added in earlier debugging), this loads same-origin audio and should work.

If the file is gone, you'll need a data URL workaround:
```js
apSetAudioSource('data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQAAAAA=')
```
(silent 1-frame WAV — confirms plumbing without playing actual audio)

Then click anywhere in the launcher (autoplay policy fallback) OR verify `canplaythrough` resolves and audio starts automatically.

Expected:
- Status changes to `PLAYING`.
- Dot turns green and pulses.
- Play icon swaps to pause icon.
- If using `bgm.mp3`, audio is audible.

- [ ] **Step 4: Test volume slider**

Click anywhere on the slider track. Verify:
- Thumb snaps to the click position.
- Fill width matches the click position.
- Number updates to the new value.
- Audio volume changes (audible difference if playing).

Drag the thumb left and right. Verify:
- Thumb follows the cursor.
- Fill animates smoothly (60ms transition).
- Number updates in real-time.
- Audio volume adjusts.

Test edge cases:
- Click at the very left → number = 0, audio silent, mute icon becomes X.
- Click at the very right → number = 100, audio at max.
- Drag outside the track → number clamps to 0 or 100.

- [ ] **Step 5: Test mute button**

Click the mute button (small icon next to track). Verify:
- Mute icon swaps to X.
- Dot turns red (no animation).
- Status text changes to `MUTED`.
- Audio becomes silent.
- Fill width drops to 0%, number shows 0.

Click mute again. Verify:
- Audio resumes at the saved volume (not from 0).
- Icon swaps back (X → low/hi based on level).
- Dot returns to green (if still playing) or gray (if paused).
- Status returns to `PLAYING` or `IDLE`.

- [ ] **Step 6: Test persistence**

With audio playing at e.g. volume 50 and not muted, reload the launcher (Ctrl+R). Verify:
- Volume stays at 50 (number shows `50`, fill width is 50%).
- Mute state is preserved.
- Status reflects the right state (IDLE since audio hasn't auto-played yet).

Run in console:
```js
localStorage.getItem('apVol')     // should be '50'
localStorage.getItem('apMuted')   // should be '0' or '1'
```

Both should be present (not null).

- [ ] **Step 7: Document any issues**

If any step above fails, do NOT proceed to Task 6. Report the issue. Likely candidates:
- **Slider not draggable:** input z-index or pointer-events issue. Check `styles-base.css` `.ap__range` rules.
- **Status not updating:** `setState` not called. Check `script-misc.js` event bindings.
- **Audio silent after `apSetAudioSource`:** cross-origin issue. Confirm `bgm.mp3` is in `Resources/Web/`.

If all steps pass, proceed to Task 6.

---

## Task 6: Cleanup test file

**Files:**
- Delete: `Resources/Web/bgm.mp3`

- [ ] **Step 1: Delete the test audio file**

```bash
rm Resources/Web/bgm.mp3
```

This 8MB file was copied earlier for local testing. It is not part of the launcher's production assets (the real BGM URL is provided by the C# bridge at runtime).

- [ ] **Step 2: Verify deletion**

```bash
ls Resources/Web/bgm.mp3 2>&1
```
Expected: `ls: cannot access 'Resources/Web/bgm.mp3': No such file or directory`.

- [ ] **Step 3: Commit the cleanup**

```bash
git add -A
git status
```

Expected: `Resources/Web/bgm.mp3` appears as deleted (or absent if already gitignored — but it's tracked, so should show as deleted).

```bash
git commit -m "chore: remove test audio file (8MB)"
```

- [ ] **Step 4: Final verification**

Run:
```bash
git log --oneline -8
```
Expected: 5 new commits on top of the design spec commit:
1. `chore(player): remove old music player code`
2. `feat(player): add new HTML structure`
3. `feat(player): add new CSS styles`
4. `feat(player): add new JS controller`
5. `chore: remove test audio file (8MB)`

Run final smoke check by reloading launcher once more and confirming the player is fully functional.

---

**End of plan.**
