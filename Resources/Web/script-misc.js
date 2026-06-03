let _launcherUpdateVer = '';

function checkLauncherUpdate(silent = true) {
    if (!silent) toast('Memeriksa pembaruan Launcher...', 'info');
    if (bridge()) bridge().CheckLauncherUpdate();
}

function initLogUpload() {
    const btn = document.getElementById('menuUploadLogs');
    if (!btn) return;

    let enabled = false;
    try {
        enabled = !!bridge()?.GetLogUploadEnabled();
    } catch {}

    btn.disabled = !enabled;
    btn.title = enabled
        ? 'Kirim log diagnostik'
        : 'Log upload belum dikonfigurasi';
    btn.classList.toggle('rp-dropdown__item--dim', !enabled);

    btn.addEventListener('click', () => {
        if (!enabled) {
            toast('Log upload belum dikonfigurasi', 'info');
            return;
        }
        btn.disabled = true;
        btn.classList.add('rp-dropdown__item--loading');
        toast('Mengirim log diagnostik...', 'info');
        bridge()?.UploadLogs(S.gamePath || '');
    });
}

window.onLogUploadStarted = () => {
    const btn = document.getElementById('menuUploadLogs');
    if (btn) {
        btn.disabled = true;
        btn.classList.add('rp-dropdown__item--loading');
    }
};

window.onLogUploadFinished = (msg) => {
    const btn = document.getElementById('menuUploadLogs');
    const enabled = !!bridge()?.GetLogUploadEnabled?.();
    if (btn) {
        btn.disabled = !enabled;
        btn.classList.remove('rp-dropdown__item--loading');
        btn.classList.toggle('rp-dropdown__item--dim', !enabled);
    }

    const text = msg || 'Log upload selesai.';
    toast(text, text.toLowerCase().startsWith('gagal') ? 'err' : 'ok');
};

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
    const warning  = document.getElementById('luRestartWarning');
    const countdown = document.getElementById('luRestartCountdown');

    if (!overlay) return;
    if (verEl) verEl.textContent = latestVer;
    if (pbar)  pbar.style.display  = 'none';
    if (btns)  btns.style.display  = '';
    if (warning) warning.style.display = 'none';
    if (countdown) countdown.textContent = '12';
    if (_restartTimer) {
        clearInterval(_restartTimer);
        _restartTimer = null;
    }
    if (btnUpdate) btnUpdate.disabled = false;
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
let _restartTimer = null;

window.onLauncherUpdateRestarting = () => {
    const warning  = document.getElementById('luRestartWarning');
    const countdown = document.getElementById('luRestartCountdown');
    const btns     = document.getElementById('luModalBtns');
    const pbar     = document.getElementById('luPbar');
    if (warning)  warning.style.display = '';
    if (btns)     btns.style.display    = 'none';
    if (pbar)     pbar.style.display    = 'none';

    let sec = 12;
    if (countdown) countdown.textContent = sec;

    if (_restartTimer) clearInterval(_restartTimer);
    _restartTimer = setInterval(() => {
        sec--;
        if (countdown) countdown.textContent = sec;
        if (sec <= 0) {
            clearInterval(_restartTimer);
            _restartTimer = null;
        }
    }, 1000);
};

async function loadSettings() {
    if (bridge()) {
        try {
            const j = await bridge().LoadSettings();
            if (j) {
                Object.assign(S.cfg, JSON.parse(j));
                S.gamePath = S.cfg.gamePath || '';
                S.cfg.installMethod = normalizeInstallMethod(S.cfg.installMethod);
                updateMethodMenu();
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

function initAudioPlayer() { /* stub — full impl lands in Task 4 */ }

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


