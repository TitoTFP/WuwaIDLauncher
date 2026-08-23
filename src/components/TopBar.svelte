<script lang="ts">
  import { appState } from '../lib/launcherState.svelte';
  import { bridge } from '../lib/bridge';
  import { INSTALL_METHOD_OPTIONS } from '../lib/types';
  import type { InstallMethod } from '../lib/types';

  let methodMenuOpen = $state(false);

  async function selectMethod(method: InstallMethod) {
    if (appState.isOperationBlocked('method-switch')) {
      appState.showToast(appState.getOperationBusyMessage('method-switch'), 'info');
      return;
    }
    if (appState.gameRunning || appState.installing || appState.launching) return;
    if (appState.config.installMethod === method) {
      methodMenuOpen = false;
      return;
    }
    methodMenuOpen = false;
    try {
      await appState.switchInstallMethod(method);
      appState.clearStatus();
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      appState.showToast(`Gagal mengganti metode instalasi.\n${detail}`, 'err');
    }
  }

  function toggleMethodMenu(e: MouseEvent) {
    e.stopPropagation();
    methodMenuOpen = !methodMenuOpen;
  }

  async function handleMinimize() {
    try { await bridge.minimizeWindow(); }
    catch (error) { appState.showToast(`Gagal meminimalkan launcher: ${error instanceof Error ? error.message : String(error)}`, 'err'); }
  }

  async function handleClose() {
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
  let methodDisabled = $derived(
    appState.gameRunning ||
      appState.installing ||
      appState.launching ||
      appState.isOperationBlocked('method-switch'),
  );
</script>

<svelte:window onclick={() => (methodMenuOpen = false)} />

<header class="top-bar" id="topBar" data-tauri-drag-region>
  <div class="top-bar__left" data-tauri-drag-region>
    <img src="/assets/logo.png" alt="Wuthering Waves" class="top-bar__logo" draggable="false" />
  </div>

  <div class="top-bar__right">
    <nav class="top-nav" id="topNav">
      <button
        class="top-nav__item"
        class:active={appState.page === 'home'}
        data-page="home"
        type="button"
        aria-current={appState.page === 'home' ? 'page' : undefined}
        onclick={() => (appState.page = 'home')}
      >
        HOME
      </button>

      <button
        class="top-nav__item top-nav__item--menu"
        class:open={methodMenuOpen}
        id="methodNavBtn"
        disabled={methodDisabled}
        aria-expanded={methodMenuOpen}
        onclick={toggleMethodMenu}
        type="button"
      >
        METODE
      </button>

      <button
        class="top-nav__item"
        class:active={appState.page === 'settings'}
        data-page="settings"
        type="button"
        aria-current={appState.page === 'settings' ? 'page' : undefined}
        onclick={() => { methodMenuOpen = false; appState.page = 'settings'; }}
      >
        PENGATURAN
      </button>

      <button
        class="top-nav__item"
        class:active={appState.page === 'about'}
        data-page="about"
        type="button"
        aria-current={appState.page === 'about' ? 'page' : undefined}
        onclick={() => { methodMenuOpen = false; appState.page = 'about'; }}
      >
        TENTANG
      </button>

      <div class="top-nav__indicator" id="topNavIndicator"></div>
    </nav>

    {#if methodMenuOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="method-menu open" id="methodMenu" onclick={(e) => e.stopPropagation()}>
        {#each INSTALL_METHOD_OPTIONS as option}
          <button
            class="method-menu__item"
            class:active={appState.config.installMethod === option.value}
            onclick={() => selectMethod(option.value)}
            type="button"
          >
            <span class="method-menu__title">{option.title}</span>
            <span class="method-menu__desc">{option.description}</span>
          </button>
        {/each}
      </div>
    {/if}

    <div class="top-bar__sep"></div>

    <button class="top-bar__btn" id="btnMinimize" title="Minimalkan" onclick={handleMinimize} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M19 13H5v-2h14v2z" />
      </svg>
    </button>

    <button class="top-bar__btn top-bar__btn--close" id="btnClose" title="Tutup" disabled={closeDisabled} onclick={handleClose} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
      </svg>
    </button>
  </div>
</header>
