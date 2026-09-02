<script lang="ts">
  import { appState } from '../lib/launcherState.svelte.ts';
  import { bridge } from '../lib/bridge.ts';
  import { isTauriRuntime } from '../lib/runtime';
  import { countdownExpired, shouldRunCountdown } from '../lib/countdown';
  import type { LauncherOperation } from '../lib/types';

  let dropdownOpen = $state(false);
  let now = $state(Date.now());

  let targetDateMs = $derived.by(() => {
    if (!appState.updateDate) return 0;
    const t = new Date(appState.updateDate).getTime();
    return isNaN(t) ? 0 : t;
  });

  $effect(() => {
    if (!shouldRunCountdown(targetDateMs, appState.launcherInTray)) return;
    let iv: ReturnType<typeof setInterval>;
    const tick = () => {
      const current = Date.now();
      now = current;
      if (countdownExpired(targetDateMs, current)) clearInterval(iv);
    };
    iv = setInterval(tick, 1000);
    tick();
    return () => clearInterval(iv);
  });

  let diff = $derived(Math.max(0, targetDateMs - now));
  let days = $derived(Math.floor(diff / (1000 * 86400)));
  let hours = $derived(Math.floor((diff % (1000 * 86400)) / (1000 * 3600)));
  let mins = $derived(Math.floor((diff % (1000 * 3600)) / (1000 * 60)));
  let secs = $derived(Math.floor((diff % (1000 * 60)) / 1000));
  let isDone = $derived(targetDateMs > 0 && diff <= 0);

  function pad(n: number): string {
    return String(Math.max(0, n)).padStart(2, '0');
  }

  function toggleDropdown(e: MouseEvent) {
    e.stopPropagation();
    dropdownOpen = !dropdownOpen;
  }

  function closeDropdown() {
    dropdownOpen = false;
  }

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  async function handleBrowseGameDir() {
    closeDropdown();
    if (!isTauriRuntime()) return;
    await appState.selectGameFolder();
  }

  async function handleCheckPatch() {
    closeDropdown();
    if (!isTauriRuntime()) return;
    if (!appState.gamePath) {
      appState.setStatus('Pilih folder game terlebih dahulu.');
      return;
    }
    try {
      await appState.requestPatchStatus(
        appState.gamePath,
        appState.config.installMethod,
        true,
      );
    } catch (error) {
      appState.setStatus('Gagal memeriksa status patch.', errorMessage(error));
    }
  }

  async function handleCheckLauncherUpdate() {
    closeDropdown();
    if (!isTauriRuntime()) return;
    try {
      await bridge.checkLauncherUpdate();
    } catch (error) {
      appState.setStatus('Gagal memeriksa update launcher.', errorMessage(error));
    }
  }

  async function handleForceQuit() {
    closeDropdown();
    if (!isTauriRuntime() || forceQuitDisabled) return;
    try {
      const terminated = await appState.forceQuitGame();
      if (!terminated) appState.showToast('Game tidak sedang berjalan.', 'info');
    } catch (error) {
      appState.setStatus('Gagal menutup game.', errorMessage(error));
    }
  }

  async function handleRestartAdmin() {
    closeDropdown();
    if (!isTauriRuntime()) return;
    try {
      await appState.restartAsAdmin();
    } catch (error) {
      appState.setStatus('Gagal menjalankan launcher sebagai admin.', errorMessage(error));
    }
  }

  async function handleResetCache() {
    closeDropdown();
    if (!isTauriRuntime()) return;
    try {
      await appState.resetWebViewCache();
      appState.setStatus('Cache tampilan berhasil direset.');
    } catch (error) {
      appState.setStatus('Gagal mereset cache tampilan.', errorMessage(error));
    }
  }

  async function handleSupport() {
    closeDropdown();
    if (!isTauriRuntime()) return;
    try {
      await bridge.openSupport();
      appState.showToast('Halaman dukungan dibuka di browser.', 'ok');
    } catch (error) {
      appState.setStatus('Gagal membuka halaman dukungan.', errorMessage(error));
    }
  }

  let showUninstallConfirm = $state(false);

  function promptUninstall() {
    closeDropdown();
    if (
      !appState.gamePath ||
      appState.gameRunning ||
      appState.installing ||
      appState.launching ||
      appState.isOperationBlocked('uninstall')
    ) return;
    showUninstallConfirm = true;
  }

  async function handleConfirmUninstall() {
    showUninstallConfirm = false;
    if (!isTauriRuntime()) return;
    if (
      !appState.gamePath ||
      appState.gameRunning ||
      appState.installing ||
      appState.launching ||
      appState.isOperationBlocked('uninstall')
    ) return;
    const token = appState.beginOperation('uninstall');
    if (!token) {
      appState.setStatus('Patch ID tidak dapat dihapus.', appState.getOperationBusyMessage('uninstall'));
      return;
    }
    try {
      const res = await bridge.uninstall(appState.gamePath);
      if (res === 'ok') {
        appState.patchState = 'not_installed';
        appState.installed = false;
        appState.vhVersion = '';
        appState.setStatus('Patch ID berhasil dihapus.');
      } else {
        appState.setStatus('Patch ID tidak dapat dihapus.', res);
      }
    } catch (error) {
      appState.setStatus('Gagal menghapus Patch ID.', errorMessage(error));
    } finally {
      appState.endOperation(token);
    }
  }

  function handleCancelUninstall() {
    showUninstallConfirm = false;
  }

  async function handlePrimaryAction() {
    if (!isTauriRuntime()) return;
    if (appState.gameRunning || appState.installing || appState.launching) return;

    const primaryOperation = !appState.gamePath
      ? 'folder'
      : appState.patchState === 'ready'
        ? 'launch'
        : 'install';
    if (appState.isOperationBlocked(primaryOperation)) {
      appState.setStatus(
        'Operasi launcher sedang berjalan.',
        appState.getOperationBusyMessage(primaryOperation),
      );
      return;
    }

    if (!appState.gamePath) {
      await appState.selectGameFolder();
      return;
    }

    if (appState.patchState === 'ready') {
      const token = appState.beginOperation('launch');
      if (!token) {
        appState.setStatus('Game tidak dapat dijalankan.', appState.getOperationBusyMessage('launch'));
        return;
      }
      appState.launching = true;
      try {
        await bridge.launchGame(
          appState.gamePath,
          appState.config.dx11,
          appState.config.csharpEnvironment,
          appState.config.installMethod,
        );
      } catch {
        // The backend emits onLaunchError for launch failures. Keep this path
        // to release the local guard only when the invoke itself fails.
        appState.endOperation(token);
        appState.launching = false;
      }
      return;
    }

    const token = appState.beginOperation('install');
    if (!token) {
      appState.setStatus('Instalasi tidak dapat dimulai.', appState.getOperationBusyMessage('install'));
      return;
    }
    try {
      const access = await bridge.checkGameFolderWriteAccess(
        appState.gamePath,
        appState.config.installMethod,
        true,
      );
      if (access !== 'ok') {
        appState.endOperation(token);
        if (access === 'needs_admin') {
          appState.openAdminPrompt(appState.gamePath);
          return;
        }
        appState.setStatus(
          'Folder game tidak valid atau metode tidak didukung.',
          access,
        );
        return;
      }
      appState.installing = true;
      appState.progressPercent = 0;
      appState.progressStatus = 'Memulai proses instalasi...';
      const uidMode = appState.config.uidMode;
      const uidText = appState.config.uidText;
      await bridge.startInstallation(
        appState.gamePath,
        'standard',
        appState.config.installMethod,
        { uidMode, uidText },
      );
    } catch (error) {
      appState.endOperation(token);
      appState.installing = false;
      appState.setStatus('Operasi launcher gagal.', errorMessage(error));
    }
  }

  function getButtonLabel(): string {
    if (appState.gameRunning) return 'Game sedang berjalan';
    if (appState.installing) return 'Sedang memasang...';
    if (appState.launching) return 'Memulai game...';
    if (appState.patchState === 'ready') return 'Mainkan Game';
    if (appState.patchState === 'needs_update') return 'Perbarui Patch ID';
    return 'Instal Patch ID';
  }

  let buttonLabel = $derived(getButtonLabel());
  let primaryOperation = $derived<LauncherOperation>(
    !appState.gamePath
      ? 'folder'
      : appState.patchState === 'ready'
        ? 'launch'
        : 'install',
  );
  let isDisabled = $derived(
    appState.gameRunning ||
      appState.installing ||
      appState.launching ||
      appState.isOperationBlocked(primaryOperation),
  );
  let forceQuitDisabled = $derived(
    !appState.gameRunning ||
      appState.gameOrigin !== 'launcher' ||
      appState.isOperationBlocked('force-quit'),
  );
</script>

<svelte:window onclick={closeDropdown} />

<aside class="right-panel" id="rightPanel">
  {#if appState.mediaStatus && appState.mediaStatus !== 'ready'}
    <div class="rp-status" role="status" aria-live="polite">
      <div class="rp-status__row">
        <span class="rp-status__text">{appState.mediaStatusMessage}</span>
        {#if appState.mediaProgress}<span class="rp-status__pct">{appState.mediaProgress.percent}%</span>{/if}
      </div>
      {#if appState.mediaProgress}
        <div class="rp-status__bar"><div class="progress__fill" style="width: {appState.mediaProgress.percent}%;"></div></div>
      {/if}
    </div>
  {/if}

  <!-- Countdown to Next Game Version -->
  {#if targetDateMs > 0}
    <div class="update-countdown" class:uc-done={isDone} id="updateCountdown">
      <div class="uc-label">VERSI GAME BERIKUTNYA</div>
      <div class="uc-timer">
        <div class="uc-unit">
          <span class="uc-digit" id="ucDays">{pad(days)}</span>
          <span class="uc-unit-label">Hari</span>
        </div>
        <span class="uc-sep">:</span>
        <div class="uc-unit">
          <span class="uc-digit" id="ucHours">{pad(hours)}</span>
          <span class="uc-unit-label">Jam</span>
        </div>
        <span class="uc-sep">:</span>
        <div class="uc-unit">
          <span class="uc-digit" id="ucMins">{pad(mins)}</span>
          <span class="uc-unit-label">Menit</span>
        </div>
        <span class="uc-sep">:</span>
        <div class="uc-unit">
          <span class="uc-digit" id="ucSecs">{pad(secs)}</span>
          <span class="uc-unit-label">Detik</span>
        </div>
      </div>
      <div class="uc-bar">
        <div
          class="uc-bar__fill"
          id="ucBarFill"
          style="width: {isDone ? 100 : Math.min(100, Math.max(0, (1 - diff / (6 * 7 * 24 * 3600 * 1000)) * 100))}%"
        ></div>
      </div>
    </div>
  {/if}

  <!-- Progress Section when Installing -->
  {#if appState.installing}
    <div class="progress" id="progressSection">
      <div class="progress__head">
        <span id="progressText">{appState.progressStatus}</span>
        <span id="progressPct">{appState.progressPercent}%</span>
      </div>
      <div class="progress__track">
        <div class="progress__fill" style="width: {appState.progressPercent}%;"></div>
      </div>
      <div class="progress__foot">
        <span id="progressSpeed">{appState.progressSpeedMbps > 0 ? `${appState.progressSpeedMbps.toFixed(1)} MB/s` : ''}</span>
        <span id="progressSize">{appState.progressTotalBytes > 0 ? `${(appState.progressDownloadedBytes / 1048576).toFixed(1)} / ${(appState.progressTotalBytes / 1048576).toFixed(1)} MB` : ''}</span>
      </div>
    </div>
  {/if}

  <!-- Dropdown Menu -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="rp-dropdown" class:open={dropdownOpen} id="rpDropdown" onclick={(e) => e.stopPropagation()}>
    <button class="rp-dropdown__item" id="menuGameDir" onclick={handleBrowseGameDir} disabled={appState.isOperationBlocked('folder')} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M10 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z" />
      </svg>
      Folder game
    </button>

    <button class="rp-dropdown__item" id="menuCheckVH" onclick={handleCheckPatch} disabled={appState.patchStatusCheckPending || appState.isOperationBlocked('install') || appState.isOperationBlocked('method-switch') || appState.isOperationBlocked('folder')} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 14H9V8h2v8zm4 0h-2V8h2v8z" />
      </svg>
      Perbarui Patch ID
    </button>

    <button class="rp-dropdown__item" id="menuCheckUpdate" onclick={handleCheckLauncherUpdate} disabled={appState.isOperationBlocked('launcher-update')} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M20 12l-1.41-1.41L13 16.17V4h-2v12.17l-5.58-5.59L4 12l8 8 8-8z" />
      </svg>
      <span id="menuCheckUpdateText">Perbarui Launcher</span>
    </button>

    <button class="rp-dropdown__item" id="menuForceQuit" onclick={handleForceQuit} disabled={forceQuitDisabled} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
      </svg>
      Paksa tutup game
    </button>

    <button class="rp-dropdown__item" id="menuRestartAdmin" onclick={handleRestartAdmin} disabled={appState.isOperationBlocked('restart-as-admin')} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 4l5 2.18V11c0 3.5-2.33 6.79-5 7.93-2.67-1.14-5-4.43-5-7.93V7.18L12 5z" />
      </svg>
      Jalankan sebagai Admin
    </button>

    <button class="rp-dropdown__item" id="menuResetCache" onclick={handleResetCache} disabled={appState.isOperationBlocked('cache-reset')} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M12 4V1L8 5l4 4V6a6 6 0 1 1-5.65 4H4.26A8 8 0 1 0 12 4z" />
      </svg>
      Reset cache tampilan
    </button>

    <button class="rp-dropdown__item rp-dropdown__item--trakteer" id="menuDukung" onclick={handleSupport} title="Dukung Kami di Trakteer" type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M7 3h8.5a4.5 4.5 0 0 1 .5 8.97V13a5 5 0 0 1-5 5H7a5 5 0 0 1-5-5V8a5 5 0 0 1 5-5Zm9 2.1v4.8A2.5 2.5 0 0 0 16 5.1ZM7.1 8.2c-1.1 0-1.9.83-1.9 1.88 0 2.18 3.52 4.1 3.8 4.1s3.8-1.92 3.8-4.1c0-1.05-.8-1.88-1.9-1.88-.74 0-1.45.43-1.9 1.08-.45-.65-1.16-1.08-1.9-1.08ZM5 20h12v2H5v-2Z" />
      </svg>
      Dukung Kami
    </button>

    <div class="rp-dropdown__sep"></div>

    <button class="rp-dropdown__item rp-dropdown__item--danger" id="menuUninstall" onclick={promptUninstall} disabled={appState.isOperationBlocked('uninstall')} type="button">
      <svg viewBox="0 0 24 24" width="14" height="14">
        <path fill="currentColor" d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" />
      </svg>
      Hapus Patch ID
    </button>
  </div>

  {#if dropdownOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="rp-dropdown-overlay" onclick={closeDropdown}></div>
  {/if}

  <!-- Actions: Hamburger Menu + Start Button -->
  <div class="rp-actions">
    <button class="rp-menu__btn" class:active={dropdownOpen} id="btnMenu" onclick={toggleDropdown} title="Menu" type="button">
      <svg viewBox="0 0 24 24" width="18" height="18">
        <path fill="currentColor" d="M3 18h18v-2H3v2zm0-5h18v-2H3v2zm0-7v2h18V6H3z" />
      </svg>
    </button>

    <button
      class="start-btn"
      class:installed={appState.patchState === 'ready'}
      class:installing={appState.installing}
      class:disabled={isDisabled}
      id="btnStart"
      disabled={isDisabled}
      onclick={handlePrimaryAction}
      type="button"
    >
      <span class="start-btn__shine"></span>
      <span class="start-btn__label" id="startBtnText">{buttonLabel}</span>
    </button>
  </div>

  <!-- Version Footer -->
  <div class="rp-version">
    <span id="verApp">Launcher v{appState.appVersion}</span>
    <span id="verVH">{appState.vhVersion ? `ID ${appState.vhVersion}` : ''}</span>
  </div>
</aside>

{#if showUninstallConfirm}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div id="confirmModal" class="modal-backdrop" onclick={handleCancelUninstall}>
    <div class="modal-box" onclick={(e) => e.stopPropagation()}>
      <div class="modal-title">
        <svg viewBox="0 0 24 24" width="18" height="18" style="flex-shrink:0"><path fill="#E8C45A" d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z"/></svg>
        <span>Konfirmasi Hapus Patch</span>
      </div>
      <div class="modal-sep"></div>
      <p class="modal-msg" id="modalMsg">
        Apakah Anda yakin ingin menghapus Patch ID Bahasa Indonesia dari folder game? Seluruh file mod akan dibersihkan dan signature asli akan dipulihkan.
      </p>
      <div class="modal-actions">
        <button class="modal-btn modal-btn--cancel" id="modalCancel" onclick={handleCancelUninstall} type="button">Batal</button>
        <button class="modal-btn modal-btn--ok" id="modalOk" onclick={handleConfirmUninstall} disabled={appState.isOperationBlocked('uninstall')} type="button">Hapus Patch</button>
      </div>
    </div>
  </div>
{/if}
