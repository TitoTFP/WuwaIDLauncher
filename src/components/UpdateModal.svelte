<script lang="ts">
  import { bridge } from '../lib/bridge';
  import { appState } from '../lib/launcherState.svelte';

  interface Props {
    open?: boolean;
    version?: string;
    zipUrl?: string;
    checksumsUrl?: string;
    progress?: number;
    status?: string;
    error?: string;
    onclose?: () => void;
  }

  let { open = false, version = '', zipUrl = '', checksumsUrl = '', progress = 0, status = '', error = '', onclose }: Props = $props();

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
    if (isUpdating || !version || !zipUrl) return;
    isUpdating = true;
    updateStatus = 'Mengunduh pembaruan...';
    try {
      await bridge.performLauncherUpdate(version, zipUrl, checksumsUrl || undefined);
    } catch (err) {
      isUpdating = false;
      onclose?.();
      appState.showToast(`Pembaruan gagal: ${err instanceof Error ? err.message : String(err)}`, 'err');
    }
  }

  function dismiss() {
    if (isUpdating) return;
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

      {#if isUpdating || (status && status !== 'Menunggu konfirmasi.')}
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
          <button class="lu-modal__btn lu-modal__btn--primary" onclick={startUpdate} type="button">Perbarui sekarang</button>
        </div>
      {/if}
    </div>
  </div>
{/if}
