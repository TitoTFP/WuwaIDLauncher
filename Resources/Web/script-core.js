

const S = {
    page: 'home',
    installing: false, installed: false, launching: false,
    gamePath: '',
    patchState: 'unchecked',
    cfg: { gamePath:'', installMethod:'method1', launcherVisualMode:'full' },
    autoCheckDone: false,
    gameRunning: false,
    gameOrigin: 'external',
    runtimeReady: false
};

const bridge = () => window.chrome?.webview?.hostObjects?.launcher;

window.onGameRuntimeState = (active, origin) => {
    const wasRunning = S.gameRunning;
    const wasExternal = S.gameRunning && S.gameOrigin === 'external';
    S.gameRunning = !!active;
    S.gameOrigin = origin === 'launcher' ? 'launcher' : 'external';
    document.body?.classList.toggle(
        'game-runtime-readonly',
        S.gameRunning && S.gameOrigin === 'external');
    window.applyEffectiveLauncherVisualMode?.();
    window.apGameRuntime?.(S.gameRunning);
    window.setLauncherEffectsRuntime?.(S.gameRunning);

    if (S.gameRunning && S.gameOrigin === 'external') {
        setLaunchLock?.('Game eksternal sedang berjalan');
    } else if (!S.gameRunning && wasExternal) {
        clearLaunchLock?.();
    }

    if (!S.gameRunning && S.runtimeReady) {
        window.initializeLauncherEffects?.();
        window.startInitialPatchCheck?.(wasRunning);
    }
};

function installRuntimeReadOnlyGuard() {
    const allowed = target => target instanceof Element &&
        !!target.closest('.top-nav__item[data-page], #btnMinimize, #btnClose, #rnToggle');
    const block = event => {
        if (!S.gameRunning || S.gameOrigin !== 'external' || allowed(event.target)) return;
        event.preventDefault();
        event.stopImmediatePropagation();
    };
    ['click', 'input', 'change', 'submit'].forEach(type =>
        document.addEventListener(type, block, true));
}

document.addEventListener('DOMContentLoaded', async () => {
    installRuntimeReadOnlyGuard();
    initTopBar();
    initTopNav();
    initBottomBar();
    initAudioPlayer();
    initPerformanceMode();
    initSidePanel();
    initLogUpload();
    await loadSettings();
    S.runtimeReady = true;
    let gameRunning = false;
    try { gameRunning = !!(await bridge()?.IsGameRunning()); } catch {}
    window.onGameRuntimeState(gameRunning, 'external');
    applyLauncherVisualMode(S.cfg.launcherVisualMode);
    if (!S.gameRunning) window.initializeLauncherEffects?.();
    loadVersions();
    bridge()?.NotifyUiInteractive();
    if (!S.gameRunning) window.startInitialPatchCheck?.();
});
