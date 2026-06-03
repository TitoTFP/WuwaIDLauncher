

const S = {
    page: 'home',
    installing: false, installed: false, launching: false,
    gamePath: '',
    cfg: { gamePath:'', installMethod:'method1' },
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
<<<<<<< HEAD
=======
    initCyberEffects();
    initFontCreator();
>>>>>>> 417b118e62c8690bcf4dfd714ce34651d12cf70f
    initPerformanceMode();
    initSidePanel();
    initLogUpload();
    loadSettings();
    loadVersions();
    loadReleaseNotes();
});
