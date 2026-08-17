<script lang="ts">
  import { appState } from '../lib/launcherState.svelte';
  import { bridge } from '../lib/bridge';
  import { INSTALL_METHOD_OPTIONS } from '../lib/types';
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
    methodMenuOpen = false;
    const previous = appState.config.installMethod;
    try {
      if (appState.gamePath) {
        const report = await bridge.switchMethod(appState.gamePath, method);
        if (report.failures.length || report.preserved.length) {
          throw new Error([...report.failures, ...report.preserved].join('; '));
        }
      }
      appState.config.installMethod = method;
      await appState.saveConfig();
      if (appState.gamePath) {
        await bridge.checkPatchStatus(appState.gamePath, method);
      }
      appState.clearStatus();
    } catch (error) {
      appState.config.installMethod = previous;
      const detail = error instanceof Error ? error.message : String(error);
      appState.setStatus('Gagal mengganti metode instalasi.', detail);
    }
  }

  function toggleMethodMenu(e: MouseEvent) {
    e.stopPropagation();
    methodMenuOpen = !methodMenuOpen;
  }

  async function handleMinimize() {
    try { await bridge.minimizeWindow(); }
    catch (error) { appState.setStatus('Gagal meminimalkan launcher.', error instanceof Error ? error.message : String(error)); }
  }

  async function handleClose() {
    try { await bridge.closeWindow(); }
    catch (error) { appState.setStatus('Gagal menutup launcher.', error instanceof Error ? error.message : String(error)); }
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
        class="top-nav__item"
        class:active={appState.page === 'home'}
        onclick={() => setPage('home')}
        type="button"
      >
        HOME
      </button>

      {#each [{ id: 'settings', label: 'SETTINGS' }, { id: 'logs', label: 'LOGS' }, { id: 'about', label: 'ABOUT' }] as item}
        <button
          class="top-nav__item"
          class:active={appState.page === item.id}
          disabled={appState.gameRunning}
          onclick={() => setPage(item.id as PageId)}
          type="button"
        >{item.label}</button>
      {/each}

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

    <button class="top-bar__btn top-bar__btn--close" id="btnClose" title="Tutup" onclick={handleClose} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
      </svg>
    </button>
  </div>
</header>
