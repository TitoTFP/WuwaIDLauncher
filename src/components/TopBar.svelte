<script lang="ts">
  import { appState } from '../lib/launcherState.svelte';
  import { bridge } from '../lib/bridge';
  import type { InstallMethod, PageId } from '../lib/types';

  let methodMenuOpen = $state(false);

  function setPage(page: PageId) {
    if (appState.gameRunning && page !== 'home') return;
    appState.page = page;
  }

  async function selectMethod(method: InstallMethod) {
    if (appState.config.installMethod === method) {
      methodMenuOpen = false;
      return;
    }
    appState.config.installMethod = method;
    methodMenuOpen = false;
    await appState.saveConfig();

    if (appState.gamePath) {
      await bridge.switchMethod(appState.gamePath, method);
      await bridge.checkPatchStatus(appState.gamePath, method);
    }
  }

  function toggleMethodMenu(e: MouseEvent) {
    e.stopPropagation();
    methodMenuOpen = !methodMenuOpen;
  }

  function handleMinimize() {
    bridge.minimizeWindow();
  }

  function handleClose() {
    bridge.closeWindow();
  }
</script>

<svelte:window onclick={() => (methodMenuOpen = false)} />

<header class="top-bar" id="topBar" data-tauri-drag-region>
  <div class="top-bar__left" data-tauri-drag-region>
    <img src="/assets/logo.png" alt="Wuthering Waves" class="top-bar__logo" draggable="false" />
  </div>

  <div class="top-bar__right">
    <nav class="top-nav" id="topNav">
      <button
        class="top-nav__item active"
        onclick={() => setPage('home')}
        type="button"
      >
        HOME
      </button>

      <button
        class="top-nav__item top-nav__item--menu"
        id="methodNavBtn"
        onclick={toggleMethodMenu}
        type="button"
      >
        METODE
      </button>

      <div class="top-nav__indicator" id="topNavIndicator"></div>
    </nav>

    {#if methodMenuOpen}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="method-menu" id="methodMenu" onclick={(e) => e.stopPropagation()}>
        <button
          class="method-menu__item"
          class:active={appState.config.installMethod === 'method3'}
          onclick={() => selectMethod('method3')}
          type="button"
        >
          <span class="method-menu__title">Metode 1</span>
          <span class="method-menu__desc">Resource Mount · tanpa signature bypass</span>
        </button>

        <button
          class="method-menu__item"
          class:active={appState.config.installMethod === 'method2'}
          onclick={() => selectMethod('method2')}
          type="button"
        >
          <span class="method-menu__title">Metode 2</span>
          <span class="method-menu__desc">winhttp.dll loader</span>
        </button>

        <button
          class="method-menu__item"
          class:active={appState.config.installMethod === 'method1'}
          onclick={() => selectMethod('method1')}
          type="button"
        >
          <span class="method-menu__title">Metode 3</span>
          <span class="method-menu__desc">Signature bypass</span>
        </button>
      </div>
    {/if}

    <div class="top-bar__sep"></div>

    <button class="top-bar__btn" id="btnMinimize" title="Minimalkan" onclick={handleMinimize} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M19 13H5v-2h14v2z" />
      </svg>
    </button>

    <button class="top-bar__btn top-bar__btn--close" id="btnClose" title="Tutup" onclick={handleClose} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
      </svg>
    </button>
  </div>
</header>
