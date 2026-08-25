<script lang="ts">
  import { appState } from '../lib/launcherState.svelte.ts';
  import { INSTALL_METHOD_OPTIONS } from '../lib/types';
  import type { InstallMethod } from '../lib/types';

  interface Props {
    open?: boolean;
    onclose?: () => void;
  }

  let { open = false, onclose }: Props = $props();

  let methodDisabled = $derived(
    appState.gameRunning ||
      appState.installing ||
      appState.launching ||
      appState.isOperationBlocked('method-switch'),
  );

  let dx11Disabled = $derived(
    appState.isOperationBlocked('folder') ||
      appState.isOperationBlocked('method-switch') ||
      appState.isOperationBlocked('install'),
  );

  let hideUidDisabled = $derived(
    appState.isOperationBlocked('folder') ||
      appState.isOperationBlocked('method-switch') ||
      appState.isOperationBlocked('install'),
  );

  let csharpEnvironmentDisabled = $derived(appState.launching || appState.gameRunning);

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  async function selectMethod(method: InstallMethod) {
    if (appState.isOperationBlocked('method-switch')) {
      appState.showToast(appState.getOperationBusyMessage('method-switch'), 'info');
      return;
    }
    if (methodDisabled) return;
    if (appState.config.installMethod === method) return;

    try {
      await appState.switchInstallMethod(method);
      appState.clearStatus();
    } catch (error) {
      appState.showToast(`Gagal mengganti metode instalasi.\n${errorMessage(error)}`, 'err');
    }
  }

  async function handleDx11Change(event: Event) {
    appState.config.dx11 = (event.currentTarget as HTMLInputElement).checked;
    try {
      await appState.saveConfig();
      appState.setStatus('Mode DX11 diperbarui.');
    } catch (error) {
      appState.setStatus('Mode DX11 tidak dapat disimpan.', errorMessage(error));
    }
  }

  async function handleCSharpEnvironmentChange(event: Event) {
    appState.config.csharpEnvironment = (event.currentTarget as HTMLInputElement).checked;
    try {
      await appState.saveConfig();
      appState.setStatus('Optimisasi C# diperbarui.');
    } catch (error) {
      appState.setStatus('Optimisasi C# tidak dapat disimpan.', errorMessage(error));
    }
  }

  async function handleHideUidChange(event: Event) {
    appState.config.hideUid = (event.currentTarget as HTMLInputElement).checked;
    try {
      await appState.saveConfig();
      if (appState.gamePath) {
        await appState.requestPatchStatus(appState.gamePath, appState.config.installMethod);
      }
      appState.setStatus('Pilihan sembunyikan UID diperbarui.');
    } catch (error) {
      appState.setStatus('Pilihan sembunyikan UID tidak dapat diterapkan.', errorMessage(error));
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (open && event.key === 'Escape') onclose?.();
  }

  function handleBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) onclose?.();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="settings-overlay"
    role="dialog"
    tabindex="-1"
    aria-modal="true"
    aria-labelledby="settingsTitle"
    onclick={handleBackdropClick}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="settings-modal" onclick={(event) => event.stopPropagation()}>
      <header class="settings-header">
        <div class="settings-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" width="22" height="22">
            <path fill="currentColor" d="M19.43 12.98c.04-.32.07-.65-.07-.98s-.02-.66.07-.98l2.11-1.65a.5.5 0 0 0 .12-.64l-2-3.46a.5.5 0 0 0-.61-.22l-2.49 1a7.2 7.2 0 0 0-1.69-.98L14.5 2.42A.49.49 0 0 0 14 2h-4a.49.49 0 0 0-.49.42L9.13 5.07c-.61.25-1.17.58-1.69.98l-2.49-1a.5.5 0 0 0-.61.22l-2 3.46a.5.5 0 0 0 .12.64l2.11 1.65c-.04.32-.08.65-.08.98s.03.66.08.98l-2.11 1.65a.5.5 0 0 0-.12.64l2 3.46c.12.22.38.3.61.22l2.49-1c.52.4 1.08.73 1.69.98l.38 2.65c.04.24.24.42.49.42h4c.25 0 .46-.18.49-.42l.38-2.65c.61-.25 1.17-.58 1.69-.98l2.49 1c.23.08.49 0 .61-.22l2-3.46a.5.5 0 0 0-.12-.64l-2.11-1.65ZM12 15.5A3.5 3.5 0 1 1 12 8a3.5 3.5 0 0 1 0 7.5Z" />
          </svg>
        </div>
        <h1 class="settings-title" id="settingsTitle">SETTINGS</h1>
        <button class="settings-close" id="settingsClose" title="Tutup" aria-label="Tutup pengaturan" onclick={() => onclose?.()} type="button">
          <svg viewBox="0 0 24 24" width="17" height="17">
            <path fill="currentColor" d="m18.3 5.7-1.4-1.4L12 9.2 7.1 4.3 5.7 5.7l4.9 4.9-4.9 4.9 1.4 1.4-4.9-4.9 4.9-4.9Z" />
          </svg>
        </button>
      </header>

      <section class="settings-section" aria-labelledby="methodHeading">
        <h2 class="section-title" id="methodHeading">METODE INSTALASI</h2>
        <div class="method-grid">
          {#each INSTALL_METHOD_OPTIONS as option}
            <button
              class="method-card"
              class:active={appState.config.installMethod === option.value}
              aria-pressed={appState.config.installMethod === option.value}
              disabled={methodDisabled}
              onclick={() => selectMethod(option.value)}
              type="button"
            >
              <span class="method-card__top">
                <span class="method-card__title">{option.title}</span>
                <span class="method-card__check" aria-hidden="true">✓</span>
              </span>
              <span class="method-card__desc">{option.description}</span>
            </button>
          {/each}
        </div>
      </section>

      <section class="settings-section" aria-labelledby="privacyHeading">
        <h2 class="section-title" id="privacyHeading">PRIVASI</h2>
        <div class="option-list">
          <label class="option-row" for="settingsHideUid">
            <span class="option-name">Sembunyikan UID</span>
            <span class="settings-switch">
              <input
                id="settingsHideUid"
                type="checkbox"
                checked={!!appState.config.hideUid}
                disabled={hideUidDisabled}
                onchange={handleHideUidChange}
              />
              <span class="switch-track" aria-hidden="true"></span>
            </span>
          </label>
        </div>
      </section>

      <section class="settings-section" aria-labelledby="optimizationHeading">
        <h2 class="section-title" id="optimizationHeading">OPTIMASI</h2>
        <div class="option-list">
          <label class="option-row" for="settingsCSharp">
            <span class="option-name">Optimasi C#</span>
            <span class="settings-switch">
              <input
                id="settingsCSharp"
                type="checkbox"
                checked={!!appState.config.csharpEnvironment}
                disabled={csharpEnvironmentDisabled}
                onchange={handleCSharpEnvironmentChange}
              />
              <span class="switch-track" aria-hidden="true"></span>
            </span>
          </label>
          <label class="option-row" for="settingsDx11">
            <span class="option-name">DirectX 11</span>
            <span class="settings-switch">
              <input
                id="settingsDx11"
                type="checkbox"
                checked={!!appState.config.dx11}
                disabled={dx11Disabled}
                onchange={handleDx11Change}
              />
              <span class="switch-track" aria-hidden="true"></span>
            </span>
          </label>
        </div>
      </section>

      <footer class="settings-footer">
        <span class="save-note"><span class="save-dot" aria-hidden="true"></span>Perubahan tersimpan otomatis</span>
        <button class="done-button" onclick={() => onclose?.()} type="button">SELESAI</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .settings-overlay {
    position: fixed;
    inset: 0;
    z-index: 450;
    display: grid;
    place-items: center;
    padding: calc(var(--top-h) + 12px) 24px 24px;
    background: rgba(5, 7, 28, 0.7);
    backdrop-filter: blur(5px);
    -webkit-backdrop-filter: blur(5px);
    -webkit-app-region: no-drag;
    animation: settings-fade-in 180ms ease both;
  }

  .settings-modal {
    width: min(620px, calc(100vw - 48px));
    max-height: calc(100vh - var(--top-h) - 36px);
    overflow: auto;
    padding: 24px 28px 20px;
    background: rgba(14, 18, 52, 0.98);
    border: 1px solid rgba(244, 212, 138, 0.58);
    clip-path: polygon(0 0, calc(100% - 18px) 0, 100% 18px, 100% calc(100% - 18px), calc(100% - 18px) 100%, 0 100%);
    box-shadow: 0 24px 70px rgba(0, 0, 0, 0.72), 0 0 34px rgba(244, 212, 138, 0.12), inset 0 1px rgba(255, 255, 255, 0.06);
    animation: settings-modal-in 300ms var(--ease) both;
  }

  .settings-modal::-webkit-scrollbar {
    width: 4px;
  }

  .settings-modal::-webkit-scrollbar-thumb {
    background: rgba(212, 176, 108, 0.45);
  }

  .settings-header {
    display: flex;
    align-items: center;
    gap: 13px;
    padding-bottom: 18px;
    border-bottom: 1px solid rgba(212, 176, 108, 0.2);
  }

  .settings-icon {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    flex: 0 0 42px;
    color: var(--accent-gold);
    background: rgba(212, 176, 108, 0.1);
    border: 1px solid rgba(212, 176, 108, 0.45);
    clip-path: polygon(7px 0, 100% 0, 100% calc(100% - 7px), calc(100% - 7px) 100%, 0 100%, 0 7px);
  }

  .settings-title {
    flex: 1;
    margin: 0;
    color: var(--accent-gold);
    font-size: 20px;
    letter-spacing: 0.1em;
  }

  .settings-close {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    color: var(--text-2);
    background: transparent;
    clip-path: polygon(7px 0, 100% 0, 100% calc(100% - 7px), calc(100% - 7px) 100%, 0 100%, 0 7px);
    transition: color var(--dur), background var(--dur);
  }

  .settings-close:hover {
    color: #fff;
    background: var(--red);
  }

  .settings-section {
    padding-top: 20px;
  }

  .section-title {
    display: block;
    margin: 0 0 10px;
    color: var(--accent-gold);
    font-size: 10px;
    letter-spacing: 0.17em;
    font-weight: 900;
  }

  .method-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .method-card {
    position: relative;
    min-height: 98px;
    padding: 13px 15px;
    color: var(--text-2);
    text-align: left;
    background: rgba(255, 255, 255, 0.025);
    border: 1px solid rgba(212, 176, 108, 0.24);
    clip-path: polygon(8px 0, 100% 0, 100% calc(100% - 8px), calc(100% - 8px) 100%, 0 100%, 0 8px);
    transition: background var(--dur), border-color var(--dur), color var(--dur), transform var(--dur);
  }

  .method-card:hover:not(:disabled) {
    color: var(--text-1);
    background: rgba(212, 176, 108, 0.1);
    border-color: rgba(212, 176, 108, 0.65);
    transform: translateY(-1px);
  }

  .method-card.active {
    color: #111;
    background: var(--accent-gold);
    border-color: var(--accent-gold);
    box-shadow: 0 8px 22px rgba(244, 212, 138, 0.18);
  }

  .method-card:disabled {
    cursor: default;
    opacity: 0.58;
  }

  .method-card__top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .method-card__title {
    font-size: 13px;
    font-weight: 900;
  }

  .method-card__check {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    color: transparent;
    border: 1px solid currentColor;
  }

  .method-card.active .method-card__check {
    color: #111;
    background: rgba(0, 0, 0, 0.08);
  }

  .method-card__desc {
    display: block;
    margin-top: 8px;
    color: inherit;
    opacity: 0.72;
    font-size: 10px;
    line-height: 1.5;
  }

  .option-list {
    display: grid;
    gap: 7px;
  }

  .option-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 13px;
    min-height: 50px;
    padding: 9px 14px;
    color: var(--text-1);
    background: rgba(255, 255, 255, 0.025);
    border: 1px solid rgba(212, 176, 108, 0.18);
    clip-path: polygon(7px 0, 100% 0, 100% calc(100% - 7px), calc(100% - 7px) 100%, 0 100%, 0 7px);
    cursor: var(--cursor-select);
    transition: background var(--dur), border-color var(--dur);
  }

  .option-row:hover {
    background: rgba(212, 176, 108, 0.07);
    border-color: rgba(212, 176, 108, 0.4);
  }

  .option-name {
    font-size: 12px;
    font-weight: 800;
  }

  .settings-switch {
    position: relative;
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    width: 42px;
    height: 23px;
  }

  .settings-switch input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .switch-track {
    position: relative;
    width: 100%;
    height: 100%;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    transition: background var(--dur), border-color var(--dur), box-shadow var(--dur);
  }

  .switch-track::after {
    content: '';
    position: absolute;
    top: 3px;
    left: 3px;
    width: 15px;
    height: 15px;
    background: var(--text-2);
    transition: transform var(--dur) var(--ease), background var(--dur);
  }

  .settings-switch input:checked + .switch-track {
    background: rgba(244, 212, 138, 0.85);
    border-color: var(--accent-gold);
    box-shadow: 0 0 12px rgba(244, 212, 138, 0.24);
  }

  .settings-switch input:checked + .switch-track::after {
    background: #111;
    transform: translateX(19px);
  }

  .settings-switch input:focus-visible + .switch-track {
    outline: 2px solid #fff;
    outline-offset: 3px;
  }

  .settings-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    margin-top: 22px;
    padding-top: 15px;
    border-top: 1px solid rgba(212, 176, 108, 0.2);
  }

  .save-note {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--text-2);
    font-size: 10px;
  }

  .save-dot {
    width: 6px;
    height: 6px;
    background: var(--green);
    box-shadow: 0 0 8px rgba(122, 202, 160, 0.65);
  }

  .done-button {
    min-width: 100px;
    padding: 10px 18px;
    color: #111;
    background: var(--accent-gold);
    clip-path: polygon(7px 0, 100% 0, calc(100% - 7px) 100%, 0 100%);
    font-size: 11px;
    font-weight: 900;
    letter-spacing: 0.08em;
    transition: background var(--dur), transform var(--dur);
  }

  .done-button:hover {
    background: #fff;
    transform: translateY(-1px);
  }

  @keyframes settings-fade-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes settings-modal-in {
    from { opacity: 0; transform: translateY(14px) scale(0.97); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  @media (max-width: 620px) {
    .settings-overlay {
      padding: calc(var(--top-h) + 8px) 12px 12px;
    }

    .settings-modal {
      width: 100%;
      padding: 20px 18px 17px;
    }

    .method-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
