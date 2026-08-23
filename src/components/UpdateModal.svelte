<script lang="ts">
  import { appState } from '../lib/launcherState.svelte';

  interface Props {
    open?: boolean;
    version?: string;
    progress?: number;
    status?: string;
    error?: string;
    restartCountdown?: number | null;
    onclose?: () => void;
  }

  let { open = false, version = '', progress = 0, status = '', error = '', restartCountdown = null, onclose }: Props = $props();

  let isUpdating = $state(false);
  let updateProgress = $state(0);
  let updateStatus = $state('');

  $effect(() => {
    updateProgress = progress;
    updateStatus = error || status;
    if (error) isUpdating = false;
    if (!open) isUpdating = false;
  });

  async function startUpdate() {
    if (isUpdating || !version) return;
    isUpdating = true;
    updateStatus = 'Mengunduh pembaruan...';
    try {
      await appState.performLauncherUpdate(version);
    } catch (err) {
      isUpdating = false;
      onclose?.();
      appState.showToast(`Pembaruan gagal: ${err instanceof Error ? err.message : String(err)}`, 'err');
    }
  }

  function dismiss() {
    if (isUpdating || restartCountdown !== null) return;
    onclose?.();
  }
</script>

{#if open}
  <div class="lu-overlay" aria-modal="true">
    <div class="lu-modal" id="luModal">
      <div class="lu-modal__icon">
        <svg viewBox="0 0 24 24" width="36" height="36">
          <path fill="currentColor" d="M20 12l-1.41-1.41L13 16.17V4h-2v12.17l-5.58-5.59L4 12l8 8 8-8z" />
        </svg>
      </div>

      <h3 class="lu-modal__title">Versi baru tersedia!</h3>
      <p class="lu-modal__ver" id="luModalVer">{version}</p>
      <p class="lu-modal__desc">Apakah Anda ingin memperbarui Launcher sekarang?</p>

      {#if restartCountdown !== null}
        <div class="lu-pbar lu-pbar--restart" role="status" aria-live="assertive">
          <div class="lu-pbar__text">{restartCountdown} detik</div>
          <div class="lu-pbar__track">
            <div class="lu-pbar__fill" style="width: {Math.max(0, Math.min(100, ((12 - restartCountdown) / 12) * 100))}%"></div>
          </div>
          <div class="lu-pbar__sub">Update selesai diunduh. Launcher akan tertutup otomatis dan dibuka ulang.</div>
        </div>
      {:else if isUpdating || (status && status !== 'Menunggu konfirmasi.')}
        <div class="lu-pbar">
          <div class="lu-pbar__text">{updateProgress}%</div>
          <div class="lu-pbar__track">
            <div class="lu-pbar__fill" style="width: {updateProgress}%"></div>
          </div>
          <div class="lu-pbar__sub">{updateStatus || 'Menyiapkan update...'}</div>
        </div>
      {:else}
        <div class="lu-modal__btns">
          <button class="lu-modal__btn lu-modal__btn--secondary" onclick={dismiss} type="button">Nanti</button>
          <button class="lu-modal__btn lu-modal__btn--primary" onclick={startUpdate} disabled={appState.isOperationBlocked('launcher-update')} type="button">Perbarui sekarang</button>
        </div>
      {/if}
    </div>
  </div>
{/if}
