<script lang="ts">
  import { marked } from 'marked';
  import { appState } from '../lib/launcherState.svelte';
  import { sanitizeReleaseNotesHtml } from '../lib/sanitize';

  interface Props {
    open?: boolean;
    version?: string;
    currentVersion?: string;
    releaseNotesBody?: string;
    prototypeMode?: boolean;
    progress?: number;
    status?: string;
    error?: string;
    restartCountdown?: number | null;
    onclose?: () => void;
  }

  let {
    open = false,
    version = '',
    currentVersion = '',
    releaseNotesBody = '',
    prototypeMode = false,
    progress = 0,
    status = '',
    error = '',
    restartCountdown = null,
    onclose,
  }: Props = $props();

  let isUpdating = $state(false);
  let prototypeComplete = $state(false);
  let updateProgress = $state(0);
  let updateStatus = $state('');

  let parsedReleaseNotes = $derived.by(() => {
    if (!releaseNotesBody.trim()) return '';
    try {
      return sanitizeReleaseNotesHtml(marked.parse(releaseNotesBody, { async: false }) as string);
    } catch {
      return sanitizeReleaseNotesHtml(releaseNotesBody);
    }
  });

  $effect(() => {
    updateProgress = progress;
    updateStatus = error || status;
    if (error) isUpdating = false;
    if (!open) {
      isUpdating = false;
      prototypeComplete = false;
    }
  });

  async function startUpdate() {
    if (isUpdating || !version) return;
    isUpdating = true;
    updateStatus = 'Mengunduh pembaruan...';
    if (prototypeMode) {
      for (const [nextProgress, nextStatus] of [
        [24, 'Mengunduh paket update...'],
        [58, 'Memverifikasi checksum...'],
        [86, 'Menyiapkan restart launcher...'],
        [100, 'Update siap diterapkan.'],
      ] as const) {
        await new Promise<void>((resolve) => window.setTimeout(resolve, 280));
        updateProgress = nextProgress;
        updateStatus = nextStatus;
      }
      isUpdating = false;
      prototypeComplete = true;
      return;
    }
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
  <div class="lu-overlay" role="presentation">
    <div class="lu-modal" id="luModal" role="dialog" aria-modal="true" aria-labelledby="luModalTitle">
      <div class="lu-modal__icon">
        <svg viewBox="0 0 24 24" width="36" height="36">
          <path fill="currentColor" d="M20 12l-1.41-1.41L13 16.17V4h-2v12.17l-5.58-5.59L4 12l8 8 8-8z" />
        </svg>
      </div>

      <h3 class="lu-modal__title" id="luModalTitle">Versi baru tersedia!</h3>
      <p class="lu-modal__ver" id="luModalVer">
        {currentVersion ? `v${currentVersion.replace(/^v/i, '')} → ` : ''}v{version.replace(/^v/i, '')}
      </p>
      <p class="lu-modal__desc">Lihat perubahan versi ini sebelum memperbarui launcher.</p>

      {#if parsedReleaseNotes}
        <section class="lu-notes" aria-labelledby="luNotesTitle">
          <div class="lu-notes__heading">
            <span id="luNotesTitle">YANG BARU DI {version}</span>
            <span>RELEASE NOTES</span>
          </div>
          <div class="lu-notes__body">
            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
            {@html parsedReleaseNotes}
          </div>
        </section>
      {/if}

      {#if restartCountdown !== null}
        <div class="lu-pbar" role="status" aria-live="assertive">
          <div class="lu-pbar__text">{restartCountdown} detik</div>
          <div class="lu-pbar__track">
            <div class="lu-pbar__fill" style="width: {Math.max(0, Math.min(100, ((12 - restartCountdown) / 12) * 100))}%"></div>
          </div>
          <div class="lu-pbar__sub">Update selesai diunduh. Launcher akan tertutup otomatis dan dibuka ulang.</div>
        </div>
      {:else if isUpdating || prototypeComplete || (status && status !== 'Menunggu konfirmasi.')}
        <div class="lu-pbar">
          <div class="lu-pbar__text">{updateProgress}%</div>
          <div class="lu-pbar__track">
            <div class="lu-pbar__fill" style="width: {updateProgress}%"></div>
          </div>
          <div class="lu-pbar__sub">{prototypeComplete ? 'Prototype selesai — launcher akan dimulai ulang.' : updateStatus || 'Menyiapkan update...'}</div>
          {#if prototypeComplete}
            <button class="lu-modal__btn lu-modal__btn--secondary lu-pbar__done" onclick={dismiss} type="button">Tutup prototype</button>
          {/if}
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

<style>
  .lu-overlay {
    background: rgba(3, 18, 21, 0.78) !important;
    backdrop-filter: blur(4px);
    -webkit-backdrop-filter: blur(4px);
  }

  .lu-modal {
    width: 380px;
    max-height: min(760px, calc(100vh - 32px));
    overflow-y: auto;
    padding: 32px 28px 24px;
    gap: 8px;
    border: 2px solid var(--mist-line-strong) !important;
    border-radius: 0 !important;
    clip-path: polygon(
      0 0,
      100% 0,
      100% calc(100% - 20px),
      calc(100% - 20px) 100%,
      0 100%
    ) !important;
    background: rgba(7, 26, 30, 0.96) !important;
    box-shadow:
      0 24px 70px rgba(0, 0, 0, 0.72),
      0 0 34px rgba(121, 203, 208, 0.14),
      inset 0 1px 0 rgba(236, 255, 249, 0.08);
  }

  .lu-modal::before,
  .lu-modal::after {
    display: block !important;
    position: absolute;
    inset: 4px;
    border: 1px solid rgba(184, 231, 230, 0.08);
    content: '';
    pointer-events: none;
  }

  .lu-modal::after {
    inset: auto 18px 8px auto;
    width: 34px;
    height: 1px;
    border: 0;
    background: var(--mist-lantern);
    box-shadow: 0 0 8px rgba(231, 211, 148, 0.42);
  }

  .lu-modal__icon {
    width: 64px;
    height: 64px;
    border: 2px solid var(--mist-line-strong) !important;
    border-radius: 0 !important;
    clip-path: polygon(
      10px 0,
      100% 0,
      100% calc(100% - 10px),
      calc(100% - 10px) 100%,
      0 100%,
      0 10px
    ) !important;
    background: rgba(121, 203, 208, 0.1) !important;
    color: var(--mist-cyan) !important;
    box-shadow: 0 0 22px rgba(121, 203, 208, 0.18) !important;
  }

  .lu-modal__ver {
    color: var(--mist-jade) !important;
    text-shadow: 0 0 10px rgba(167, 203, 181, 0.28) !important;
  }

  .lu-modal__desc {
    color: var(--text-2) !important;
  }

  .lu-notes {
    width: 100%;
    margin: 4px 0 8px;
    padding: 10px 12px;
    box-sizing: border-box;
    border: 1px solid var(--mist-line) !important;
    clip-path: polygon(
      0 0,
      calc(100% - 10px) 0,
      100% 10px,
      100% calc(100% - 10px),
      calc(100% - 10px) 100%,
      0 100%
    );
    background: rgba(7, 26, 30, 0.62) !important;
    box-shadow: inset 0 1px 0 rgba(236, 255, 249, 0.05);
    text-align: left;
  }

  .lu-notes__heading {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 6px;
    color: var(--mist-lantern);
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.12em;
  }

  .lu-notes__heading span:last-child {
    color: var(--mist-slate);
    font-weight: 600;
  }

  .lu-notes__body {
    max-height: 240px;
    overflow-y: auto;
    color: var(--text-2);
    font-size: 11px;
    line-height: 1.4;
  }

  .lu-notes__body :global(h1),
  .lu-notes__body :global(h2),
  .lu-notes__body :global(h3) {
    margin: 0 0 6px;
    color: var(--mist-jade);
    font-size: 13px;
  }

  .lu-notes__body :global(p),
  .lu-notes__body :global(ul),
  .lu-notes__body :global(ol) {
    margin: 0 0 6px;
  }

  .lu-notes__body :global(ul),
  .lu-notes__body :global(ol) {
    padding-left: 16px;
  }

  .lu-notes__body :global(a) {
    color: var(--mist-aqua);
  }

  .lu-pbar__track {
    background: rgba(170, 214, 217, 0.1) !important;
    border-color: var(--mist-line) !important;
  }

  .lu-pbar__sub {
    color: var(--mist-aqua) !important;
  }

  .lu-pbar__done {
    align-self: center;
    margin-top: 8px;
  }
</style>
