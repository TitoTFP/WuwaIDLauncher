

const S = {
    page: 'home',
    installing: false, installed: false, launching: false,
    gamePath: '',
    patchState: 'unchecked',
    cfg: { gamePath:'', installMethod:'method1', launcherVisualMode:'full' },
    autoCheckDone: false
};

const bridge = () => window.chrome?.webview?.hostObjects?.launcher;

document.addEventListener('DOMContentLoaded', async () => {
    initTopBar();
    initTopNav();
    initBottomBar();
    initAudioPlayer();
    initPerformanceMode();
    initSidePanel();
    initLogUpload();
    await loadSettings();
    applyLauncherVisualMode(S.cfg.launcherVisualMode);
    initParticles();
    initWaterRipple();
    initCyberEffects();
    loadVersions();
    bridge()?.NotifyUiInteractive();
});
