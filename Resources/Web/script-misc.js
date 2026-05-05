let _launcherUpdateVer = '';

function checkLauncherUpdate(silent = true) {
    if (!silent) toast('Memeriksa pembaruan Launcher...', 'info');
    if (bridge()) bridge().CheckLauncherUpdate();
}

function loadReleaseNotes() {
    if (bridge()) {
        bridge().GetVHReleaseNotes();
    } else {
        setTimeout(() => {
            window.onVHReleaseNotes('v1.0-demo', new Date().toISOString(),
                '## Demo\n- Ini versi demo\n- Fitur lengkap tersedia setelah terhubung\n\n### Detail\n- Fitur A\n- Fitur B',
                'Demo Release');
        }, 800);
    }
}

function initSidePanel() {
    const btn   = document.getElementById('rnToggle');
    const panel = document.getElementById('sidePanel');
    if (!btn || !panel) return;
    panel.addEventListener('animationend', () => {
        panel.style.animation = 'none';
    }, { once: true });
    btn.addEventListener('click', () => panel.classList.toggle('collapsed'));
}

function _rnMd(text) {
    const lines = text.split('\n');
    let html = '';
    let inUl = false;

    const inline = s => s
        .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
        .replace(/\*(.+?)\*/g,     '<em>$1</em>')
        .replace(/`([^`]+)`/g,     '<code>$1</code>');

    for (const raw of lines) {
        const safe = raw
            .replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');

        const mH3 = safe.match(/^### (.+)/);
        const mH2 = safe.match(/^## (.+)/);
        const mH1 = safe.match(/^# (.+)/);
        const mLi = safe.match(/^[ \t]*[-*+] (.+)/);
        const mHr = /^---+$/.test(safe.trim());

        if (!mLi && inUl) { html += '</ul>'; inUl = false; }

        if      (mH1) html += `<h1>${inline(mH1[1])}</h1>`;
        else if (mH2) html += `<h2>${inline(mH2[1])}</h2>`;
        else if (mH3) html += `<h3>${inline(mH3[1])}</h3>`;
        else if (mHr) html += '<hr>';
        else if (mLi) {
            if (!inUl) { html += '<ul>'; inUl = true; }
            html += `<li>${inline(mLi[1])}</li>`;
        }
        else if (safe.trim() === '') {  }
        else html += `<p>${inline(safe)}</p>`;
    }

    if (inUl) html += '</ul>';
    return html;
}

window.onVHReleaseNotes = (tag, dateStr, body, name) => {
    const panel  = document.getElementById('sidePanel');
    const tagEl  = document.getElementById('rnTag');
    const dateEl = document.getElementById('rnDate');
    const bodyEl = document.getElementById('rnBody');
    if (!panel) return;

    if (tagEl) tagEl.textContent = tag || 'VH';

    if (dateEl && dateStr) {
        try {
            const d = new Date(dateStr);
            dateEl.textContent = d.toLocaleDateString('vi-VN',
                { day:'2-digit', month:'2-digit', year:'numeric' });
        } catch { dateEl.textContent = ''; }
    }

    if (bodyEl) {
        const content = (body || '').trim();
        bodyEl.innerHTML = content
            ? _rnMd(content)
            : '<p style="opacity:0.4">Tidak ada informasi.</p>';
    }

    const ap  = document.getElementById('audioPlayer');
    const gap = parseInt(getComputedStyle(document.documentElement)
        .getPropertyValue('--edge-gap')) || 20;
    panel.style.bottom = ap
        ? (gap + ap.offsetHeight + 10) + 'px'
        : gap + 'px';

    panel.dataset.loaded = '1';
    if (S.page === 'home') panel.style.display = 'flex';
};

window.onLauncherUpdateAvailable = (latestVer, downloadUrl) => {
    _launcherUpdateUrl = downloadUrl;
    _launcherUpdateVer = latestVer;

    const badge = document.getElementById('rpUpdateBadge');
    if (badge) badge.style.display = '';

    document.querySelector('.rp-version')?.classList.add('has-update');

    const overlay  = document.getElementById('luOverlay');
    const verEl    = document.getElementById('luModalVer');
    const pbar     = document.getElementById('luPbar');
    const btns     = document.getElementById('luModalBtns');
    const btnLater  = document.getElementById('luBtnLater');
    const btnUpdate = document.getElementById('luBtnUpdate');

    if (!overlay) return;
    if (verEl) verEl.textContent = latestVer;
    if (pbar)  pbar.style.display  = 'none';
    if (btns)  btns.style.display  = '';
    overlay.style.display = '';

    btnLater?.addEventListener('click', () => {
        overlay.style.display = 'none';
    }, { once: true });

    btnUpdate?.addEventListener('click', () => {
        if (!bridge()) return;
        if (btns) btns.style.display = 'none';
        if (pbar) pbar.style.display = '';
        btnUpdate.disabled = true;
        bridge().PerformLauncherUpdate(_launcherUpdateVer, _launcherUpdateUrl);
    }, { once: true });
};

window.onLauncherUpdateProgress = (pct, text) => {
    const fill    = document.getElementById('luPbarFill');
    const textEl  = document.getElementById('luPbarText');
    const subEl   = document.getElementById('luPbarSub');
    const pbar    = document.getElementById('luPbar');
    if (pbar) pbar.style.display = '';
    if (fill)   fill.style.width   = pct + '%';
    if (textEl) textEl.textContent = pct + '%';
    if (subEl)  subEl.textContent  = text;
};

window.onLauncherUpdateError = (msg) => {
    const overlay = document.getElementById('luOverlay');
    if (overlay) overlay.style.display = 'none';
    toast('Pembaruan gagal: ' + msg, 'err');
};

async function loadSettings() {
    if (bridge()) {
        try {
            const j = await bridge().LoadSettings();
            if (j) {
                Object.assign(S.cfg, JSON.parse(j));
                S.gamePath = S.cfg.gamePath || '';
            }
        } catch(e) {}
    }
    if (S.gamePath && !S.autoCheckDone && !S.installing) {
        S.autoCheckDone = true;
        setTimeout(() => { if (!S.installing) startInstall(); }, 800);
    }
}

async function browseFolder() {
    if (bridge()) {
        const p = await bridge().BrowseGameFolder();
        if (p === "?INVALID") {
            toast('Folder Wuthering Waves tidak ditemukan!', 'err');
            return false;
        }
        if (p) {
            S.cfg.gamePath = p;
            S.gamePath = p;
            saveSettings();
            toast('Folder dipilih: ' + p.split('\\').pop(), 'ok');
            return true;
        }
        return false;
    } else {
        S.gamePath = 'C:\\Wuthering Waves\\Wuthering Waves Game';
        S.cfg.gamePath = S.gamePath;
        saveSettings();
        toast('Demo: folder game dipilih', 'info');
        return true;
    }
}


function initWaterRipple() {
    document.addEventListener('click', e => {
        const origin = document.createElement('div');
        origin.className = 'ripple-origin';
        origin.style.left = e.clientX + 'px';
        origin.style.top  = e.clientY + 'px';

        const splash = document.createElement('div');
        splash.className = 'ripple-splash';
        origin.appendChild(splash);

        const config = [
            { delay:   0, dur:  880 },
            { delay: 110, dur: 1050 },
            { delay: 230, dur: 1230 },
            { delay: 370, dur: 1450 },
        ];
        config.forEach(({ delay, dur }) => {
            const ring = document.createElement('div');
            ring.className = 'ripple-ring';
            ring.style.setProperty('--delay', delay + 'ms');
            ring.style.setProperty('--dur',   dur   + 'ms');
            origin.appendChild(ring);
        });

        document.body.appendChild(origin);
        setTimeout(() => origin.remove(), 2000);
    });
}


function initAudioPlayer() {
    const audio      = document.getElementById('bgMusic');
    const player     = document.getElementById('audioPlayer');
    const btnPlay    = document.getElementById('apPlay');
    const track      = document.getElementById('apTrack');
    const fill       = document.getElementById('apFill');
    const curEl      = document.getElementById('apCur');
    const durEl      = document.getElementById('apDur');
    const btnShuffle = document.getElementById('apShuffle');
    const btnPrev    = document.getElementById('apPrev');
    const btnNext    = document.getElementById('apNext');
    const btnRepeat  = document.getElementById('apRepeat');
    const btnVolBtn  = document.getElementById('apVolBtn');
    const volSlider  = document.getElementById('apVolSlider');
    const volFill    = document.getElementById('apVolFill');
    const volLabel   = document.getElementById('apVolLabel');
    if (!audio || !player) return;

    const savedVol = parseInt(localStorage.getItem('apVolume') ?? '35', 10);
    const initVol  = Math.max(0, Math.min(100, isNaN(savedVol) ? 35 : savedVol));
    audio.volume   = initVol / 100;
    audio.loop     = true;
    if (volSlider) volSlider.value       = initVol;
    if (volFill)   volFill.style.width   = initVol + '%';
    if (volLabel)  volLabel.textContent  = initVol;
    updateVolIcon(initVol);

    function fmt(s) {
        if (!isFinite(s) || isNaN(s)) return '--:--';
        const m   = Math.floor(s / 60);
        const sec = Math.floor(s % 60);
        return `${m}:${sec.toString().padStart(2, '0')}`;
    }

    function setPlaying(on) {
        document.getElementById('apIconPlay') .style.display = on ? 'none' : '';
        document.getElementById('apIconPause').style.display = on ? ''     : 'none';
        player.classList.toggle('playing', on);
    }

    function updateProgress() {
        const pct = audio.duration ? (audio.currentTime / audio.duration) * 100 : 0;
        fill.style.width  = pct + '%';
        curEl.textContent = fmt(audio.currentTime);
        if (audio.duration) durEl.textContent = fmt(audio.duration);
    }

    function updateVolIcon(vol) {
        const icon = document.getElementById('apVolIcon');
        if (!icon) return;
        if (vol === 0) {
            icon.innerHTML = '<path fill="currentColor" d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"/>';
        } else if (vol < 50) {
            icon.innerHTML = '<path fill="currentColor" d="M18.5 12c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM5 9v6h4l5 5V4L9 9H5z"/>';
        } else {
            icon.innerHTML = '<path fill="currentColor" d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"/>';
        }
    }

    window.apSetAudioSource = (url) => {
        if (!url) return;
        audio.src = url;
        audio.load();
        audio.addEventListener('canplaythrough', () => {
            audio.play().then(() => setPlaying(true)).catch(()=>{});
        }, { once: true });
        document.addEventListener('click', function onFirstClick() {
            if (audio.paused && audio.src) audio.play().then(()=>setPlaying(true)).catch(()=>{});
            document.removeEventListener('click', onFirstClick);
        });
    };

    btnPlay?.addEventListener('click', () => {
        if (audio.paused) { audio.play().then(() => setPlaying(true)).catch(() => {}); }
        else              { audio.pause(); setPlaying(false); }
    });

    track?.addEventListener('click', e => {
        const rect  = track.getBoundingClientRect();
        const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
        if (audio.duration) audio.currentTime = ratio * audio.duration;
    });

    btnPrev?.addEventListener('click', () => { audio.currentTime = 0; });
    btnNext?.addEventListener('click', () => { audio.currentTime = 0; });

    btnRepeat?.addEventListener('click', () => {
        audio.loop = !audio.loop;
        btnRepeat.classList.toggle('ap-btn--active', audio.loop);
    });

    let shuffleOn = false;
    btnShuffle?.addEventListener('click', () => {
        shuffleOn = !shuffleOn;
        btnShuffle.classList.toggle('ap-btn--active', shuffleOn);
    });

    volSlider?.addEventListener('input', () => {
        const v = parseInt(volSlider.value, 10);
        audio.volume = v / 100;
        if (audio.muted && v > 0) audio.muted = false;
        if (volFill)  volFill.style.width  = v + '%';
        if (volLabel) volLabel.textContent  = v;
        updateVolIcon(v);
        localStorage.setItem('apVolume', v);
    });

    btnVolBtn?.addEventListener('click', () => {
        audio.muted = !audio.muted;
        const displayVol = audio.muted ? 0 : parseInt(volSlider?.value ?? '35', 10);
        updateVolIcon(displayVol);
        if (volLabel) volLabel.textContent = audio.muted ? '0' : (volSlider?.value ?? '35');
    });

    audio.addEventListener('timeupdate',     updateProgress);
    audio.addEventListener('loadedmetadata', () => { durEl.textContent = fmt(audio.duration); });
    audio.addEventListener('play',  () => setPlaying(true));
    audio.addEventListener('pause', () => setPlaying(false));
    audio.addEventListener('ended', () => { if (!audio.loop) setPlaying(false); });
}

function toggleBgm(on) {
    const a = document.getElementById('bgMusic');
    if (!a) return;
    if (on) { a.volume = 0.25; a.play().catch(() => {}); }
    else    { a.pause(); }
}



function showConfirm(message) {
    return new Promise(resolve => {
        const modal  = document.getElementById('confirmModal');
        const msgEl  = document.getElementById('modalMsg');
        const btnOk  = document.getElementById('modalOk');
        const btnCan = document.getElementById('modalCancel');
        msgEl.textContent = message;
        modal.style.display = 'flex';

        const cleanup = (result) => {
            modal.style.display = 'none';
            btnOk.removeEventListener('click', onOk);
            btnCan.removeEventListener('click', onCancel);
            resolve(result);
        };
        const onOk     = () => cleanup(true);
        const onCancel = () => cleanup(false);
        btnOk.addEventListener('click', onOk);
        btnCan.addEventListener('click', onCancel);
    });
}

function toast(msg, type='info') {
    const c = document.getElementById('toasts');
    if (!c) return;
    const el = document.createElement('div');
    el.className = 'toast toast--'+type;
    el.textContent = msg;
    c.appendChild(el);
    requestAnimationFrame(() => el.classList.add('show'));
    setTimeout(() => {
        el.classList.remove('show');
        setTimeout(() => el.remove(), 400);
    }, 3500);
}


const FC = {
    fontPath: '',
    building: false,
};

function initFontCreator() {
    document.getElementById('fcBrowseFont')?.addEventListener('click', fcBrowseFont);
    document.getElementById('fcBuildBtn')?.addEventListener('click', fcBuild);
    document.getElementById('fcRevertBtn')?.addEventListener('click', fcRevert);
}

function fcRefreshStatus() {
    if (!S.gamePath) {
        fcSetCurrentFont(null);
        return;
    }
    if (bridge()) {
        bridge().GetCustomFontName(S.gamePath).then(name => fcSetCurrentFont(name || null));
    }
}

function fcSetCurrentFont(name) {
    const nameEl   = document.getElementById('fcCurrentName');
    const revertBtn = document.getElementById('fcRevertBtn');
    if (!nameEl) return;
    if (name) {
        nameEl.textContent = name;
        nameEl.classList.add('fc-current__name--custom');
        if (revertBtn) revertBtn.style.display = '';
    } else {
        nameEl.textContent = 'Font asli (UTMAlexander)';
        nameEl.classList.remove('fc-current__name--custom');
        if (revertBtn) revertBtn.style.display = 'none';
    }
}

async function fcBrowseFont() {
    if (!bridge()) {
        FC.fontPath = 'C:\\Fonts\\MyFont.ttf';
        document.getElementById('fcFontDisplay').textContent = 'MyFont.ttf';
        document.getElementById('fcOutputName').value = 'MyFont';
        document.getElementById('fcBuildBtn').disabled = false;
        return;
    }
    const path = await bridge().BrowseFontFile();
    if (!path) return;
    FC.fontPath = path;
    const fileName = path.split('\\').pop().split('/').pop();
    const baseName = fileName.replace(/\.[^.]+$/, ''); 
    document.getElementById('fcFontDisplay').textContent = fileName;
    document.getElementById('fcOutputName').value = baseName;
    document.getElementById('fcBuildBtn').disabled = false;
    fcSetStatus('', false);
}

async function fcBuild() {
    if (FC.building) return;
    if (!FC.fontPath) { toast('Pilih file font terlebih dahulu!', 'err'); return; }
    if (!S.gamePath) { toast('Belum memilih folder game!', 'err'); return; }

    const baseName = (document.getElementById('fcOutputName')?.value.trim() || 'CustomFont');

    FC.building = true;
    const btn = document.getElementById('fcBuildBtn');
    if (btn) { btn.disabled = true; btn.classList.add('fc-btn--loading'); }
    fcSetStatus('Memproses...', false);

    if (bridge()) {
        bridge().CreateFontPak(FC.fontPath, S.gamePath, baseName);
    } else {
        setTimeout(() => {
            window.onFontPakDone(`C:\\WW\\wuwaIndonesia\\${baseName}_100_P.pak`, '2.4 MB');
        }, 1200);
    }
}

async function fcRevert() {
    if (!S.gamePath) { toast('Belum memilih folder game!', 'err'); return; }
    const confirmed = await showConfirm('Hapus font kustom dan gunakan lagi font asli UTMAlexander?');
    if (!confirmed) return;
    fcSetStatus('Menghapus font kustom...', false);
    if (bridge()) {
        bridge().RemoveCustomFont(S.gamePath);
    } else {
        setTimeout(() => window.onFontRevertDone(), 600);
    }
}

window.onFontPakProgress = (msg) => {
    fcSetStatus(msg, false);
};

window.onFontPakDone = (outputPath, sizeStr) => {
    FC.building = false;
    const btn = document.getElementById('fcBuildBtn');
    if (btn) { btn.disabled = false; btn.classList.remove('fc-btn--loading'); }

    const fileName = outputPath.split('\\').pop().split('/').pop();
    fcSetStatus(`✓ Terpasang: ${fileName} (${sizeStr})`, false, true);
    toast('Font berhasil dipasang!', 'ok');
    fcRefreshStatus();
};

window.onFontPakError = (msg) => {
    FC.building = false;
    const btn = document.getElementById('fcBuildBtn');
    if (btn) { btn.disabled = false; btn.classList.remove('fc-btn--loading'); }
    fcSetStatus('Error: ' + msg, true);
    toast('Error: ' + msg, 'err');
};

window.onFontRevertDone = () => {
    fcSetStatus('✓ Font kustom dihapus. Font asli akan diunduh lagi saat pembaruan.', false, true);
    toast('Font asli digunakan kembali!', 'ok');
    fcRefreshStatus();
};

window.onFontRevertError = (msg) => {
    fcSetStatus('Error: ' + msg, true);
    toast('Error: ' + msg, 'err');
};

function fcSetStatus(msg, isError, isSuccess = false) {
    const el = document.getElementById('fcStatus');
    if (!el) return;
    if (!msg) { el.style.display = 'none'; return; }
    el.style.display = '';
    el.className = 'fc-status' + (isError ? ' fc-status--err' : isSuccess ? ' fc-status--ok' : '');
    el.textContent = msg;
}





