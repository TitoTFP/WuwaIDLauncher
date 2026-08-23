<script lang="ts">
  import { onMount } from 'svelte';
  import { appState } from './lib/launcherState.svelte';
  import BackgroundFx from './components/BackgroundFx.svelte';
  import TopBar from './components/TopBar.svelte';
  import SidePanel from './components/SidePanel.svelte';
  import AudioPlayer from './components/AudioPlayer.svelte';
  import RightPanel from './components/RightPanel.svelte';
  import SettingsPanel from './components/SettingsPanel.svelte';
  import AboutPanel from './components/AboutPanel.svelte';
  import UpdateModal from './components/UpdateModal.svelte';
  import PatchNotesModal from './components/PatchNotesModal.svelte';
  import ToastHost from './components/ToastHost.svelte';
  import AdminModal from './components/AdminModal.svelte';

  onMount(() => {
    let mounted = true;
    void appState.init().catch((error) => {
      if (mounted) {
        appState.setStatus('Launcher tidak dapat diinisialisasi.', String(error));
      }
    });
    return () => {
      mounted = false;
      appState.dispose();
    };
  });

  $effect(() => {
    if (typeof document === 'undefined') return;
    document.body.classList.toggle(
      'game-runtime-readonly',
      appState.launcherInTray ||
        appState.installing ||
        (appState.launching && !appState.gameRunning),
    );
    return () => document.body.classList.remove('game-runtime-readonly');
  });

</script>

<div
  class="app-root"
  class:game-running={appState.gameRunning && appState.launcherInTray}
  class:runtime-paused={appState.launcherInTray}
>
  <BackgroundFx />
  <TopBar />
  {#if appState.page === 'home'}
    <SidePanel />
    <AudioPlayer />
    <RightPanel />
  {:else if appState.page === 'settings'}
    <SettingsPanel />
  {:else if appState.page === 'about'}
    <AboutPanel />
  {/if}
  <UpdateModal
    open={appState.launcherUpdateAvailable}
    version={appState.launcherUpdatePayload?.version ?? ''}
    progress={appState.launcherUpdateProgress}
    status={appState.launcherUpdateStatus}
    error={appState.launcherUpdateError}
    restartCountdown={appState.launcherUpdateRestartCountdown}
    onclose={() => appState.dismissLauncherUpdate()}
  />
  <PatchNotesModal
    note={appState.firstLaunchLauncherReleaseNotes}
    onclose={() => appState.dismissFirstLaunchLauncherReleaseNotes()}
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
