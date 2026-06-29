

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
    initCyberEffects();
    initFontCreator();
    initPerformanceMode();
    initSidePanel();
    loadSettings();
    loadVersions();
    loadReleaseNotes();
});
