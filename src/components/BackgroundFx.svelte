<script lang="ts">
  import { onMount } from 'svelte';
  import { appState } from '../lib/launcherState.svelte';
  import { themeRuntime } from '../lib/themeRuntime.svelte';

  type ParticleTint = 'gold' | 'cyan';

  interface Particle {
    x: number;
    y: number;
    radius: number;
    vx: number;
    vy: number;
    alpha: number;
    dAlpha: number;
    tint: ParticleTint;
    r: number;
    g: number;
    b: number;
  }

  let particles = $state.raw<Particle[]>([]);
  let particlesMounted = $state(false);
  // Non-reactive on purpose: the effect below is what refreshes it, so the
  // animation loop reads a plain array instead of a proxy.
  let particlePalette = themeRuntime.particlePalette();

  let videoElement: HTMLVideoElement | null = $state(null);
  let canvasElement: HTMLCanvasElement | null = $state(null);
  let videoLoaded = $state(false);
  let particleAnimationControl: {
    schedule: () => void;
    stop: () => void;
  } | null = null;

  $effect(() => {
    // Reading the revision subscribes this effect to every applied theme. One
    // read per theme, not two per particle: the loop below and `resetParticle`
    // both consume the cached palette.
    const revision = themeRuntime.revision;
    particlePalette = themeRuntime.particlePalette();
    if (!particlesMounted || revision === 0) return;
    for (const p of particles) {
      const [r, g, b] = p.tint === 'gold' ? particlePalette.gold : particlePalette.cyan;
      p.r = r;
      p.g = g;
      p.b = b;
    }
  });

  let isVideoAllowed = $derived(!appState.launcherInTray);
  // The theme background sits behind the synced video as its poster and
  // fallback: it shows while the video loads, whenever the video is blocked,
  // and whenever media is unavailable entirely.
  let backgroundPoster = $derived(themeRuntime.backgroundUrl());

  $effect(() => {
    if (!videoElement) return;

    if (!appState.videoUrl || !isVideoAllowed) {
      videoElement.pause();
      videoElement.removeAttribute('src');
      videoElement.load();
      videoLoaded = false;
      return;
    }

    if (appState.videoUrl && isVideoAllowed) {
      if (videoElement.src !== appState.videoUrl) {
        videoElement.src = appState.videoUrl;
        videoElement.load();
      }
      void videoElement.play().catch(() => {});
    }
  });

  $effect(() => {
    const runtimeBlocked = appState.launcherInTray;
    if (!particleAnimationControl) return;
    if (runtimeBlocked) particleAnimationControl.stop();
    else particleAnimationControl.schedule();
  });

  function handleVideoPlaying() {
    videoLoaded = true;
  }

  function handleVideoError() {
    videoLoaded = false;
    appState.videoUrl = '';
    appState.setStatus('Video latar tidak dapat diputar. Launcher tetap dapat digunakan.');
  }

  onMount(() => {
    if (!canvasElement) return;
    const canvas = canvasElement;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const context = ctx;

    let width = (canvas.width = window.innerWidth);
    let height = (canvas.height = window.innerHeight);

    const handleResize = () => {
      width = canvas.width = window.innerWidth;
      height = canvas.height = window.innerHeight;
    };
    window.addEventListener('resize', handleResize);

    const particleCount = 24;

    function resetParticle(p: Partial<Particle> = {}): Particle {
      const tint: ParticleTint = Math.random() > 0.4 ? 'gold' : 'cyan';
      const [r, g, b] = tint === 'gold' ? particlePalette.gold : particlePalette.cyan;
      return {
        x: p.x ?? Math.random() * width,
        y: p.y ?? Math.random() * height,
        radius: Math.random() * 1.8 + 0.5,
        vx: (Math.random() - 0.5) * 0.25,
        vy: -Math.random() * 0.35 - 0.08,
        alpha: Math.random() * 0.35 + 0.1,
        dAlpha: (Math.random() > 0.5 ? 1 : -1) * (Math.random() * 0.004 + 0.001),
        r,
        g,
        b,
        tint,
      };
    }

    particles = Array.from({ length: particleCount }, () => resetParticle());
    particlesMounted = true;

    let animFrameId: number | null = null;

    function stopRender() {
      if (animFrameId !== null) {
        cancelAnimationFrame(animFrameId);
        animFrameId = null;
      }
      context.clearRect(0, 0, width, height);
    }

    function scheduleRender() {
      if (
        animFrameId === null &&
        !appState.launcherInTray
      ) {
        animFrameId = requestAnimationFrame(render);
      }
    }

    function render() {
      animFrameId = null;
      if (appState.launcherInTray) {
        context.clearRect(0, 0, width, height);
        return;
      }

      context.clearRect(0, 0, width, height);

      for (let i = 0; i < particles.length; i++) {
        const p = particles[i];
        p.x += p.vx;
        p.y += p.vy;
        p.alpha += p.dAlpha;

        if (p.alpha > 0.5) p.dAlpha = -Math.abs(p.dAlpha);
        if (p.alpha < 0.05) p.dAlpha = Math.abs(p.dAlpha);

        if (p.y < -10 || p.x < -10 || p.x > width + 10) {
          particles[i] = resetParticle({ y: height + 10 });
        }

        context.beginPath();
        context.arc(p.x, p.y, p.radius, 0, Math.PI * 2);
        context.fillStyle = `rgba(${p.r}, ${p.g}, ${p.b}, ${p.alpha})`;
        context.fill();
      }

      scheduleRender();
    }

    particleAnimationControl = {
      schedule: scheduleRender,
      stop: stopRender,
    };
    scheduleRender();

    return () => {
      particleAnimationControl = null;
      particles = [];
      particlesMounted = false;
      stopRender();
      window.removeEventListener('resize', handleResize);
    };
  });
</script>

<div class="bg-layer">
  <video
    bind:this={videoElement}
    id="bgVideo"
    muted
    loop
    playsinline
    preload="metadata"
    poster={backgroundPoster}
    onplaying={handleVideoPlaying}
    oncanplay={handleVideoPlaying}
    onerror={handleVideoError}
    class:visible={videoLoaded && isVideoAllowed}
  ></video>
  <div class="bg-vignette"></div>
  <div class="scanlines"></div>
  <div id="stageLights" class="stage-lights">
    <div class="stage-light" style="left: 20%;"></div>
    <div class="stage-light" style="left: 50%;"></div>
    <div class="stage-light" style="left: 80%;"></div>
  </div>
</div>

<canvas id="audioViz" class="audio-viz"></canvas>
<canvas id="waterFx" class="water-fx"></canvas>
<canvas bind:this={canvasElement} id="particleCanvas"></canvas>
