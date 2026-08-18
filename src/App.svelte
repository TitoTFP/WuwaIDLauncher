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
</script>

<div class="app-root" class:game-running={appState.gameRunning}>
  <BackgroundFx />
  <TopBar />
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
</style>
