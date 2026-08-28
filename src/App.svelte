<script lang="ts">
  import { onMount } from 'svelte';
  import { appState } from './lib/launcherState.svelte';
  import { isTauriRuntime } from './lib/runtime';
  import BackgroundFx from './components/BackgroundFx.svelte';
  import TopBar from './components/TopBar.svelte';
  import SettingsOverlay from './components/SettingsOverlay.svelte';
  import SidePanel from './components/SidePanel.svelte';
  import AudioPlayer from './components/AudioPlayer.svelte';
  import RightPanel from './components/RightPanel.svelte';
  import UpdateModal from './components/UpdateModal.svelte';
  import PatchNotesModal from './components/PatchNotesModal.svelte';
  import ToastHost from './components/ToastHost.svelte';
  import AdminModal from './components/AdminModal.svelte';

  const hasTauriRuntime = isTauriRuntime();

  let settingsOpen = $state(false);

  onMount(() => {
    // Keep the browser preview usable without invoking the Tauri bridge.
    // The packaged launcher always exposes __TAURI_INTERNALS__.
    if (!hasTauriRuntime) {
      appState.releaseNotesLoading = false;
      return () => appState.dispose();
    }

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
  <TopBar settingsopen={settingsOpen} onsettings={() => (settingsOpen = true)} />
  <SettingsOverlay open={settingsOpen} onclose={() => (settingsOpen = false)} />
  <SidePanel />
  <AudioPlayer />
  <RightPanel />
  <UpdateModal
    open={appState.launcherUpdateAvailable}
    version={appState.launcherUpdatePayload?.version ?? ''}
    currentVersion={appState.appVersion}
    releaseNotesBody={appState.launcherUpdatePayload?.body ?? ''}
    releaseNote={appState.launcherUpdatePayload}
    progress={appState.launcherUpdateProgress}
    status={appState.launcherUpdateStatus}
    error={appState.launcherUpdateError}
    restartCountdown={appState.launcherUpdateRestartCountdown}
    onclose={() => appState.dismissLauncherUpdate()}
  />
  <PatchNotesModal
    note={appState.launcherUpdateAvailable ? null : appState.firstLaunchLauncherReleaseNotes}
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
