
function initPerformanceMode() {
    document.getElementById('pmEnabled')?.addEventListener('change', pmOnToggle);
    document.getElementById('pmShadow')?.addEventListener('change', pmOnShadowChange);
}

async function pmRefreshStatus() {
    const perf = S.cfg.perf || {};
    const elEnabled = document.getElementById('pmEnabled');
    const elShadow  = document.getElementById('pmShadow');
    if (elEnabled) elEnabled.checked = !!perf.enabled;
    if (elShadow)  elShadow.checked  = !!perf.shadow;

    if (!S.gamePath || !bridge()) {
        pmSetStatus('inactive', 'Chưa bật');
        return;
    }
    try {
        const active = await bridge().GetPerformanceModeActive(S.gamePath);
        pmSetStatus(active ? 'active' : 'inactive', active ? 'Đang hoạt động' : 'Chưa bật');
        if (elEnabled) elEnabled.checked = active;
        S.cfg.perf = { ...perf, enabled: active };
    } catch (e) {
        pmSetStatus('inactive', 'Chưa bật');
    }
}

function pmSetStatus(state, text) {
    const dot = document.getElementById('pmStatusDot');
    const txt = document.getElementById('pmStatusText');
    if (dot) dot.className = 'pm-status__dot' + (state === 'active' ? ' pm-status__dot--active' : '');
    if (txt) txt.textContent = text;
}

async function pmOnToggle() {
    const shadow = document.getElementById('pmShadow')?.checked ?? false;
    if (document.getElementById('pmEnabled')?.checked) {
        await pmDoApply(shadow);
    } else {
        await pmDoClear();
    }
}

async function pmOnShadowChange() {
    const enabled = document.getElementById('pmEnabled')?.checked ?? false;
    const shadow  = document.getElementById('pmShadow')?.checked  ?? false;
    S.cfg.perf = { enabled, shadow };
    saveSettings();
    if (enabled && S.gamePath && bridge()) await pmDoApply(shadow);
}

async function pmDoApply(shadow) {
    if (!S.gamePath) { toast('Chưa chọn thư mục game!', 'err'); return; }
    S.cfg.perf = { enabled: true, shadow };
    saveSettings();
    if (!bridge()) { pmSetStatus('active', 'Đang hoạt động'); return; }
    try {
        const result = await bridge().ApplyPerformanceMode(S.gamePath, shadow);
        if (result === 'ok') {
            pmSetStatus('active', 'Đang hoạt động');
            toast('Đã bật. Khởi động lại game để có hiệu lực.', 'ok');
        } else {
            toast('Lỗi: ' + result, 'err');
            const el = document.getElementById('pmEnabled');
            if (el) el.checked = false;
            S.cfg.perf = { enabled: false, shadow };
            saveSettings();
        }
    } catch (e) { toast('Lỗi: ' + e, 'err'); }
}

async function pmDoClear() {
    if (!S.gamePath) { toast('Chưa chọn thư mục game!', 'err'); return; }
    const shadow = document.getElementById('pmShadow')?.checked ?? false;
    S.cfg.perf = { enabled: false, shadow };
    saveSettings();
    if (!bridge()) { pmSetStatus('inactive', 'Chưa bật'); return; }
    try {
        const result = await bridge().ClearPerformanceMode(S.gamePath);
        if (result === 'ok') {
            pmSetStatus('inactive', 'Chưa bật');
            toast('Đã tắt.', 'ok');
        } else {
            toast('Lỗi: ' + result, 'err');
            const el = document.getElementById('pmEnabled');
            if (el) el.checked = true;
            S.cfg.perf = { enabled: true, shadow };
            saveSettings();
        }
    } catch (e) { toast('Lỗi: ' + e, 'err'); }
}
