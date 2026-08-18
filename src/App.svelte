<script lang="ts">
  import { onMount } from 'svelte';
  import { appState } from './lib/launcherState.svelte';
  import BackgroundFx from './components/BackgroundFx.svelte';
  import TopBar from './components/TopBar.svelte';
  import SidePanel from './components/SidePanel.svelte';
  import AudioPlayer from './components/AudioPlayer.svelte';
  import RightPanel from './components/RightPanel.svelte';
  import UpdateModal from './components/UpdateModal.svelte';
  import ToastHost from './components/ToastHost.svelte';
  import AdminModal from './components/AdminModal.svelte';

  onMount(async () => {
    await appState.init();
  });

  $effect(() => {
    if (typeof document === 'undefined') return;
    document.body.classList.toggle('game-runtime-readonly', appState.gameRunning);
  });

  function formatCountdown(totalSeconds: number): string {
    const minutes = Math.floor(totalSeconds / 60).toString().padStart(2, '0');
    const seconds = (totalSeconds % 60).toString().padStart(2, '0');
    return `${minutes}:${seconds}`;
  }
</script>

<div class="app-root" class:game-running={appState.gameRunning}>
  <BackgroundFx />
  <TopBar />
  {#if appState.signatureRestoreCountdown !== null}
    <div class="signature-restore-notice" role="status" aria-live="polite">
      <span>Metode 3 aktif — launcher belum dapat ditutup</span>
      <strong>{formatCountdown(appState.signatureRestoreCountdown)}</strong>
      <small>Menunggu pemulihan signature game.</small>
    </div>
  {/if}
  <SidePanel />
  <AudioPlayer />
  <RightPanel />
  <UpdateModal
    open={appState.launcherUpdateAvailable}
    version={appState.launcherUpdatePayload?.version ?? ''}
    zipUrl={appState.launcherUpdatePayload?.zipUrl ?? ''}
    checksumsUrl={appState.launcherUpdatePayload?.checksumsUrl ?? ''}
    progress={appState.launcherUpdateProgress}
    status={appState.launcherUpdateStatus}
    error={appState.launcherUpdateError}
    onclose={() => appState.dismissLauncherUpdate()}
  />
  <ToastHost />
  <AdminModal />
</div>

<style>
  .app-root {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    position: relative;
    user-select: none;
  }

  .game-running {
    filter: brightness(0.85);
  }

  .signature-restore-notice {
    position: fixed;
    top: 56px;
    left: 50%;
    z-index: 700;
    display: grid;
    grid-template-columns: auto auto;
    align-items: center;
    gap: 0 14px;
    padding: 9px 14px;
    transform: translateX(-50%);
    background: rgba(14, 18, 52, 0.94);
    border: 1px solid rgba(212, 176, 108, 0.55);
    border-left: 3px solid var(--gold);
    box-shadow: 0 5px 22px rgba(0, 0, 0, 0.48);
    color: var(--text-1);
    font-size: 12px;
    line-height: 1.35;
    pointer-events: none;
  }

  .signature-restore-notice strong {
    color: var(--gold);
    font-size: 18px;
    letter-spacing: 0.08em;
  }

  .signature-restore-notice small {
    grid-column: 1 / -1;
    color: var(--text-2);
  }
</style>
