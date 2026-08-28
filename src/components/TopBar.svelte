<script lang="ts">
  import { appState } from '../lib/launcherState.svelte';
  import { bridge } from '../lib/bridge';
  import { isTauriRuntime } from '../lib/runtime';

  interface Props {
    onsettings?: () => void;
    settingsopen?: boolean;
  }

  let { onsettings, settingsopen = false }: Props = $props();

  async function handleMinimize() {
    if (!isTauriRuntime()) return;
    try { await bridge.minimizeWindow(); }
    catch (error) { appState.showToast(`Gagal meminimalkan launcher: ${error instanceof Error ? error.message : String(error)}`, 'err'); }
  }

  async function handleClose() {
    if (!isTauriRuntime()) return;
    const token = appState.beginOperation('close');
    if (!token) {
      appState.showToast(appState.getOperationBusyMessage('close'), 'info');
      return;
    }
    try {
      await bridge.closeWindow();
    } catch (error) {
      appState.showToast(`Gagal menutup launcher: ${error instanceof Error ? error.message : String(error)}`, 'err');
    } finally {
      appState.endOperation(token);
    }
  }

  let closeDisabled = $derived(appState.isOperationBlocked('close'));
</script>

<header class="top-bar" id="topBar" data-tauri-drag-region>
  <div class="top-bar__left" data-tauri-drag-region>
    <img src="/assets/logo.png" alt="Wuthering Waves" class="top-bar__logo" draggable="false" />
  </div>

  <div class="top-bar__right">
    <button
      class="top-bar__btn"
      id="settingsTrigger"
      title="Pengaturan"
      aria-label="Pengaturan"
      aria-haspopup="dialog"
      aria-expanded={settingsopen}
      onclick={() => onsettings?.()}
      type="button"
    >
      <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
        <path fill="currentColor" d="M19.43 12.98c.04-.32.07-.65-.07-.98s-.02-.66.07-.98l2.11-1.65a.5.5 0 0 0 .12-.64l-2-3.46a.5.5 0 0 0-.61-.22l-2.49 1a7.2 7.2 0 0 0-1.69-.98L14.5 2.42A.49.49 0 0 0 14 2h-4a.49.49 0 0 0-.49.42L9.13 5.07c-.61.25-1.17.58-1.69.98l-2.49-1a.5.5 0 0 0-.61.22l-2 3.46a.5.5 0 0 0 .12.64l2.11 1.65c-.04.32-.08.65-.08.98s.03.66.08.98l-2.11 1.65a.5.5 0 0 0-.12.64l2 3.46c.12.22.38.3.61.22l2.49-1c.52.4 1.08.73 1.69.98l.38 2.65c.04.24.24.42.49.42h4c.25 0 .46-.18.49-.42l.38-2.65c.61-.25 1.17-.58 1.69-.98l2.49 1c.23.08.49 0 .61-.22l2-3.46a.5.5 0 0 0-.12-.64l-2.11-1.65ZM12 15.5A3.5 3.5 0 1 1 12 8a3.5 3.5 0 0 1 0 7.5Z" />
      </svg>
    </button>

    <div class="top-bar__sep"></div>

    <button class="top-bar__btn" id="btnMinimize" title="Minimalkan" disabled={!isTauriRuntime()} onclick={handleMinimize} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M19 13H5v-2h14v2z" />
      </svg>
    </button>

    <button class="top-bar__btn top-bar__btn--close" id="btnClose" title="Tutup" disabled={closeDisabled || !isTauriRuntime()} onclick={handleClose} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
      </svg>
    </button>
  </div>
</header>
