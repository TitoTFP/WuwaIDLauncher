<script lang="ts">
  import { onMount } from 'svelte';
  import { appState } from '../lib/launcherState.svelte';

  let videoElement: HTMLVideoElement | null = $state(null);
  let canvasElement: HTMLCanvasElement | null = $state(null);
  let videoLoaded = $state(false);

  let isVideoAllowed = $derived(!appState.gameRunning);

  $effect(() => {
    if (!videoElement) return;

    if (appState.videoUrl && isVideoAllowed) {
      if (videoElement.src !== appState.videoUrl) {
        videoElement.src = appState.videoUrl;
        videoElement.load();
      }
      void videoElement.play().catch(() => {});
    } else {
      videoElement.pause();
    }
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

    let width = (canvas.width = window.innerWidth);
    let height = (canvas.height = window.innerHeight);

    const handleResize = () => {
      width = canvas.width = window.innerWidth;
      height = canvas.height = window.innerHeight;
    };
    window.addEventListener('resize', handleResize);

    interface Particle {
      x: number;
      y: number;
      radius: number;
      vx: number;
      vy: number;
      alpha: number;
      dAlpha: number;
      r: number;
      g: number;
      b: number;
    }

    const particleCount = 24;
    const particles: Particle[] = [];

    function resetParticle(p: Partial<Particle> = {}): Particle {
      const isGold = Math.random() > 0.4;
      return {
        x: p.x ?? Math.random() * width,
        y: p.y ?? Math.random() * height,
        radius: Math.random() * 1.8 + 0.5,
        vx: (Math.random() - 0.5) * 0.25,
        vy: -Math.random() * 0.35 - 0.08,
        alpha: Math.random() * 0.35 + 0.1,
        dAlpha: (Math.random() > 0.5 ? 1 : -1) * (Math.random() * 0.004 + 0.001),
        r: isGold ? 212 : 80,
        g: isGold ? 176 : 195,
        b: isGold ? 108 : 220,
      };
    }

    for (let i = 0; i < particleCount; i++) {
      particles.push(resetParticle());
    }

    let animFrameId: number;

    function render() {
      if (!ctx) return;
      if (appState.gameRunning) {
        ctx.clearRect(0, 0, width, height);
        animFrameId = requestAnimationFrame(render);
        return;
      }

      ctx.clearRect(0, 0, width, height);

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

        ctx.beginPath();
        ctx.arc(p.x, p.y, p.radius, 0, Math.PI * 2);
        ctx.fillStyle = `rgba(${p.r}, ${p.g}, ${p.b}, ${p.alpha})`;
        ctx.fill();
      }

      animFrameId = requestAnimationFrame(render);
    }

    animFrameId = requestAnimationFrame(render);

    return () => {
      window.removeEventListener('resize', handleResize);
      cancelAnimationFrame(animFrameId);
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
    preload="auto"
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
