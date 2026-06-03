
function initParticles() {
    const c = document.getElementById('particleCanvas');
    if (!c) return;
    const ctx = c.getContext('2d');
    let W, H;
    const P = [];
    const N = 20; // reduced from 35
    const INTERVAL = 1000 / 24; // cap at 24fps
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

function initCyberEffects() {
    initGlitchEffect();
    initAudioVisualizer();
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

let _audioCtx, _analyser, _srcNode;
function initAudioVisualizer() {
    const audio = document.getElementById('bgMusic');
    const vizCanvas = document.getElementById('audioViz');
    const stageContainer = document.getElementById('stageLights');
    if (!audio || !vizCanvas) return;

    // --- Matrix Rain Setup ---
    const RAIN_CHARS = 'ｦｧｨｩｪｫｬｭｮｯｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝ0123456789ABCDEF';
    const EASTER_EGGS = ['LUCY','REBECCA','DAVID','JOHNNY','ROVER','AEMEATH','SHOREKEEPER'];
    const COL_W = 11, CHAR_H = 11, TRAIL = 14;
    let ctx = null, rainCols = [];

    function initCanvas() {
        vizCanvas.width  = window.innerWidth;
        vizCanvas.height = 180;
        ctx = vizCanvas.getContext('2d');
        ctx.font = (CHAR_H - 1) + 'px Consolas';
        ctx.textBaseline = 'top';
        const count = Math.floor(vizCanvas.width / COL_W);
        rainCols = [];
        for (let i = 0; i < count; i++) {
            const hasEgg = Math.random() < 0.18;
            rainCols.push({
                y:      -Math.random() * 180,
                speed:  0.3 + Math.random() * 0.7,
                chars:  Array.from({length: TRAIL}, () => RAIN_CHARS[Math.floor(Math.random() * RAIN_CHARS.length)]),
                charT:  0,
                egg:    hasEgg ? EASTER_EGGS[Math.floor(Math.random() * EASTER_EGGS.length)] : null,
                eggOff: Math.floor(Math.random() * (TRAIL - 9)),
            });
        }
    }
    initCanvas();
    window.addEventListener('resize', initCanvas);

    // Stage lights: 3 lights only
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
    let _fftData = null;

    const vizLoop = (ts) => {
        if (!_vizRunning || document.hidden) return;
        if (ts - _vizLastT < VIZ_INTERVAL) { requestAnimationFrame(vizLoop); return; }
        _vizLastT = ts;

        if (!_fftData) _fftData = new Uint8Array(_analyser.frequencyBinCount);
        _analyser.getByteFrequencyData(_fftData);

        const bass    = _fftData[2] || 0;
        const dataLen = _fftData.length;

        // Stage lights
        const op = (0.1 + (bass * 0.00275)).toFixed(2);
        lights.forEach(l => { l.style.opacity = op; });

        // Background zoom
        if (bgLayer) {
            const target = 1.0 + (bass * 0.000549);
            _bgScale += (target - _bgScale) * (target > _bgScale ? 0.55 : 0.45);
            bgLayer.style.transform = 'scale(' + _bgScale.toFixed(4) + ')';
        }

        // --- Matrix Rain Draw ---
        if (!ctx) { requestAnimationFrame(vizLoop); return; }
        ctx.clearRect(0, 0, vizCanvas.width, vizCanvas.height);

        const colCount = rainCols.length;
        for (let i = 0; i < colCount; i++) {
            const col = rainCols[i];
            const freqIdx = Math.floor((i / colCount) * dataLen);
            const freq    = _fftData[freqIdx] || 0;
            const energy  = freq / 255;

            // Speed reacts to frequency
            col.y += col.speed + energy * 4;
            if (col.y > vizCanvas.height + TRAIL * CHAR_H) {
                col.y = -CHAR_H;
                col.egg    = Math.random() < 0.18 ? EASTER_EGGS[Math.floor(Math.random() * EASTER_EGGS.length)] : null;
                col.eggOff = Math.floor(Math.random() * (TRAIL - 9));
            }

            // Randomly mutate characters
            col.charT++;
            if (col.charT > 2) {
                col.charT = 0;
                col.chars[Math.floor(Math.random() * TRAIL)] = RAIN_CHARS[Math.floor(Math.random() * RAIN_CHARS.length)];
            }

            const x = i * COL_W;
            for (let t = 0; t < TRAIL; t++) {
                const cy = col.y - t * CHAR_H;
                if (cy < -CHAR_H || cy > vizCanvas.height) continue;

                const trailFade = (1 - t / TRAIL);
                const alpha = trailFade * (0.65 + energy * 0.35);

                // Easter egg chars render gold
                if (col.egg) {
                    const eggIdx = (col.eggOff + col.egg.length - 1) - t;
                    if (eggIdx >= 0 && eggIdx < col.egg.length) {
                        ctx.fillStyle = 'rgba(252,238,9,' + alpha.toFixed(2) + ')';
                        ctx.fillText(col.egg[eggIdx], x, cy);
                        continue;
                    }
                }

                if (t === 0) {
                    // Head: bright white-cyan
                    ctx.fillStyle = 'rgba(200,255,255,' + Math.min(1, alpha * 1.6).toFixed(2) + ')';
                } else {
                    ctx.fillStyle = 'rgba(0,240,200,' + alpha.toFixed(2) + ')';
                }
                ctx.fillText(col.chars[t], x, cy);
            }
        }

        requestAnimationFrame(vizLoop);
    };

    const start = () => {
        try {
            if (!_audioCtx) {
                _audioCtx = new (window.AudioContext || window.webkitAudioContext)();
                _analyser = _audioCtx.createAnalyser();
                _analyser.fftSize = 128;
                _srcNode = _audioCtx.createMediaElementSource(audio);
                _srcNode.connect(_analyser);
                _analyser.connect(_audioCtx.destination);
            }
            _vizRunning = true;
            requestAnimationFrame(vizLoop);
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

