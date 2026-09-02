<script lang="ts">
  import { slide } from 'svelte/transition';
  import { appState } from '../lib/launcherState.svelte.ts';
  import {
    INSTALL_METHOD_OPTIONS,
    isEffectivelyEmptyUidText,
    isValidUidText,
    MAX_UID_TEXT_LENGTH,
  } from '../lib/types.ts';
  import type { InstallMethod, UidMode } from '../lib/types.ts';

  interface Props {
    open?: boolean;
    onclose?: () => void;
  }

  let { open = false, onclose }: Props = $props();

  let uidPreview = $derived(
    appState.config.uidMode === 'default'
      ? 'ID Pengguna: {0}'
      : isEffectivelyEmptyUidText(appState.config.uidText)
        ? 'UID disembunyikan'
        : appState.config.uidText,
  );

  let uidDisabled = $derived(
    appState.gameRunning ||
      appState.launching ||
      appState.isOperationBlocked('folder') ||
      appState.isOperationBlocked('method-switch') ||
      appState.isOperationBlocked('install'),
  );

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

  function applyUidSelection(mode: UidMode, text: string) {
    if (uidDisabled) {
      appState.setStatus('Identitas UID tidak dapat diubah saat operasi berjalan.');
      return;
    }
    void appState.updateUidSelection(mode, text).catch((error) => {
      appState.setStatus('Identitas UID tidak dapat diterapkan.', errorMessage(error));
    });
  }

  function selectUidMode(mode: UidMode) {
    applyUidSelection(mode, appState.config.uidText);
  }

  function handleCustomUidInput(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    if (!isValidUidText(input.value)) {
      input.value = appState.config.uidText;
      appState.setStatus(
        `Teks UID harus satu baris dan maksimal ${MAX_UID_TEXT_LENGTH} karakter.`,
      );
      return;
    }
    applyUidSelection('custom', input.value);
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

      <section class="settings-section" aria-labelledby="uidHeading">
        <h2 class="section-title" id="uidHeading">IDENTITAS UID</h2>
        <div class="uid-editor">
          <div class="uid-mode-grid" role="group" aria-label="Mode tampilan UID">
            <button
              class="uid-mode-card"
              class:active={appState.config.uidMode === 'default'}
              aria-pressed={appState.config.uidMode === 'default'}
              disabled={uidDisabled}
              onclick={() => selectUidMode('default')}
              type="button"
            >
              <span class="uid-mode-card__top">
                <span class="uid-mode-card__title">DEFAULT</span>
                <span class="uid-mode-card__mark" aria-hidden="true">✓</span>
              </span>
            </button>
            <button
              class="uid-mode-card"
              class:active={appState.config.uidMode === 'custom'}
              aria-pressed={appState.config.uidMode === 'custom'}
              disabled={uidDisabled}
              onclick={() => selectUidMode('custom')}
              type="button"
            >
              <span class="uid-mode-card__top">
                <span class="uid-mode-card__title">CUSTOM</span>
                <span class="uid-mode-card__mark" aria-hidden="true">✓</span>
              </span>
            </button>
          </div>

          {#if appState.config.uidMode === 'custom'}
            <div class="uid-input-field" transition:slide={{ duration: 180 }}>
              <label class="uid-input-label" for="settingsCustomUid">TEKS CUSTOM</label>
              <div class="uid-input-shell">
                <input
                  id="settingsCustomUid"
                  class="uid-input"
                  type="text"
                  value={appState.config.uidText}
                  autocomplete="off"
                  placeholder={'ID Pengguna: {0}'}
                  disabled={uidDisabled}
                  oninput={handleCustomUidInput}
                />
              </div>
            </div>
          {/if}

          <div class="uid-preview">
            <div class="uid-preview__copy">
              <span class="uid-preview__label">PREVIEW DI GAME</span>
              <strong>{uidPreview}</strong>
            </div>
          </div>

        </div>
        <p class="uid-note"><span aria-hidden="true">i</span> Perubahan tersimpan otomatis dan berlaku saat patch dipasang ulang.</p>
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

  .uid-editor {
    padding: 14px;
    background: rgba(255, 255, 255, 0.025);
    border: 1px solid rgba(212, 176, 108, 0.24);
    clip-path: polygon(8px 0, 100% 0, 100% calc(100% - 8px), calc(100% - 8px) 100%, 0 100%, 0 8px);
  }

  .uid-preview {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .uid-input-label,
  .uid-preview__label {
    color: var(--text-1);
    font-size: 10px;
    font-weight: 900;
    letter-spacing: 0.11em;
  }

  .uid-mode-grid {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }

  .uid-mode-card {
    min-height: 0;
    flex: 0 0 auto;
    padding: 8px 10px;
    color: var(--text-2);
    text-align: left;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(212, 176, 108, 0.2);
    clip-path: polygon(7px 0, 100% 0, 100% calc(100% - 7px), calc(100% - 7px) 100%, 0 100%, 0 7px);
    transition: background var(--dur), border-color var(--dur), color var(--dur), transform var(--dur);
  }

  .uid-mode-card:hover:not(:disabled) {
    color: var(--text-1);
    background: rgba(212, 176, 108, 0.07);
    border-color: rgba(212, 176, 108, 0.5);
    transform: translateY(-1px);
  }

  .uid-mode-card:disabled {
    cursor: default;
    opacity: 0.58;
  }

  .uid-mode-card.active {
    color: #111;
    background: linear-gradient(135deg, #aad6d9, #e7d394);
    border-color: var(--accent-gold);
    box-shadow: 0 8px 20px rgba(244, 212, 138, 0.12);
  }

  .uid-mode-card__top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .uid-mode-card__title {
    font-size: 11px;
    font-weight: 900;
    letter-spacing: 0.06em;
  }

  .uid-mode-card__mark {
    display: grid;
    place-items: center;
    width: 15px;
    height: 15px;
    color: transparent;
    border: 1px solid currentColor;
    font-size: 10px;
  }

  .uid-mode-card.active .uid-mode-card__mark {
    color: #111;
    background: rgba(255, 255, 255, 0.28);
  }

  .uid-input-field {
    margin-top: 12px;
  }

  .uid-input-shell {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 40px;
    margin-top: 6px;
    padding: 0 11px;
    background: rgba(4, 12, 17, 0.5);
    border: 1px solid rgba(212, 176, 108, 0.38);
    transition: border-color var(--dur), box-shadow var(--dur);
  }

  .uid-input-shell:focus-within {
    border-color: var(--accent-gold);
    box-shadow: 0 0 14px rgba(244, 212, 138, 0.1);
  }

  .uid-input {
    min-width: 0;
    flex: 1;
    height: 100%;
    padding: 0;
    color: var(--text-1);
    background: transparent;
    border: 0;
    outline: 0;
    user-select: text;
    font-size: 14px;
    font-weight: 800;
    letter-spacing: 0.04em;
  }

  .uid-input::placeholder {
    color: var(--text-3);
  }

  .uid-preview {
    margin-top: 12px;
    padding: 10px 11px;
    background: rgba(121, 203, 208, 0.07);
    border-left: 2px solid var(--accent-orange);
  }

  .uid-preview__copy {
    min-width: 0;
    display: grid;
    gap: 2px;
  }

  .uid-preview__label {
    color: var(--accent-orange);
    font-size: 8px;
  }

  .uid-preview strong {
    overflow: hidden;
    color: var(--text-1);
    font-size: 13px;
    letter-spacing: 0.04em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .uid-note {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 2px 0;
    color: var(--text-3);
    font-size: 9px;
  }

  .uid-note span {
    display: grid;
    place-items: center;
    width: 13px;
    height: 13px;
    color: var(--accent-orange);
    border: 1px solid currentColor;
    font-size: 8px;
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

    .uid-mode-grid {
      flex-direction: column;
    }

    .uid-mode-card {
      width: 100%;
    }

  }
</style>
