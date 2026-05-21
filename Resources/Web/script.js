

const S = {
    page: 'home',
    installing: false, installed: false,
    gamePath: '',
    cfg: { gamePath:'' },
    autoCheckDone: false
};

const bridge = () => window.chrome?.webview?.hostObjects?.launcher;

document.addEventListener('DOMContentLoaded', () => {
    initParticles();
    initTopBar();
    initTopNav();
    initBottomBar();
    initAudioPlayer();
    initWaterRipple();
    loadSettings();
    loadVersions();
});

function initParticles() {
    const c = document.getElementById('particleCanvas');
    if (!c) return;
    const ctx = c.getContext('2d');
    let W, H;
    const P = [];
    const N = 35;

    function resize() { W = c.width = innerWidth; H = c.height = innerHeight; }
    resize();
    addEventListener('resize', resize);

    class Dot {
        constructor() { this.reset(); }
        reset() {
            this.x = Math.random()*W;
            this.y = Math.random()*H;
            this.r = Math.random()*1.8+0.4;
            this.vx = (Math.random()-0.5)*0.25;
            this.vy = -Math.random()*0.35-0.05;
            this.a = Math.random()*0.4+0.08;
            this.da = (Math.random()>0.5?1:-1)*(Math.random()*0.004+0.001);
            const gold = Math.random()>0.35;
            this.R = gold?210:80; this.G = gold?170:195; this.B = gold?68:220;
        }
        tick() {
            this.x += this.vx; this.y += this.vy;
            this.a += this.da;
            if (this.a>0.55) this.da = -Math.abs(this.da);
            if (this.a<0.04) this.da = Math.abs(this.da);
            if (this.y<-10||this.x<-10||this.x>W+10) {
                this.x = Math.random()*W; this.y = H+10; this.a = 0.04;
            }
        }
        draw(ctx) {
            ctx.beginPath();
            ctx.arc(this.x, this.y, this.r, 0, Math.PI*2);
            ctx.fillStyle = `rgba(${this.R},${this.G},${this.B},${this.a})`;
            ctx.fill();
            if (this.r>1) {
                ctx.beginPath();
                ctx.arc(this.x, this.y, this.r*3, 0, Math.PI*2);
                ctx.fillStyle = `rgba(${this.R},${this.G},${this.B},${this.a*0.12})`;
                ctx.fill();
            }
        }
    }
    for (let i=0; i<N; i++) P.push(new Dot());
    (function loop() {
        ctx.clearRect(0,0,W,H);
        P.forEach(p => { p.tick(); p.draw(ctx); });
        requestAnimationFrame(loop);
    })();
}

let navWaveT = 0;      // global time tick
let _indCurL = 0, _indCurW = 0;   // currently rendered bounds (px from nav left)
let _indTgtL = 0, _indTgtW = 0;   // target bounds
let _indReady = false;

function initNavWave() {
    return;
}

function drawNavWave(canvas) {
    const ctx = canvas.getContext('2d');
    const W = canvas.width;
    const H = canvas.height;
    if (W <= 0 || H <= 0 || !_indReady) return;

    ctx.clearRect(0, 0, W, H);

    const cL = _indCurL;        // indicator left edge (lerped)
    const cW = _indCurW;        // indicator width (lerped)
    if (cW <= 1) return;

    const t = navWaveT;
    const arch = x => Math.sin(Math.PI * (x - cL) / cW);
    const breathe = 0.88 + 0.12 * Math.sin(t * 0.010);
    const MAIN_AMP = H * 0.12;

    const N = 4;

    const drawArc = (side) => {
        for (let i = 0; i < N; i++) {
            const scale  = (i + 1) / N;
            const amp    = MAIN_AMP * scale * breathe;
            const freq   = 0.044 + i * 0.007;
            const speed  = 0.030 + i * 0.004;
            const phase  = i * 0.85 + (side > 0 ? Math.PI : 0);
            const oscAmp = H * 0.015 * scale;

            const yLine  = x =>
                H * 0.72
                + side * amp * arch(x)
                + oscAmp * arch(x) * Math.sin((x - cL) * freq + t * speed + phase);

            const outerRatio = scale;
            const op   = 0.10 + outerRatio * 0.45;
            const lw   = 0.4  + outerRatio * 1.2;
            const blur = 2    + outerRatio * 8;

            ctx.save();
            ctx.shadowColor = `rgba(232,85,126,${op * 0.55})`;
            ctx.shadowBlur  = blur;
            ctx.beginPath();
            for (let x = cL; x <= cL + cW; x++) {
                x === cL ? ctx.moveTo(x, yLine(x)) : ctx.lineTo(x, yLine(x));
            }
            ctx.strokeStyle = `rgba(255,107,157,${op})`;
            ctx.lineWidth   = lw;
            ctx.stroke();
            ctx.restore();
        }
    };

    drawArc(-1); // top arcs
    drawArc(+1); // bottom arcs
}

function initTopNav() {
    document.querySelectorAll('.top-nav__item').forEach(btn => {
        btn.addEventListener('click', () => switchPage(btn.dataset.page));
    });
    requestAnimationFrame(() => {
        updateNavIndicator();
        initNavWave();
    });
}

let _adminCheckDone = false;

async function checkAdminIfNeeded() {
    if (_adminCheckDone || !S.gamePath || !bridge()) return;
    _adminCheckDone = true;
    try {
        const result = await bridge().CheckGameFolderWriteAccess(S.gamePath);
        if (result !== 'admin_required') return;
        const pathEl = document.getElementById('adminModalPath');
        if (pathEl) pathEl.textContent = S.gamePath;

        const ok = await showAdminModal();
        if (ok && bridge()) {
            await gfxSaveCache();
            bridge().RestartAsAdmin();
        }
    } catch(e) {}
}

function showAdminModal() {
    return new Promise(resolve => {
        const modal  = document.getElementById('adminModal');
        const btnOk  = document.getElementById('adminModalOk');
        const btnCan = document.getElementById('adminModalCancel');
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

function switchPage(page) {
    S.page = page;
    const isHome = page === 'home';
    document.querySelectorAll('.top-nav__item').forEach(b =>
        b.classList.toggle('active', b.dataset.page === page));
    document.getElementById('rightPanel').style.display        = isHome                  ? '' : 'none';
    document.getElementById('pageFontCreator').style.display   = page === 'font-creator' ? '' : 'none';
    document.getElementById('pageGraphics').style.display      = page === 'graphics'     ? '' : 'none';
    const ap = document.getElementById('audioPlayer');
    const uc = document.getElementById('updateCountdown');
    if (ap) ap.style.display = isHome ? '' : 'none';
    if (uc) uc.style.display = isHome ? '' : 'none';
    updateNavIndicator();
    if (page === 'font-creator') { fcRefreshStatus(); checkAdminIfNeeded(); }
    if (page === 'graphics')     { gfxInit(); checkAdminIfNeeded(); }
}

function updateNavIndicator() {
    const active = document.querySelector('.top-nav__item.active');
    const nav    = document.getElementById('topNav');
    if (!active || !nav) return;

    const navRect = nav.getBoundingClientRect();
    const actRect = active.getBoundingClientRect();

    _indTgtL = actRect.left - navRect.left;
    _indTgtW = actRect.width;

    if (!_indReady) {
        _indCurL = _indTgtL;
        _indCurW = _indTgtW;
        _indReady = true;
    }
    const canvas = document.getElementById('navWaveCanvas');
    if (canvas) {
        const fw = Math.round(navRect.width);
        const fh = Math.round(navRect.height);
        if (canvas.width !== fw || canvas.height !== fh) {
            canvas.width  = fw;  canvas.style.width  = fw + 'px';
            canvas.height = fh;  canvas.style.height = fh + 'px';
        }
    }
}

function initTopBar() {
    document.getElementById('btnMinimize')?.addEventListener('click', () => bridge()?.MinimizeWindow());
    document.getElementById('btnClose')?.addEventListener('click', () => bridge()?.CloseWindow());
    document.addEventListener('mousedown', e => {
        if (e.button !== 0) return;
        if (e.target.closest('button, a, input, select, label, .sidebar__inner, .right-panel')) return;
        window.chrome?.webview?.postMessage('drag');
    });
}

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
        if (!S.gamePath) { toast('Direktori game belum dipilih!', 'err'); return; }
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
            toast('Game berhasil dihentikan paksa.', 'ok');
        } else {
            toast('Demo: Paksa tutup game...', 'info');
        }
    });

    document.getElementById('menuRestartAdmin')?.addEventListener('click', () => {
        dropdown?.classList.remove('open');
        document.getElementById('btnMenu')?.classList.remove('active');
        if (bridge()) {
            bridge().RestartAsAdmin();
        } else {
            toast('Demo: Mulai ulang sebagai Admin...', 'info');
        }
    });

    document.getElementById('menuUninstall')?.addEventListener('click', async () => {
        dropdown?.classList.remove('open');
        document.getElementById('btnMenu')?.classList.remove('active');
        if (!S.gamePath) { toast('Direktori game belum dipilih!', 'err'); return; }
        const confirmed = await showConfirm('Yakin ingin menghapus Patch ID?');
        if (!confirmed) return;
        if (bridge()) {
            const result = await bridge().Uninstall(S.gamePath);
            if (result === 'ok') {
                S.installed = false;
                const btn = document.getElementById('btnStart');
                const txt = document.getElementById('startBtnText');
                btn.classList.remove('installed');
                txt.textContent = 'Instal Patch ID';
                toast('Patch ID berhasil dihapus.', 'ok');
            } else {
                toast('Error: ' + result, 'err');
            }
        } else {
            toast('Demo: menghapus patch...', 'info');
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
    txt.textContent = 'Sedang memasang...';
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
        setProgress(p, 'Mengunduh Patch ID...',
            (Math.random()*15+5).toFixed(1)+' MB/s',
            (total*p/100).toFixed(0)+' / '+total+' MB');
        if (p>=100) {
            clearInterval(iv);
            setTimeout(() => {
                setProgress(100, 'Sedang memasang...', '', '');
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
    txt.textContent = 'Mainkan Game';
    prog.style.display = 'none';
    const dx11Row = document.getElementById('dx11Row');
    if (dx11Row) dx11Row.style.display = '';
    toast('Patch ID berhasil diinstal!','ok');
}

function launchGame() {
    const dx11 = document.getElementById('chkDx11')?.checked ?? false;
    if (bridge()) {
        bridge().LaunchGame(S.gamePath, dx11);
    } else {
        toast('Demo: menjalankan game...','info');
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
    txt.textContent = 'Coba lagi';
    prog.style.display = 'none';
    const dx11Row = document.getElementById('dx11Row');
    if (dx11Row) dx11Row.style.display = 'none';
    toast('Error: '+msg,'err');
};
window.onAdminRequired = () => {
    S.installing = false;
    const btn  = document.getElementById('btnStart');
    const txt  = document.getElementById('startBtnText');
    const prog = document.getElementById('progressSection');
    btn.classList.remove('installing','disabled');
    txt.textContent = 'Mulai ulang (Admin)';
    prog.style.display = 'none';
    const dx11Row = document.getElementById('dx11Row');
    if (dx11Row) dx11Row.style.display = 'none';
    const oldHandler = handleStart;
    btn.removeEventListener('click', oldHandler);
    
    const adminHandler = () => {
        if (bridge()) bridge().RestartAsAdmin();
    };
    btn.addEventListener('click', adminHandler);
    
    toast('Folder game terkunci. Perlu izin Admin!', 'err');
};
window.onGamePathDetected = path => {
    S.gamePath = path;
    S.cfg.gamePath = path;
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

    function reposition() {
        const el  = document.getElementById('updateCountdown');
        const ap  = document.getElementById('audioPlayer');
        if (!el || !ap) return;
        const apH = ap.offsetHeight;
        const gap = parseInt(getComputedStyle(document.documentElement)
            .getPropertyValue('--edge-gap')) || 20;
        el.style.bottom = (gap + apH + 10) + 'px';
    }

    window.onUpdateDate = (dateStr) => {
        const el = document.getElementById('updateCountdown');
        if (!el) return;

        const target = new Date(dateStr);
        if (isNaN(target.getTime())) return;

        _targetDate = target.getTime();
        _totalMs = 6 * 7 * 24 * 3600 * 1000;

        el.style.display = '';
        reposition();
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
        if (txt)  txt.textContent    = 'Memeriksa pembaruan...';
    } else if (status === 'error') {
        el.style.display = '';
        if (bar) bar.style.display = 'none';
        if (txt) txt.textContent  = msg || 'Gagal memuat aset';
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
        if (elVH)  elVH.textContent  = vhVer  ? `ID ${vhVer}` : '';
    } catch(e) {}
}

let _launcherUpdateUrl = '';

const GFX_CATS = [
    { id:'texture', title:'TEXTURE & LOD', items:[
        {k:'r.MipMapLODBias', l:'Anisotropic Filtering (LOD Bias)', d:'Anti-aliasing anisotropik. Nilai lebih rendah membuat tekstur lebih tajam tetapi lebih berat.', t:'slider', v:-2, min:-3, max:0, step:1, pw:3, pi:true},
        {k:'r.Streaming.MinBoost', l:'Prioritas Texture HD', d:'Memprioritaskan texture resolusi tinggi. Angka lebih tinggi memberi prioritas lebih besar ke texture tajam.', w:'Setelan terlalu tinggi bisa menyentuh batas VRAM dan menyebabkan stutter, terutama di Lahai Roi.', t:'slider', v:2.0, min:1.0, max:3.0, step:0.5, pw:3},
        {k:'r.Streaming.PoolSize', l:'Batas VRAM untuk Texture', d:'Batas VRAM maksimum untuk texture streaming (MB). 0 = tanpa batas.', w:'Set 0 (Unlimited) memakai banyak VRAM; GPU lemah bisa crash.', t:'select', v:1536, opts:[{v:0,l:'Unlimited'},{v:1536,l:'1.5 GB'},{v:2048,l:'2 GB'},{v:4096,l:'4 GB'}], pw:4},
        {k:'r.Streaming.UsingKuroStreamingPriority', l:'Prioritas streaming (Kuro)', d:'Tingkat prioritas streaming khusus Kuro.', w:'0 memperbaiki armor Aemeath tetapi Smartprint Cube Reboot memuat lebih lambat.', t:'select', v:0, opts:[{v:0,l:'Mati'},{v:1,l:'Hanya simpan'},{v:2,l:'Hanya muat'},{v:3,l:'Penuh'}], pw:2},
        {k:'r.streaming.MeshMaxKeepMips', l:'Pertahankan Mesh Mipmap', d:'Jumlah Mipmap mesh yang disimpan di memori. Default engine = 8.', t:'slider', v:15, min:8, max:15, step:1, pw:2},
        {k:'r.streaming.TextureMaxKeepMips', l:'Pertahankan Texture Mipmap', d:'Jumlah Mipmap texture yang disimpan di memori. Default engine = 15.', t:'slider', v:15, min:8, max:15, step:1, pw:2},
        {k:'r.StaticMeshLODDistanceScale', l:'Jarak LOD objek', d:'Rasio jarak perpindahan LOD. Makin rendah, objek jauh makin detail.', w:'Jangan di bawah 0.5; bisa membuat texture tanah berkedip.', t:'slider', v:0.7, min:0.5, max:1.0, step:0.1, pw:3, pi:true},
        {k:'wp.Runtime.PlannedLoadingRangeScale', l:'Jarak load objek', d:'Angka lebih tinggi membuat objek dan texture jauh dimuat lebih baik. Batas sekitar 1.0.', t:'slider', v:1.0, min:0.5, max:1.0, step:0.1, pw:2},
    ]},
    { id:'viewdist', title:'JARAK PANDANG', items:[
        {k:'foliage.LODDistanceScale', l:'Jarak pandang vegetasi', d:'Rasio tampilan vegetasi di kejauhan.', w:'1 = 97 FPS | 3 = 93 FPS | 5 = 85 FPS (test di Septimont).', t:'slider', v:2, min:1, max:5, step:1, pw:5},
        {k:'r.Kuro.Foliage.GrassCullDistanceMax', l:'Jarak cull rumput', d:'Jarak maksimum sebelum rumput dikeluarkan dari tampilan.', t:'input', v:20000, min:5000, max:50000, pw:4},
        {k:'r.Kuro.Foliage.Grass3_0CullDistanceMax', l:'Jarak cull rumput 3.0', d:'Jarak maksimum rumput tipe baru (3.0) sebelum di-cull.', t:'input', v:20000, min:5000, max:50000, pw:4},
    ]},
    { id:'ssr', title:'REFLEKSI SSR', items:[
        {k:'r.SSR.Quality', l:'Kualitas SSR', d:'Kualitas refleksi ruang layar (Screen Space Reflections).', t:'select', v:3, opts:[{v:0,l:'Mati'},{v:1,l:'Rendah'},{v:2,l:'Sedang'},{v:3,l:'Tinggi'},{v:4,l:'Sangat tinggi'}], pw:5},
        {k:'r.SSR.MaxRoughness', l:'Roughness maksimum SSR', d:'Roughness maksimum sebelum SSR mulai memudar. 1.0 = refleksi semua permukaan.', t:'slider', v:1.0, min:0.0, max:1.0, step:0.1, pw:3},
        {k:'r.SSR.HalfResSceneColor', l:'SSR setengah resolusi', d:'1 = setengah resolusi (lebih ringan). 0 = full (lebih bagus).', t:'toggle', v:0, pw:3, pi:true},
    ]},
    { id:'shadow', title:'BAYANGAN', items:[
        {k:'r.Shadow.RadiusThreshold', l:'Ambang bayangan', d:'Makin rendah, bayangan makin banyak/jelas.', t:'slider', v:0.01, min:0.01, max:0.10, step:0.01, pw:3, pi:true},
        {k:'r.Shadow.MaxCSMResolution', l:'Resolusi Shadow Map (CSM)', d:'Resolusi cascade shadow map. Angka lebih tinggi membuat bayangan lebih tajam.', t:'select', v:2048, opts:[{v:512,l:'512'},{v:1024,l:'1024'},{v:2048,l:'2048'},{v:4096,l:'4096'}], pw:5},
        {k:'r.Shadow.PerObjectResolutionMax', l:'Shadow PerObject (Max)', d:'Resolusi maksimum bayangan karakter/objek. Default game cukup rendah sehingga terlihat bergerigi.', t:'select', v:1024, opts:[{v:256,l:'256'},{v:512,l:'512'},{v:1024,l:'1024'},{v:2048,l:'2048'},{v:4096,l:'4096'}], pw:4},
        {k:'r.Shadow.PerObjectResolutionMin', l:'Shadow PerObject (Min)', d:'Resolusi minimum bayangan karakter/objek.', t:'select', v:1024, opts:[{v:256,l:'256'},{v:512,l:'512'},{v:1024,l:'1024'},{v:2048,l:'2048'},{v:4096,l:'4096'}], pw:3},
    ]},
    { id:'ao', title:'AMBIENT OCCLUSION', items:[
        {k:'r.AODownsampleFactor', l:'Resolusi DFAO', d:'1 = full (bagus/berat), 2 = setengah (default engine).', t:'select', v:2, opts:[{v:1,l:'Full (Berat)'},{v:2,l:'Setengah (Ringan)'}], pw:3, pi:true},
        {k:'r.AmbientOcclusion.Intensity', l:'Intensitas AO', d:'Intensitas bayangan di sudut sempit/kontak objek. Nilai lebih besar = bayangan lebih pekat.', t:'slider', v:0.9, min:0.0, max:2.0, step:0.1, pw:1},
    ]},
    { id:'fog', title:'KABUT & AWAN', items:[
        {k:'r.KuroVolumeCloudEnable', l:'Awan volumetrik', d:'Aktif/nonaktif awan/kabut dekat tanah di area map baru.', w:'Mati untuk menaikkan FPS dan visibilitas.', t:'toggle', v:0, pw:4},
        {k:'r.SSFS', l:'Kabut ruang layar', d:'Screen Space Fog Scattering — efek hamburan kabut.', t:'toggle', v:1, pw:2},
    ]},
    { id:'cull', title:'CULLING & OPTIMASI', items:[
        {k:'r.HZBOcclusion', l:'HZB Occlusion Culling', d:'Mengubah algoritma culling ke HZB. Memperbaiki bintik putih saat kamera diputar.', w:'⚠ AKAN MENURUNKAN FPS BESAR (~10 FPS di Ragunna). PC lemah sebaiknya tidak memakai ini.', t:'toggle', v:1, pw:6},
        {k:'r.ParallelFrustumCull', l:'Parallel Frustum Cull', d:'Mengoptimalkan CPU saat rendering. Aktifkan untuk menaikkan performa.', t:'toggle', v:1, pw:0},
        {k:'r.ParallelOcclusionCull', l:'Parallel Occlusion Cull', d:'Mengoptimalkan CPU untuk occlusion culling.', t:'toggle', v:1, pw:0},
        {k:'r.ScreenSizeCullRatioFactor', l:'Rasio cull berdasarkan ukuran', d:'Angka lebih besar membuat objek kecil jauh lebih banyak di-cull -> FPS naik.', t:'slider', v:3, min:1, max:5, step:1, pw:2, pi:true},
    ]},
    { id:'post', title:'POST-PROCESS & FILTER WARNA', items:[
        {k:'r.KuroTonemapping', l:'Filter warna (Tonemapping)', d:'Mode filter warna keseluruhan game.', t:'select', v:3, opts:[{v:0,l:'Mati'},{v:1,l:'Mode Genshin'},{v:2,l:'Mode Death Stranding'},{v:3,l:'Kuro (Default)'}], pw:0},
        {k:'r.Kuro.KuroEnableFFTBloom', l:'FFT Bloom (Lahai-Roi)', d:'Efek bloom tipe baru di Lahai-Roi.', t:'toggle', v:0, pw:3},
        {k:'r.Kuro.KuroEnableToonFFTBloom', l:'Toon FFT Bloom', d:'Efek bloom untuk karakter toon.', t:'toggle', v:0, pw:3},
        {k:'r.Kuro.KuroBloomStreak', l:'Bloom Streak', d:'Streak bloom. Matikan untuk mengurangi silau.', t:'toggle', v:0, pw:1},
        {k:'r.EnableLensflareSceneSample', l:'Lens Flare', d:'Efek lens flare.', t:'toggle', v:0, pw:1},
        {k:'r.DepthOfFieldQuality', l:'Depth of Field', d:'Kualitas depth of field.', t:'select', v:0, opts:[{v:0,l:'Mati'},{v:1,l:'Rendah'},{v:2,l:'Tinggi'},{v:3,l:'Sangat tinggi'},{v:4,l:'Ekstra tinggi'}], pw:3},
        {k:'r.SceneColorFringeQuality', l:'Chromatic Aberration', d:'Fringe warna di tepi layar. Matikan agar gambar lebih bersih.', t:'toggle', v:0, pw:0},
        {k:'r.Tonemapper.Quality', l:'Kualitas Tonemapper', d:'0 = semua mati. 1 = FilmContrast (lebih terang, matikan grain + vignette).', t:'select', v:1, opts:[{v:0,l:'Mati'},{v:1,l:'FilmContrast'},{v:2,l:'Vignette'},{v:4,l:'Vignette + Noise'}], pw:1},
    ]},
    { id:'fx', title:'EFEK & LAINNYA', items:[
        {k:'a.URO.ForceAnimRate', l:'Update Animasi NPC', d:'1 = update tiap frame, memperbaiki NPC jauh yang patah-patah. 0 = default engine.', w:'Aktif akan memakai CPU lebih banyak.', t:'toggle', v:1, pw:2},
        {k:'r.Upscale.Quality', l:'Kualitas Upscale UI', d:'Kualitas upscale antarmuka. 3 = paling tajam.', w:'Jangan gunakan angka di atas 3; bisa menyebabkan glitch UI.', t:'select', v:3, opts:[{v:0,l:'0'},{v:1,l:'1'},{v:2,l:'2'},{v:3,l:'3 (Paling tajam)'}], pw:1},
        {k:'r.VRS.EnableMaterial', l:'VRS Material', d:'Variable Rate Shading untuk material. Mati = lebih stabil di beberapa GPU.', t:'toggleStr', v:'false', pw:2, pi:true},
        {k:'r.VRS.EnableMesh', l:'VRS Mesh', d:'Variable Rate Shading untuk mesh. Mati = lebih stabil.', t:'toggleStr', v:'false', pw:2, pi:true},
        {k:'r.LightShaftDownSampleFactor', l:'Resolusi light shaft', d:'1 = paling tajam. Angka lebih besar = kualitas lebih rendah (lebih ringan).', t:'slider', v:1, min:1, max:8, step:1, pw:2, pi:true},
        {k:'r.KuroVolumetricLight.ColorMaskDownSampleFactor', l:'Volumetric Light (Color)', d:'1 = paling tajam. Angka lebih besar = kualitas lebih rendah.', t:'slider', v:1, min:1, max:4, step:1, pw:2, pi:true},
        {k:'r.KuroVolumetricLight.DownSampleFactor', l:'Volumetric Light (Main)', d:'1 = paling tajam. Angka lebih besar = kualitas lebih rendah.', t:'slider', v:1, min:1, max:4, step:1, pw:2, pi:true},
        {k:'r.Kuro.InteractionEffect.UseCppWaterEffect', l:'Efek riak air', d:'Efek riak air saat terbang rendah / memakai skill.', t:'toggle', v:1, pw:1},
        {k:'foliage.DensityScaleLOD.DrawCallOptimize', l:'Optimasi Draw Call dedaunan', d:'Aktifkan untuk mengoptimalkan CPU saat render kepadatan dedaunan.', t:'toggle', v:1, pw:0},
        {k:'wp.Runtime.SoraGridBlackListHeight', l:'Tinggi cahaya (terbang)', d:'Ketinggian sebelum cahaya/objek hilang saat terbang.', t:'input', v:20000, min:5000, max:50000, pw:1},
        {k:'r.Kuro.NpcDisappearDistance', l:'Jarak NPC menghilang', d:'Set tinggi agar NPC tidak menghilang terlalu cepat.', t:'input', v:20000, min:1000, max:50000, pw:2},
        {k:'Kuro.Blueprint.EnableGameBudget', l:'Blueprint Game Budget', d:'Mengelola batas waktu eksekusi Blueprint.', w:'⚠ Mati memperbaiki lag animasi Lahai-Roi, TAPI merusak objek interaktif di Honami (payung) dan Lahai-Roi (bola).', t:'toggleStr', v:'false', pw:2, pi:true},
    ]},
    { id:'nvidia', title:'NVIDIA (RTX)', items:[
        {k:'t.Streamline.Reflex.HandleMaxTickRate', l:'Reflex Handle Tick Rate', d:'Aktifkan agar Reflex mengelola tick rate.', t:'toggleStr', v:'true', pw:0},
        {k:'t.Streamline.Reflex.Enable', l:'NVIDIA Reflex', d:'Mengurangi latensi input.', w:'Tidak berfungsi pada seri RTX 40/50 saat Frame Gen aktif.', t:'toggle', v:1, pw:0},
        {k:'t.Streamline.Reflex.Mode', l:'Reflex Mode', d:'1 = Latensi rendah, 2 = Rendah + Boost.', t:'select', v:1, opts:[{v:1,l:'Rendah'},{v:2,l:'Rendah + Boost'}], pw:1},
        {k:'r.NGX.DLAA.Enable', l:'DLAA (Anti-aliasing NVIDIA)', d:'Memaksa DLAA aktif; anti-aliasing kualitas tinggi. Sangat berat di GPU.', w:'Aktif akan menonaktifkan DLSS Quality/Balanced/Performance di game.', t:'toggle', v:1, pw:6},
    ]},
    { id:'lumen', title:'RAY TRACING & LUMEN', s:'RendererSettings', items:[
        {k:'r.Lumen.ScreenProbeGather.Temporal.DistanceThreshold', l:'Lumen Temporal Threshold', d:'Nilai rendah = ghosting lebih sedikit, tetapi flickering lebih banyak.', t:'slider', v:0.03, min:0.01, max:0.10, step:0.01, pw:2, pi:true},
        {k:'r.Lumen.Reflections.AsyncCompute', l:'Async Compute (Reflections)', d:'Aktif dapat meningkatkan FPS pada GPU modern.', t:'toggle', v:1, pw:0},
        {k:'r.Lumen.Reflections.ScreenTraces', l:'Screen Traces', d:'Screen trace untuk refleksi Lumen.', w:'Mati memperbaiki bayangan kotak rusak, tetapi menghilangkan refleksi air/cermin.', t:'toggle', v:0, pw:3},
        {k:'r.Lumen.Reflections.SmoothBias', l:'Kilap refleksi', d:'0.0 → 1.0. Set 1.0 = permukaan berkilap seperti cermin.', t:'slider', v:1.0, min:0.0, max:1.0, step:0.1, pw:2},
    ]},
];

let _gfxValues = {};
let _gfxDefaults = {};
let _gfxBuilt = false;
let _gfxCacheHasData = false; // true once cache was successfully loaded from disk
let _gfxFileValues = null; // values loaded from disk (null = not loaded yet)

async function gfxInit() {
    const hint = document.getElementById('gfxConfigPath');
    if (hint) {
        hint.textContent = S.gamePath
            ? S.gamePath + '\\Client\\Saved\\Config\\WindowsNoEditor'
            : 'belum memilih folder game';
    }
    if (!_gfxBuilt) gfxBuild();
    await gfxLoadCache();
    if (S.gamePath && bridge()) await gfxLoadFromDisk();
}

async function gfxLoadCache() {
    if (!bridge()) return;
    try {
        const raw = await bridge().ReadGfxCache();
        if (!raw) return;
        const cached = JSON.parse(raw);
        let found = false;
        GFX_CATS.forEach(cat => cat.items.forEach(it => {
            if (cached.hasOwnProperty(it.k)) {
                _gfxValues[it.k] = cached[it.k];
                found = true;
            }
        }));
        if (found) {
            _gfxCacheHasData = true;
            gfxSyncDOM();
            gfxUpdatePerf();
            gfxUpdatePresetHighlight();
        }
    } catch(e) {}
}

async function gfxSaveCache() {
    if (!bridge()) return;
    try {
        const obj = {};
        GFX_CATS.forEach(cat => cat.items.forEach(it => {
            obj[it.k] = _gfxValues[it.k];
        }));
        await bridge().WriteGfxCache(JSON.stringify(obj));
    } catch(e) {}
}

function gfxBuild() {
    _gfxBuilt = true;
    const container = document.getElementById('gfxScroll');
    if (!container) return;
    container.innerHTML = '';
    GFX_CATS.forEach(cat => cat.items.forEach(it => {
        _gfxDefaults[it.k] = it.v;
        _gfxValues[it.k] = it.v;
    }));

    GFX_CATS.forEach(cat => {
        const catEl = document.createElement('div');
        catEl.className = 'gfx-cat';
        const head = document.createElement('div');
        head.className = 'gfx-cat__head';
        head.innerHTML = '<svg viewBox="0 0 24 24" width="10" height="10"><path fill="currentColor" d="M7 10l5 5 5-5z"/></svg>' + cat.title;
        head.addEventListener('click', () => catEl.classList.toggle('collapsed'));
        catEl.appendChild(head);

        const body = document.createElement('div');
        body.className = 'gfx-cat__body';

        cat.items.forEach(it => {
            const row = document.createElement('div');
            row.className = 'gfx-row';
            const info = document.createElement('div');
            info.className = 'gfx-row__info';
            let html = `<div class="gfx-row__label">${it.l}<span class="gfx-row__key">${it.k}</span></div>`;
            html += `<div class="gfx-row__desc">${it.d}</div>`;
            if (it.w) html += `<div class="gfx-row__warn">${it.w}</div>`;
            info.innerHTML = html;
            row.appendChild(info);
            const ctrl = document.createElement('div');
            ctrl.className = 'gfx-row__ctrl';
            ctrl.appendChild(gfxMakeCtrl(it));
            row.appendChild(ctrl);

            body.appendChild(row);
        });

        catEl.appendChild(body);
        container.appendChild(catEl);
    });

    gfxUpdatePerf();
    document.getElementById('gfxResetBtn')?.addEventListener('click', gfxReset);
    document.getElementById('gfxApplyBtn')?.addEventListener('click', gfxApply);
}

function gfxMakeCtrl(it) {
    const wrap = document.createElement('span');

    if (it.t === 'toggle' || it.t === 'toggleStr') {
        const label = document.createElement('label');
        label.className = 'gfx-toggle';
        const inp = document.createElement('input');
        inp.type = 'checkbox';
        inp.dataset.key = it.k;
        inp.dataset.type = it.t;
        if (it.t === 'toggleStr') {
            inp.checked = (String(it.v) === 'true');
        } else {
            inp.checked = !!it.v;
        }
        const track = document.createElement('span');
        track.className = 'gfx-toggle__track';
        const valSpan = document.createElement('span');
        valSpan.className = 'gfx-toggle__label';
        valSpan.textContent = inp.checked ? 'ON' : 'OFF';
        inp.addEventListener('change', () => {
            if (it.t === 'toggleStr') {
                _gfxValues[it.k] = inp.checked ? 'true' : 'false';
            } else {
                _gfxValues[it.k] = inp.checked ? 1 : 0;
            }
            valSpan.textContent = inp.checked ? 'ON' : 'OFF';
            gfxOnChange();
        });
        label.appendChild(inp);
        label.appendChild(track);
        label.appendChild(valSpan);
        wrap.appendChild(label);

    } else if (it.t === 'slider') {
        const sw = document.createElement('div');
        sw.className = 'gfx-slider-wrap';
        const range = document.createElement('input');
        range.type = 'range';
        range.className = 'gfx-slider';
        range.min = it.min; range.max = it.max; range.step = it.step;
        range.value = it.v;
        range.dataset.key = it.k;
        const val = document.createElement('span');
        val.className = 'gfx-slider__val';
        val.textContent = gfxFmt(it.v, it.step);
        range.addEventListener('input', () => {
            const n = parseFloat(range.value);
            _gfxValues[it.k] = n;
            val.textContent = gfxFmt(n, it.step);
            gfxOnChange();
        });
        sw.appendChild(range);
        sw.appendChild(val);
        wrap.appendChild(sw);

    } else if (it.t === 'select') {
        const sel = document.createElement('select');
        sel.className = 'gfx-select';
        sel.dataset.key = it.k;
        it.opts.forEach(o => {
            const opt = document.createElement('option');
            opt.value = o.v;
            opt.textContent = o.l;
            if (o.v === it.v) opt.selected = true;
            sel.appendChild(opt);
        });
        sel.addEventListener('change', () => {
            _gfxValues[it.k] = gfxParseNum(sel.value);
            gfxOnChange();
        });
        wrap.appendChild(sel);

    } else if (it.t === 'input') {
        const inp = document.createElement('input');
        inp.type = 'number';
        inp.className = 'gfx-input';
        inp.dataset.key = it.k;
        inp.min = it.min; inp.max = it.max;
        inp.value = it.v;
        inp.addEventListener('change', () => {
            let n = parseFloat(inp.value);
            if (isNaN(n)) n = it.v;
            n = Math.max(it.min, Math.min(it.max, n));
            inp.value = n;
            _gfxValues[it.k] = n;
            gfxOnChange();
        });
        wrap.appendChild(inp);
    }

    return wrap;
}

function gfxFmt(v, step) {
    return step < 1 ? v.toFixed(2) : String(v);
}

function gfxParseNum(s) {
    const n = parseFloat(s);
    return isNaN(n) ? s : n;
}

function gfxOnChange() {
    gfxUpdatePerf();
    gfxSaveCache();
    const btn = document.getElementById('gfxApplyBtn');
    if (btn) btn.disabled = false;
}

function gfxNormItem(it) {
    const v = _gfxValues[it.k];
    let norm = 0;
    if (it.t === 'toggle') {
        norm = v ? 1 : 0;
    } else if (it.t === 'toggleStr') {
        norm = (String(v) === 'true') ? 1 : 0;
    } else if (it.t === 'slider' || it.t === 'input') {
        const range = (it.max ?? 1) - (it.min ?? 0);
        norm = range > 0 ? (v - (it.min ?? 0)) / range : 0;
    } else if (it.t === 'select' && it.opts) {
        const idx = it.opts.findIndex(o => o.v === v);
        norm = it.opts.length > 1 ? Math.max(0, idx) / (it.opts.length - 1) : 0;
    }
    if (it.pi) norm = 1 - norm;
    return Math.max(0, Math.min(1, norm));
}

function gfxCalcRawPct() {
    let totalCost = 0, totalWeight = 0;
    GFX_CATS.forEach(cat => cat.items.forEach(it => {
        if (!it.pw) return;
        totalCost += it.pw * gfxNormItem(it);
        totalWeight += it.pw;
    }));
    return totalWeight > 0 ? totalCost / totalWeight : 0;
}

function gfxUpdatePerf() {
    const raw = gfxCalcRawPct();
    const pct = raw * 100;           // already 0-1 normalised
    const display = Math.round(pct);
    const fill = document.getElementById('gfxPerfFill');
    const txt  = document.getElementById('gfxPerfPct');
    const hue = Math.max(0, 120 - pct * 0.9);
    const color = `hsl(${hue}, 72%, 55%)`;
    if (fill) {
        fill.style.width = Math.min(100, pct) + '%';
        fill.style.background = `linear-gradient(90deg, hsl(${Math.min(120,hue+30)}, 72%, 45%), ${color})`;
        fill.style.boxShadow = `0 0 12px ${color.replace(')', ',0.45)')}`;
    }
    if (txt) {
        txt.textContent = display + '%';
        txt.style.color = color;
    }
}

function gfxReset() {
    GFX_CATS.forEach(cat => cat.items.forEach(it => {
        _gfxValues[it.k] = it.v;
    }));
    gfxSyncDOM();
    gfxOnChange();
}

async function gfxApply() {
    if (!S.gamePath) { toast('Belum memilih folder game!', 'err'); return; }
    if (!bridge()) { toast('Demo: tidak dapat menulis file.', 'info'); return; }
    const settings = {};
    GFX_CATS.forEach(cat => cat.items.forEach(it => {
        settings[it.k] = String(_gfxValues[it.k]);
    }));

    const result = await bridge().WriteEngineIni(S.gamePath, JSON.stringify(settings));

    if (result === 'ok') {
        _gfxFileValues = { ..._gfxValues };
        gfxSaveCache();
        toast('Engine.ini berhasil disimpan!', 'ok');
    } else if (result === 'not_found') {
        toast('Engine.ini tidak ditemukan. Masuk ke game minimal 1 kali agar file dibuat otomatis.', 'err');
    } else if (result === 'admin_required') {
        const ok = await showConfirm('Tidak memiliki izin menulis Engine.ini.\nMulai ulang Launcher sebagai Admin?');
        if (ok && bridge()) {
            await gfxSaveCache(); // ensure latest settings are persisted before restart
            bridge().RestartAsAdmin();
        }
    } else {
        toast('Gagal menulis file: ' + result, 'err');
    }
}

async function gfxLoadFromDisk() {
    if (!bridge()) return;
    try {
        const json = await bridge().ReadEngineIni(S.gamePath);
        const resp = JSON.parse(json);

        if (resp.status === 'not_found') {
            toast('Engine.ini tidak ditemukan. Masuk ke game 1 kali agar file dibuat.', 'info');
            return;
        }
        if (resp.status === 'error') {
            toast('Gagal membaca Engine.ini: ' + (resp.message || ''), 'err');
            return;
        }
        const hint = document.getElementById('gfxConfigPath');
        if (hint && resp.path) hint.textContent = resp.path;
        if (_gfxCacheHasData) return;
        const data = resp.data || {};
        _gfxFileValues = {};
        GFX_CATS.forEach(cat => cat.items.forEach(it => {
            const fileKey = Object.keys(data).find(k => k.toLowerCase() === it.k.toLowerCase());
            if (fileKey !== undefined && data[fileKey] !== undefined) {
                let val = data[fileKey];
                if (it.t === 'toggleStr') {
                    val = (val === 'true' || val === '1') ? 'true' : 'false';
                } else if (it.t === 'toggle') {
                    val = parseInt(val) ? 1 : 0;
                } else {
                    val = parseFloat(val);
                    if (isNaN(val)) val = it.v;
                }
                _gfxValues[it.k] = val;
                _gfxFileValues[it.k] = val;
            }
        }));

        gfxSyncDOM();
        gfxOnChange();
    } catch(e) {}
}

function gfxSyncDOM() {
    document.querySelectorAll('#gfxScroll [data-key]').forEach(el => {
        const k = el.dataset.key;
        const it = gfxFindItem(k);
        if (!it) return;
        const v = _gfxValues[it.k];
        if (el.type === 'checkbox') {
            el.checked = it.t === 'toggleStr' ? (String(v) === 'true') : !!v;
            const lbl = el.parentElement?.querySelector('.gfx-toggle__label');
            if (lbl) lbl.textContent = el.checked ? 'ON' : 'OFF';
        } else if (el.type === 'range') {
            el.value = v;
            const valSpan = el.parentElement?.querySelector('.gfx-slider__val');
            if (valSpan) valSpan.textContent = gfxFmt(v, it.step);
        } else if (el.tagName === 'SELECT') {
            el.value = v;
        } else if (el.type === 'number') {
            el.value = v;
        }
    });
}

function gfxFindItem(k) {
    for (const cat of GFX_CATS)
        for (const it of cat.items)
            if (it.k === k) return it;
    return null;
}
let _launcherUpdateVer = '';

function checkLauncherUpdate(silent = true) {
    if (!silent) toast('Memeriksa pembaruan Launcher...', 'info');
    if (bridge()) bridge().CheckLauncherUpdate();
}

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

function loadSettings() {
    if (bridge()) {
        try {
            const j = bridge().LoadSettings();
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

