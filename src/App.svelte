<script lang="ts">
  import { onMount } from 'svelte';
  import { appState } from './lib/launcherState.svelte';
  import BackgroundFx from './components/BackgroundFx.svelte';
  import TopBar from './components/TopBar.svelte';
  import SidePanel from './components/SidePanel.svelte';
  import AudioPlayer from './components/AudioPlayer.svelte';
  import RightPanel from './components/RightPanel.svelte';
  import SettingsPanel from './components/SettingsPanel.svelte';
  import LogsPanel from './components/LogsPanel.svelte';
  import AboutPanel from './components/AboutPanel.svelte';
  import UpdateModal from './components/UpdateModal.svelte';

  onMount(async () => {
    await appState.init();
  });
</script>

<div class="app-root" class:game-running={appState.gameRunning} data-visual-mode={appState.config.launcherVisualMode || 'full'}>
  <BackgroundFx />
  <TopBar />
  {#if appState.page === 'home'}
    <SidePanel />
    <AudioPlayer />
    <RightPanel />
  {:else if appState.page === 'settings'}
    <SettingsPanel />
  {:else if appState.page === 'logs'}
    <LogsPanel />
  {:else if appState.page === 'about'}
    <AboutPanel />
  {/if}
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
