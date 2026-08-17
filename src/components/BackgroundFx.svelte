<script lang="ts">
  import { appState } from '../lib/launcherState.svelte';

  let videoElement: HTMLVideoElement | null = $state(null);

  $effect(() => {
    if (appState.videoUrl && videoElement) {
      videoElement.src = appState.videoUrl;
      void videoElement.play().catch(() => {});
    }
  });
</script>

<div class="bg-layer" data-visual-mode="full">
  <video
    bind:this={videoElement}
    id="bgVideo"
    muted
    loop
    playsinline
    preload="none"
    class:visible={!!appState.videoUrl}
  ></video>
  <div class="bg-vignette"></div>
  <div class="scanlines"></div>
  <div id="stageLights" class="stage-lights"></div>
</div>

<canvas id="particleCanvas"></canvas>
