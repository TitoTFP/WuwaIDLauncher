<script lang="ts">
  import { onMount } from 'svelte';
  import { appState } from '../lib/launcherState.svelte';

  let volumePercent = $state(35);
  let isMuted = $state(false);
  let isPlaying = $state(false);
  let userWantsPlayback = $state(true);
  let wasPlayingBeforeGame = $state(false);
  let audioElement: HTMLAudioElement | null = $state(null);

  // Sync state with loaded configuration
  $effect(() => {
    if (appState.config.bgmVolume !== undefined) {
      volumePercent = Math.max(0, Math.min(100, Math.round(appState.config.bgmVolume * 100)));
    }
    if (appState.config.bgmEnabled !== undefined) {
      userWantsPlayback = appState.config.bgmEnabled;
    }
  });

  // Handle media source changes and initial playback
  $effect(() => {
    if (!audioElement) return;

    if (appState.bgmUrl && audioElement.src !== appState.bgmUrl) {
      audioElement.src = appState.bgmUrl;
      audioElement.volume = isMuted ? 0 : volumePercent / 100;
      audioElement.muted = isMuted;

      if (userWantsPlayback && !appState.gameRunning && appState.config.bgmEnabled !== false) {
        void audioElement.play().then(() => {
          isPlaying = true;
        }).catch(() => {
          isPlaying = false;
        });
      }
    }
  });

  // Handle game running suspension and resumption
  $effect(() => {
    if (!audioElement) return;

    if (appState.gameRunning) {
      if (isPlaying) {
        wasPlayingBeforeGame = true;
        audioElement.pause();
        isPlaying = false;
      }
    } else if (wasPlayingBeforeGame && userWantsPlayback && appState.config.bgmEnabled !== false) {
      wasPlayingBeforeGame = false;
      void audioElement.play().then(() => {
        isPlaying = true;
      }).catch(() => {});
    }
  });

  // Browser Autoplay Policy: attempt play on user's first document interaction
  onMount(() => {
    const handleFirstInteraction = () => {
      if (audioElement && userWantsPlayback && !isPlaying && !appState.gameRunning && appState.bgmUrl) {
        void audioElement.play().then(() => {
          isPlaying = true;
        }).catch(() => {});
      }
    };

    window.addEventListener('click', handleFirstInteraction, { once: true });
    return () => {
      window.removeEventListener('click', handleFirstInteraction);
    };
  });

  function togglePlay() {
    if (!audioElement) return;

    if (isPlaying) {
      audioElement.pause();
      isPlaying = false;
      userWantsPlayback = false;
      appState.config.bgmEnabled = false;
      void appState.saveConfig();
    } else {
      userWantsPlayback = true;
      appState.config.bgmEnabled = true;
      void appState.saveConfig();

      if (audioElement.src) {
        void audioElement.play().then(() => {
          isPlaying = true;
        }).catch(() => {
          isPlaying = false;
        });
      }
    }
  }

  function toggleMute() {
    if (!audioElement) return;
    isMuted = !isMuted;
    audioElement.muted = isMuted;
    if (!isMuted && volumePercent === 0) {
      volumePercent = 35;
      audioElement.volume = 0.35;
      appState.config.bgmVolume = 0.35;
      void appState.saveConfig();
    }
  }

  function handleVolumeChange(e: Event) {
    const val = parseInt((e.target as HTMLInputElement).value, 10);
    volumePercent = val;
    if (audioElement) {
      if (val > 0 && isMuted) {
        isMuted = false;
        audioElement.muted = false;
      }
      audioElement.volume = isMuted ? 0 : val / 100;
    }
    appState.config.bgmVolume = val / 100;
    void appState.saveConfig();
  }
</script>

<div class="audio-player" id="audioPlayer">
  <audio
    bind:this={audioElement}
    id="bgMusic"
    loop
    preload="auto"
    onplay={() => (isPlaying = true)}
    onpause={() => (isPlaying = false)}
    onerror={() => (isPlaying = false)}
  ></audio>

  <button
    class="ap-btn ap-btn--play"
    id="apPlay"
    onclick={togglePlay}
    title={isPlaying ? 'Jeda Musik' : 'Putar Musik'}
    type="button"
  >
    {#if isPlaying}
      <svg id="apIconPause" viewBox="0 0 24 24" width="18" height="18">
        <path fill="currentColor" d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
      </svg>
    {:else}
      <svg id="apIconPlay" viewBox="0 0 24 24" width="18" height="18">
        <path fill="currentColor" d="M8 5v14l11-7z" />
      </svg>
    {/if}
  </button>

  <div class="ap-vol">
    <button
      class="ap-btn ap-btn--sm"
      id="apVolBtn"
      onclick={toggleMute}
      title={isMuted ? 'Aktifkan Suara' : 'Bisukan'}
      type="button"
    >
      {#if isMuted || volumePercent === 0}
        <svg id="apVolIconMuted" viewBox="0 0 24 24" width="13" height="13">
          <path
            fill="currentColor"
            d="M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z"
          />
        </svg>
      {:else if volumePercent < 50}
        <svg id="apVolIconLo" viewBox="0 0 24 24" width="13" height="13">
          <path
            fill="currentColor"
            d="M18.5 12c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM5 9v6h4l5 5V4L9 9H5z"
          />
        </svg>
      {:else}
        <svg id="apVolIconHi" viewBox="0 0 24 24" width="13" height="13">
          <path
            fill="currentColor"
            d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"
          />
        </svg>
      {/if}
    </button>

    <div class="ap-vol__track">
      <input
        type="range"
        class="ap-vol__slider"
        id="apVolSlider"
        min="0"
        max="100"
        value={isMuted ? 0 : volumePercent}
        oninput={handleVolumeChange}
        step="1"
      />
      <div
        class="ap-vol__fill"
        id="apVolFill"
        style="width: {isMuted ? 0 : volumePercent}%"
      ></div>
    </div>

    <span class="ap-vol__label" id="apVolLabel">{isMuted ? '0' : volumePercent}</span>
  </div>
</div>
