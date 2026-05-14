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
    document.getElementById('pagePerformance').style.display   = page === 'performance'  ? '' : 'none';
    document.getElementById('pageFontCreator').style.display   = page === 'font-creator' ? '' : 'none';
    
    const ap = document.getElementById('audioPlayer');
    const sp = document.getElementById('sidePanel');
    if (ap) ap.style.display = isHome ? '' : 'none';
    if (sp && sp.dataset.loaded) sp.style.display = isHome ? 'flex' : 'none';
    updateNavIndicator();
    if (page === 'performance') { pmRefreshStatus(); }
    if (page === 'font-creator') { fcRefreshStatus(); checkAdminIfNeeded(); }
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

