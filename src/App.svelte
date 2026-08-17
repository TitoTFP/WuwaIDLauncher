<script lang="ts">
  import { onMount } from 'svelte';
  import { appState } from './lib/launcherState.svelte';
  import BackgroundFx from './components/BackgroundFx.svelte';
  import TopBar from './components/TopBar.svelte';
  import SidePanel from './components/SidePanel.svelte';
  import AudioPlayer from './components/AudioPlayer.svelte';
  import RightPanel from './components/RightPanel.svelte';
  import UpdateModal from './components/UpdateModal.svelte';

  let updateModalOpen = $state(false);
  let updateVersion = $state('');
  let updateZipUrl = $state('');

  onMount(async () => {
    await appState.init();
  });
</script>

<div class="app-root" class:game-running={appState.gameRunning} data-visual-mode={appState.config.launcherVisualMode || 'full'}>
  <BackgroundFx />
  <TopBar />
  <SidePanel />
  <AudioPlayer />
  <RightPanel />
  <UpdateModal
    open={updateModalOpen}
    version={updateVersion}
    zipUrl={updateZipUrl}
    onclose={() => (updateModalOpen = false)}
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
