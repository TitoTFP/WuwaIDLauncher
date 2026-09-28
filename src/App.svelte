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

  import { themeRuntime } from './lib/themeRuntime.svelte';

  type VitePreviewManifest = {
    assets?: Array<{ name: string }>;
    theme?: {
      active?: boolean;
      id: string;
      name: string;
      tokens: Record<string, string>;
      background?: { name: string };
    };
  };

  function localPreviewAssetUrl(...segments: string[]): string {
    const safeSegment = /^[a-z0-9][a-z0-9._-]*$/i;
    if (segments.some((segment) => !safeSegment.test(segment))) {
      return '';
    }
    return `/${segments.join('/')}`;
  }

  async function applyViteThemePreview(
    isCurrent: () => boolean,
  ): Promise<boolean> {
    const response = await fetch('/Web/assets.json');
    if (!response.ok) {
      throw new Error(`Manifest request failed with HTTP ${response.status}.`);
    }

    const manifest = (await response.json()) as VitePreviewManifest;
    const theme = manifest.theme;
    if (!theme?.active || !theme.background || !isCurrent()) {
      return false;
    }

    const backgroundUrl = localPreviewAssetUrl(
      'Web',
      'Theme',
      theme.id,
      theme.background.name,
    );
    if (!backgroundUrl) return false;

    const videoAsset = manifest.assets?.find(
      ({ name }) => name === 'bg-video.mp4',
    );
    const videoUrl = videoAsset
      ? localPreviewAssetUrl('Web', 'Video', videoAsset.name)
      : '';

    themeRuntime.apply({
      id: theme.id,
      name: theme.name,
      tokens: theme.tokens,
      css: '',
      backgroundUrl,
    });
    if (videoUrl) appState.videoUrl = videoUrl;
    return true;
  }

  const hasTauriRuntime = isTauriRuntime();

  let settingsOpen = $state(false);

  onMount(() => {
    // Keep the browser preview usable without invoking the Tauri bridge.
    // The packaged launcher always exposes __TAURI_INTERNALS__.
    if (!hasTauriRuntime) {
      appState.releaseNotesLoading = false;
      let previewMounted = true;
      if (import.meta.env.DEV) {
        void applyViteThemePreview(() => previewMounted).catch((error) => {
          if (previewMounted) {
            console.warn('Vite theme preview failed to load.', error);
          }
        });
      }
      return () => {
        previewMounted = false;
        if (import.meta.env.DEV) {
          appState.videoUrl = '';
          themeRuntime.apply(null);
        }
        appState.dispose();
      };
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
