

const PM_TOGGLES = [
    { id: 'pmShadows',         key: 'shadows'         },
    { id: 'pmSsr',             key: 'ssr'             },
    { id: 'pmAo',              key: 'ao'              },
    { id: 'pmBloom',           key: 'bloom'           },
    { id: 'pmLensFlare',       key: 'lensFlare'       },
    { id: 'pmDof',             key: 'dof'             },
    { id: 'pmMaterials',       key: 'materials'       },
    { id: 'pmSss',             key: 'sss'             },
    { id: 'pmViewDist',        key: 'viewDist'        },
    { id: 'pmFoliage',         key: 'foliage'         },
    { id: 'pmFoliageInteract', key: 'foliageInteract' },
    { id: 'pmParticles',       key: 'particles'       },
    { id: 'pmClouds',          key: 'clouds'          },
    { id: 'pmVolumetric',      key: 'volumetric'      },
];

function initPerformanceMode() {
    document.getElementById('pmApplyBtn')?.addEventListener('click', pmApply);
    document.getElementById('pmClearBtn')?.addEventListener('click', pmClear);
}

function pmLoadToggles() {
    const perf = S.cfg.perf || {};
    PM_TOGGLES.forEach(({ id, key }) => {
        const el = document.getElementById(id);
        if (el) el.checked = !!perf[key];
    });
}

async function pmRefreshStatus() {
    pmLoadToggles();
    if (!S.gamePath || !bridge()) {
        pmSetStatus('inactive', 'Belum memilih folder game');
        return;
    }
    try {
        const active = await bridge().GetPerformanceConfigActive(S.gamePath);
        pmSetStatus(active ? 'active' : 'inactive', active ? 'Aktif' : 'Belum diterapkan');
    } catch(e) {
        pmSetStatus('inactive', 'Belum diterapkan');
    }
}

function pmSetStatus(state, text) {
    const dot = document.getElementById('pmStatusDot');
    const txt = document.getElementById('pmStatusText');
    if (dot) dot.className = 'pm-status__dot' + (state === 'active' ? ' pm-status__dot--active' : '');
    if (txt) txt.textContent = text;
}

async function pmApply() {
    if (!S.gamePath) { toast('Belum memilih folder game!', 'err'); return; }

    const settings = {};
    PM_TOGGLES.forEach(({ id, key }) => {
        const el = document.getElementById(id);
        settings[key] = el ? el.checked : false;
    });

    const anyEnabled = Object.values(settings).some(Boolean);
    if (!anyEnabled) { toast('Belum ada efek yang diaktifkan untuk dioptimalkan.', 'info'); return; }

    S.cfg.perf = settings;
    saveSettings();

    if (!bridge()) { toast('Demo: konfigurasi performa tersimpan', 'ok'); return; }

    const btn = document.getElementById('pmApplyBtn');
    if (btn) { btn.disabled = true; btn.textContent = 'Menulis...'; }

    try {
        const result = await bridge().ApplyPerformanceConfig(S.gamePath, JSON.stringify(settings));
        if (result === 'ok') {
            toast('Diterapkan! Mulai ulang game agar berlaku.', 'ok');
            pmSetStatus('active', 'Aktif');
        } else {
            toast('Error: ' + result, 'err');
        }
    } catch(e) {
        toast('Gagal menulis config: ' + e, 'err');
    } finally {
        if (btn) { btn.disabled = false; btn.textContent = 'Terapkan & Simpan'; }
    }
}

async function pmClear() {
    if (!S.gamePath) { toast('Belum memilih folder game!', 'err'); return; }

    if (!bridge()) { toast('Demo: konfigurasi performa dihapus', 'info'); return; }

    const btn = document.getElementById('pmClearBtn');
    if (btn) { btn.disabled = true; btn.textContent = 'Menghapus...'; }

    try {
        const result = await bridge().ClearPerformanceConfig(S.gamePath);
        if (result === 'ok') {
            toast('Config dihapus. Game akan memakai setelan default.', 'ok');
            pmSetStatus('inactive', 'Belum diterapkan');
            S.cfg.perf = {};
            saveSettings();
            PM_TOGGLES.forEach(({ id }) => {
                const el = document.getElementById(id);
                if (el) el.checked = false;
            });
        } else {
            toast('Error: ' + result, 'err');
        }
    } catch(e) {
        toast('Error: ' + e, 'err');
    } finally {
        if (btn) { btn.disabled = false; btn.textContent = 'Hapus config'; }
    }
}
