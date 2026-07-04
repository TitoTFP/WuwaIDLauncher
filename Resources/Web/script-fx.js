
function initParticles() {
    const c = document.getElementById('particleCanvas');
    if (!c) return;
    const ctx = c.getContext('2d');
    let W, H;
    const P = [];
    const N = 20;
    const INTERVAL = 1000 / 24;
    let lastT = 0;

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
        }
    }
    for (let i=0; i<N; i++) P.push(new Dot());
    (function loop(ts) {
        if (!document.hidden) {
            if (ts - lastT >= INTERVAL) {
                lastT = ts;
                ctx.clearRect(0,0,W,H);
                P.forEach(p => { p.tick(); p.draw(ctx); });
            }
        }
        requestAnimationFrame(loop);
    })(0);
}

let navWaveT = 0;
let _indCurL = 0, _indCurW = 0;
let _indTgtL = 0, _indTgtW = 0;
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

    const cL = _indCurL;
    const cW = _indCurW;
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

    drawArc(-1);
    drawArc(+1);
}

function initCyberEffects() {
    initGlitchEffect();
    initAudioVisualizer();
    initWaterEffect();
}

function initGlitchEffect() {
    const btn = document.getElementById('btnStart');
    if (!btn) return;
    setInterval(() => {
        if (Math.random() < 0.05) {
            btn.classList.add('glitch-active');
            setTimeout(() => btn.classList.remove('glitch-active'), 400);
        }
    }, 3000);
}

function getE(d, a, b) {
    let s = 0;
    for (let i = a; i < b && i < d.length; i++) s += d[i];
    return s / ((b - a) * 255);
}

function getPeak(d, a, b) {
    let mx = 0;
    for (let i = a; i < b && i < d.length; i++) {
        if (d[i] > mx) mx = d[i];
    }
    return mx / 255;
}

let _audioCtx, _analyser, _srcNode;
function initAudioVisualizer() {
    const audio = document.getElementById('bgMusic');
    const vizCanvas = document.getElementById('audioViz');
    const stageContainer = document.getElementById('stageLights');
    if (!audio || !vizCanvas) return;

    const VIZ_H = 180;
    let ctx = null, clouds = [];

    function makeCloudSprite(w, h) {
        const cv = document.createElement('canvas');
        cv.width = Math.ceil(w); cv.height = Math.ceil(h);
        const c = cv.getContext('2d');
        const puffs = 7 + Math.floor(Math.random() * 6);
        for (let i = 0; i < puffs; i++) {
            const r  = h * (0.22 + Math.random() * 0.34);
            const px = r + Math.random() * (w - 2 * r);
            const py = h * 0.55 + (Math.random() - 0.5) * h * 0.4;
            const g  = c.createRadialGradient(px, py, 0, px, py, r);
            g.addColorStop(0.0, 'rgba(255,252,242,0.42)');
            g.addColorStop(0.45,'rgba(246,228,188,0.22)');
            g.addColorStop(1.0, 'rgba(232,204,150,0)');
            c.fillStyle = g;
            c.beginPath(); c.arc(px, py, r, 0, Math.PI * 2); c.fill();
        }

        c.globalCompositeOperation = 'lighter';
        for (let i = 0; i < 3; i++) {
            const r  = h * (0.14 + Math.random() * 0.18);
            const px = r + Math.random() * (w - 2 * r);
            const py = h * 0.45 + (Math.random() - 0.5) * h * 0.3;
            const g  = c.createRadialGradient(px, py, 0, px, py, r);
            g.addColorStop(0, 'rgba(255,250,235,0.26)');
            g.addColorStop(1, 'rgba(255,250,235,0)');
            c.fillStyle = g;
            c.beginPath(); c.arc(px, py, r, 0, Math.PI * 2); c.fill();
        }
        return cv;
    }

    function initCanvas() {
        vizCanvas.width  = window.innerWidth;
        vizCanvas.height = VIZ_H;
        ctx = vizCanvas.getContext('2d');
        const W = vizCanvas.width;
        const count = Math.max(5, Math.round(W / 360));
        clouds = [];
        for (let i = 0; i < count; i++) {
            const depth = Math.random();
            const h = VIZ_H * (0.5 + depth * 0.6);
            const w = h * (2.2 + Math.random() * 1.6);
            clouds.push({
                sprite: makeCloudSprite(w, h),
                w, h,
                x: Math.random() * (W + w) - w,
                y: -h * 0.30 + Math.random() * (VIZ_H - h * 0.6),
                speed: 0.15 + depth * 0.55,
                alpha: 0.35 + depth * 0.45,
            });
        }
        clouds.sort((a, b) => a.alpha - b.alpha);
    }
    initCanvas();
    window.addEventListener('resize', initCanvas);

    const lights = [];
    if (stageContainer) {
        stageContainer.innerHTML = '';
        [20, 50, 80].forEach(pos => {
            const light = document.createElement('div');
            light.className = 'stage-light';
            light.style.left = pos + '%';
            stageContainer.appendChild(light);
            lights.push(light);
        });
    }

    let _vizRunning = false;
    const VIZ_INTERVAL = 1000 / 30;
    let _vizLastT = 0;
    const bgLayer = document.querySelector('.bg-layer');
    let _bgScale = 1.0;
    let _bgVelocity = 0;
    let _fftData = null;
    let bassE = 0, midE = 0, hiE = 0, overallE = 0;

    const vizLoop = (ts) => {
        if (document.hidden) { requestAnimationFrame(vizLoop); return; }
        if (ts - _vizLastT < VIZ_INTERVAL) { requestAnimationFrame(vizLoop); return; }
        _vizLastT = ts;

        const hasAudio = _vizRunning && _analyser;
        if (hasAudio) {
            if (!_fftData) _fftData = new Uint8Array(_analyser.frequencyBinCount);
            _analyser.getByteFrequencyData(_fftData);

            const dataLen = _fftData.length;

            const rBass = getE(_fftData, 0, 12);
            const rMid  = getE(_fftData, 12, 100);
            const rHi   = getE(_fftData, 100, 320);
            const rAll  = getE(_fftData, 0, 360);
            bassE    += (rBass - bassE)    * 0.20;
            midE     += (rMid  - midE)     * 0.15;
            hiE      += (rHi   - hiE)      * 0.12;
            overallE += (rAll  - overallE) * 0.10;

            const op = (0.1 + bassE * 0.70).toFixed(2);
            lights.forEach(l => { l.style.opacity = op; });

        } else {

            bassE *= 0.92; midE *= 0.92; hiE *= 0.92; overallE *= 0.92;
            lights.forEach(l => { l.style.opacity = (0.1 + bassE * 0.70).toFixed(2); });
        }

        if (!ctx) { requestAnimationFrame(vizLoop); return; }
        const W = vizCanvas.width, H = vizCanvas.height;
        ctx.clearRect(0, 0, W, H);

        const drift = 1 + overallE * 0.8;
        const glow  = 0.78 + overallE * 0.5;
        for (let i = 0; i < clouds.length; i++) {
            const cl = clouds[i];
            cl.x += cl.speed * drift;
            if (cl.x > W + cl.w) {
                cl.x = -cl.w;
                cl.y = -cl.h * 0.30 + Math.random() * (H - cl.h * 0.6);
            }
            ctx.globalAlpha = Math.min(1, cl.alpha * glow);
            ctx.drawImage(cl.sprite, cl.x, cl.y);
        }
        ctx.globalAlpha = 1;

        requestAnimationFrame(vizLoop);
    };
    requestAnimationFrame(vizLoop);

    const start = () => {
        try {
            if (!_audioCtx) {
                _audioCtx = new (window.AudioContext || window.webkitAudioContext)();
                _analyser = _audioCtx.createAnalyser();
                _analyser.fftSize = 2048;
                _analyser.smoothingTimeConstant = 0.15;
                _srcNode = _audioCtx.createMediaElementSource(audio);
                _srcNode.connect(_analyser);
                _analyser.connect(_audioCtx.destination);
            }
            _vizRunning = true;
        } catch (e) { console.warn('Viz failed:', e); }
    };

    audio.addEventListener('play', () => {
        if (_audioCtx?.state === 'suspended') _audioCtx.resume();
        if (!_vizRunning) start();
        else _vizRunning = true;
    });
    audio.addEventListener('pause', () => { _vizRunning = false; });
    document.addEventListener('visibilitychange', () => {
        if (!document.hidden && !audio.paused && _audioCtx) _vizRunning = true;
    });
}

function initWaterEffect() {
    const canvas = document.getElementById('waterFx');
    if (!canvas) return;

    const WATER_H = 150;
    let ctx = null, W = 0, H = 0;

    function build() {
        canvas.width  = window.innerWidth;
        canvas.height = WATER_H;
        ctx = canvas.getContext('2d');
        W = canvas.width; H = canvas.height;
    }
    build();
    window.addEventListener('resize', build);

    const layers = [
        { amp: 3, len: 230, speed: 0.5, yf: 0.16, col: 'rgba(244,212,138,', lw: 1.3, a: 0.30, phase: 0.0 },
        { amp: 5, len: 330, speed: 0.8, yf: 0.34, col: 'rgba(255,246,222,', lw: 1.6, a: 0.22, phase: 1.4 },
        { amp: 7, len: 470, speed: 1.2, yf: 0.54, col: 'rgba(212,176,108,', lw: 2.1, a: 0.18, phase: 3.1 },
        { amp: 9, len: 640, speed: 1.7, yf: 0.74, col: 'rgba(255,250,235,', lw: 2.5, a: 0.13, phase: 0.7 },
    ];

    let t = 0;
    const waterLoop = () => {
        if (document.hidden || !ctx) { requestAnimationFrame(waterLoop); return; }
        ctx.clearRect(0, 0, W, H);

        const bg = ctx.createLinearGradient(0, 0, 0, H);
        bg.addColorStop(0.0, 'rgba(10,16,46,0)');
        bg.addColorStop(0.5, 'rgba(14,22,62,0.30)');
        bg.addColorStop(1.0, 'rgba(22,34,86,0.58)');
        ctx.fillStyle = bg;
        ctx.fillRect(0, 0, W, H);

        ctx.globalCompositeOperation = 'lighter';

        const cols = Math.max(4, Math.round(W / 260));
        for (let i = 0; i < cols; i++) {
            const baseX = (((i / cols) * W) + t * 0.6) % (W + 120) - 60;
            const x = baseX + Math.sin(t * 0.02 + i) * 14;
            const g = ctx.createLinearGradient(x - 32, 0, x + 32, 0);
            g.addColorStop(0.0, 'rgba(246,224,170,0)');
            g.addColorStop(0.5, 'rgba(246,224,170,0.07)');
            g.addColorStop(1.0, 'rgba(246,224,170,0)');
            ctx.fillStyle = g;
            ctx.fillRect(x - 32, H * 0.12, 64, H * 0.88);
        }

        for (const L of layers) {
            L.phase += L.speed;
            const y0 = H * L.yf;
            ctx.beginPath();
            for (let x = 0; x <= W; x += 6) {
                const y = y0
                    + Math.sin((x - L.phase) / L.len * Math.PI * 2) * L.amp
                    + Math.sin((x * 0.5 - L.phase * 0.7) / L.len * Math.PI * 2) * L.amp * 0.4;
                if (x === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
            }
            ctx.strokeStyle = L.col + L.a + ')';
            ctx.lineWidth = L.lw;
            ctx.stroke();
        }

        ctx.globalCompositeOperation = 'source-over';
        t += 1;
        requestAnimationFrame(waterLoop);
    };
    requestAnimationFrame(waterLoop);
}
