function initBottomBar() {
    document.getElementById('btnStart')?.addEventListener('click', handleStart);

    const menuBtn  = document.getElementById('btnMenu');
    const dropdown = document.getElementById('rpDropdown');
    menuBtn?.addEventListener('click', e => {
        e.stopPropagation();
        const open = dropdown?.classList.toggle('open');
        menuBtn.classList.toggle('active', !!open);
    });
    document.addEventListener('click', () => {
        dropdown?.classList.remove('open');
        document.getElementById('btnMenu')?.classList.remove('active');
    });

    document.getElementById('menuGameDir')?.addEventListener('click', async () => {
        dropdown?.classList.remove('open');
        document.getElementById('btnMenu')?.classList.remove('active');
        await browseFolder();
    });

    document.getElementById('menuCheckVH')?.addEventListener('click', () => {
        dropdown?.classList.remove('open');
        document.getElementById('btnMenu')?.classList.remove('active');
        if (!S.gamePath) { toast('Chưa chọn thư mục game!', 'err'); return; }
        if (S.installing) return;
        startInstall();
    });

    document.getElementById('menuCheckUpdate')?.addEventListener('click', () => {
        dropdown?.classList.remove('open');
        document.getElementById('btnMenu')?.classList.remove('active');
        checkLauncherUpdate(false);
    });

    document.getElementById('menuForceQuit')?.addEventListener('click', () => {
        dropdown?.classList.remove('open');
        document.getElementById('btnMenu')?.classList.remove('active');
        if (bridge()) {
            bridge().ForceQuitGame();
            toast('Đã buộc thoát game.', 'ok');
        } else {
            toast('Demo: Buộc thoát game...', 'info');
        }
    });

    document.getElementById('menuRestartAdmin')?.addEventListener('click', () => {
        dropdown?.classList.remove('open');
        document.getElementById('btnMenu')?.classList.remove('active');
        if (bridge()) {
            bridge().RestartAsAdmin();
        } else {
            toast('Demo: Khởi động lại với Admin...', 'info');
        }
    });

    document.getElementById('menuUninstall')?.addEventListener('click', async () => {
        dropdown?.classList.remove('open');
        document.getElementById('btnMenu')?.classList.remove('active');
        if (!S.gamePath) { toast('Chưa chọn thư mục game!', 'err'); return; }
        const confirmed = await showConfirm('Bạn có chắc muốn gỡ bỏ bản Việt Hoá không?');
        if (!confirmed) return;
        if (bridge()) {
            const result = await bridge().Uninstall(S.gamePath);
            if (result === 'ok') {
                S.installed = false;
                const btn = document.getElementById('btnStart');
                const txt = document.getElementById('startBtnText');
                btn.classList.remove('installed');
                txt.textContent = 'Cài Việt Hoá';
                toast('Đã gỡ cài đặt Việt Hoá.', 'ok');
            } else {
                toast('Lỗi: ' + result, 'err');
            }
        } else {
            toast('Demo: Gỡ cài đặt...', 'info');
        }
    });
}

async function handleStart() {
    if (S.installing) return;
    if (!S.gamePath) {
        if (!await browseFolder()) return;
    }
    if (S.installed) { launchGame(); return; }
    startInstall();
}

function startInstall() {
    S.installing = true;
    const btn = document.getElementById('btnStart');
    const txt = document.getElementById('startBtnText');
    const prog = document.getElementById('progressSection');
    btn.classList.remove('installed');
    btn.classList.add('installing','disabled');
    txt.textContent = 'Đang cài đặt...';
    prog.style.display = '';
    const dx11Row = document.getElementById('dx11Row');
    if (dx11Row) dx11Row.style.display = 'none';

    if (bridge()) {
        bridge().StartInstallation(S.gamePath, S.cfg.vhMode, S.cfg.backup);
    } else {
        simulateInstall();
    }
}

function simulateInstall() {
    let p = 0;
    const total = 256;
    const iv = setInterval(() => {
        p += Math.random()*3+1;
        if (p>100) p = 100;
        setProgress(p, 'Đang tải xuống bản Việt Hoá...',
            (Math.random()*15+5).toFixed(1)+' MB/s',
            (total*p/100).toFixed(0)+' / '+total+' MB');
        if (p>=100) {
            clearInterval(iv);
            setTimeout(() => {
                setProgress(100, 'Đang cài đặt...', '', '');
                setTimeout(installDone, 1200);
            }, 400);
        }
    }, 150);
}

function setProgress(pct, text, speed, size) {
    const fill = document.getElementById('progressFill');
    const t = document.getElementById('progressText');
    const pc = document.getElementById('progressPct');
    const sp = document.getElementById('progressSpeed');
    const sz = document.getElementById('progressSize');
    if (fill) fill.style.width = pct+'%';
    if (t) t.textContent = text;
    if (pc) pc.textContent = Math.round(pct)+'%';
    if (sp) sp.textContent = speed;
    if (sz) sz.textContent = size;
}

function installDone() {
    S.installing = false;
    S.installed = true;
    loadVersions();
    const btn = document.getElementById('btnStart');
    const txt = document.getElementById('startBtnText');
    const prog = document.getElementById('progressSection');
    btn.classList.remove('installing','disabled');
    btn.classList.add('installed');
    txt.textContent = 'Chơi Game';
    prog.style.display = 'none';
    const dx11Row = document.getElementById('dx11Row');
    if (dx11Row) dx11Row.style.display = '';
    toast('Cài đặt Việt Hoá thành công!','ok');
}

function launchGame() {
    const dx11 = document.getElementById('chkDx11')?.checked ?? false;
    if (bridge()) {
        bridge().LaunchGame(S.gamePath, dx11);
    } else {
        toast('Demo: Đang khởi chạy game...','info');
    }
}

window.onProgressUpdate  = (p,t,sp,sz) => setProgress(p,t,sp,sz);
window.onInstallComplete = () => installDone();
window.onInstallError = msg => {
    S.installing = false;
    const btn  = document.getElementById('btnStart');
    const txt  = document.getElementById('startBtnText');
    const prog = document.getElementById('progressSection');
    btn.classList.remove('installing','disabled');
    txt.textContent = 'Thử lại';
    prog.style.display = 'none';
    const dx11Row = document.getElementById('dx11Row');
    if (dx11Row) dx11Row.style.display = 'none';
    toast('Lỗi: '+msg,'err');
};
window.onAdminRequired = () => {
    S.installing = false;
    const btn  = document.getElementById('btnStart');
    const txt  = document.getElementById('startBtnText');
    const prog = document.getElementById('progressSection');
    btn.classList.remove('installing','disabled');
    txt.textContent = 'Khởi động lại (Admin)';
    prog.style.display = 'none';
    const dx11Row = document.getElementById('dx11Row');
    if (dx11Row) dx11Row.style.display = 'none';

    const oldHandler = handleStart;
    btn.removeEventListener('click', oldHandler);

    const adminHandler = () => {
        if (bridge()) bridge().RestartAsAdmin();
    };
    btn.addEventListener('click', adminHandler);

    toast('Thư mục game đang bị khóa. Cần quyền Admin!', 'err');
};
window.onGamePathDetected = path => {
    S.gamePath = path;
    S.cfg.gamePath = path;
    saveSettings();
    if (!S.autoCheckDone && !S.installing) {
        S.autoCheckDone = true;
        setTimeout(() => { if (!S.installing) startInstall(); }, 800);
    }
};

(function() {
    let _targetDate = null;
    let _totalMs = 0;
    let _ticker = null;

    function pad(n) { return String(Math.max(0, n)).padStart(2, '0'); }

    function tick() {
        const el = document.getElementById('updateCountdown');
        if (!el || !_targetDate) return;

        const now = Date.now();
        const diff = _targetDate - now;

        if (diff <= 0) {

            ['ucDays','ucHours','ucMins','ucSecs'].forEach(id => {
                const e = document.getElementById(id);
                if (e) e.textContent = '00';
            });
            const fill = document.getElementById('ucBarFill');
            if (fill) fill.style.width = '100%';
            el.classList.add('uc-done');
            clearInterval(_ticker);
            _ticker = null;
            return;
        }

        el.classList.remove('uc-done');
        const totalSec = Math.floor(diff / 1000);
        const days  = Math.floor(totalSec / 86400);
        const hours = Math.floor((totalSec % 86400) / 3600);
        const mins  = Math.floor((totalSec % 3600) / 60);
        const secs  = totalSec % 60;

        const dEl = document.getElementById('ucDays');
        const hEl = document.getElementById('ucHours');
        const mEl = document.getElementById('ucMins');
        const sEl = document.getElementById('ucSecs');
        if (dEl) dEl.textContent = pad(days);
        if (hEl) hEl.textContent = pad(hours);
        if (mEl) mEl.textContent = pad(mins);
        if (sEl) sEl.textContent = pad(secs);

        const fill = document.getElementById('ucBarFill');
        if (fill && _totalMs > 0) {
            const elapsed = _totalMs - diff;
            fill.style.width = Math.min(100, (elapsed / _totalMs) * 100).toFixed(2) + '%';
        }
    }

    window.onUpdateDate = (dateStr) => {
        const el = document.getElementById('updateCountdown');
        if (!el) return;

        const target = new Date(dateStr);
        if (isNaN(target.getTime())) return;

        _targetDate = target.getTime();

        _totalMs = 6 * 7 * 24 * 3600 * 1000;

        el.style.display = '';
        tick();
        if (_ticker) clearInterval(_ticker);
        _ticker = setInterval(tick, 1000);
    };
})();

window.onMediaStatus = (status, msg) => {
    const el   = document.getElementById('rpStatus');
    const txt  = document.getElementById('rpStatusText');
    const bar  = document.getElementById('mediaProgressBar');
    const pct  = document.getElementById('mediaProgressPct');
    const size = document.getElementById('mediaProgressSize');
    if (!el) return;
    if (status === 'ready' || status === 'offline') {
        el.style.display = 'none';
    } else if (status === 'checking') {
        el.style.display = '';
        if (bar)  bar.style.display  = 'none';
        if (pct)  pct.textContent    = '';
        if (size) size.textContent   = '';
        if (txt)  txt.textContent    = 'Đang kiểm tra cập nhật...';
    } else if (status === 'error') {
        el.style.display = '';
        if (bar) bar.style.display = 'none';
        if (txt) txt.textContent  = msg || 'Lỗi tải tài nguyên';
    }
};

window.onMediaProgress = (pct, text, speed, size) => {
    const el    = document.getElementById('rpStatus');
    const txt   = document.getElementById('rpStatusText');
    const bar   = document.getElementById('mediaProgressBar');
    const fill  = document.getElementById('mediaProgressFill');
    const pctEl = document.getElementById('mediaProgressPct');
    const sizeEl= document.getElementById('mediaProgressSize');
    if (el)    el.style.display    = '';
    if (bar)   bar.style.display   = '';
    if (txt)   txt.textContent     = text;
    if (fill)  fill.style.width    = pct + '%';
    if (pctEl) pctEl.textContent   = pct + '%';
    if (sizeEl)sizeEl.textContent  = size;
};

window.onMediaReady = (bgmUrl, videoUrl) => {

    if (videoUrl) {
        const vid = document.getElementById('bgVideo');
        if (vid) {
            vid.src = videoUrl;
            vid.load();
            const onReady = () => {
                vid.play().catch(()=>{});
                vid.classList.add('visible');
                vid.removeEventListener('canplay', onReady);
            };
            vid.addEventListener('canplay', onReady);
        }
    }

    if (bgmUrl && window.apSetAudioSource) window.apSetAudioSource(bgmUrl);
    window.onMediaStatus('ready');
};

function initModal() {

}

function openModal() {}
function closeModal() {}

function populateModal() {}

function saveSettings() {
    if (bridge()) bridge().SaveSettings(JSON.stringify(S.cfg));
}

async function loadVersions() {
    if (!bridge()) return;
    try {
        const appVer = await bridge().GetAppVersion();
        const vhVer  = await bridge().GetVhVersion();
        const elApp = document.getElementById('verApp');
        const elVH  = document.getElementById('verVH');
        if (elApp) elApp.textContent = appVer ? `Launcher v${appVer}` : '';
        if (elVH)  elVH.textContent  = vhVer  ? `VH ${vhVer}` : '';
    } catch(e) {}
}

let _launcherUpdateUrl = '';
