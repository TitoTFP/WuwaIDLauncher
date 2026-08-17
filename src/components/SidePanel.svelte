<script lang="ts">
  import { appState } from '../lib/launcherState.svelte.ts';
  import { marked } from 'marked';
  import { INSTALL_METHOD_OPTIONS } from '../lib/types';
  import { sanitizeReleaseNotesHtml } from '../lib/sanitize';

  let collapsed = $state(false);
  let methodLabel = $derived(
    INSTALL_METHOD_OPTIONS.find((option) => option.value === appState.config.installMethod)?.title ?? 'Metode tidak diketahui',
  );

  // SidePanel toggle handler
  function toggle() {
    collapsed = !collapsed;
  }

  let parsedHtml = $derived.by(() => {
    if (!appState.releaseNotes?.body) return '';
    try {
      return sanitizeReleaseNotesHtml(marked.parse(appState.releaseNotes.body, { async: false }) as string);
    } catch {
      return sanitizeReleaseNotesHtml(appState.releaseNotes.body);
    }
  });
</script>

<div class="side-panel" class:collapsed id="sidePanel">
  <div class="rn-content">
    <div class="rn-head">
      <div class="rn-head__left">
        <span class="rn-tag" id="rnTag">ID</span>
        <span class="rn-head__title">PATCH ID - PENGUMUMAN</span>
      </div>
      <span class="rn-date" id="rnDate">
        {appState.releaseNotes ? appState.releaseNotes.tag : `v${appState.appVersion}`}
      </span>
    </div>
    <div class="rn-sep"></div>
    <div class="rn-body" id="rnBody">
      {#if appState.releaseNotesLoading && !appState.releaseNotes}
        <div class="rn-loading">
          <span></span><span></span><span></span>
        </div>
      {:else if appState.releaseNotes}
        <!-- eslint-disable-next-line svelte/no-at-html-tags -->
        {@html parsedHtml}
      {:else}
        <div class="rn-text">
          <p><strong>Selamat datang di WuwaID Launcher!</strong></p>
          <p>Nikmati petualangan di Sol3 dengan patch terjemahan Bahasa Indonesia untuk Wuthering Waves.</p>
          <br />
          <p>• Versi Launcher: v{appState.appVersion}</p>
          <p>• Metode: {methodLabel}</p>
          <p>• Status Patch: {appState.patchState === 'ready' ? 'Siap digunakan' : appState.patchState === 'not_installed' ? 'Belum terpasang' : appState.patchState}</p>
        </div>
      {/if}
    </div>
  </div>

  <button class="rn-toggle" id="rnToggle" title="Tutup / Buka" onclick={toggle} type="button">
    <svg viewBox="0 0 24 24" width="11" height="11">
      <path fill="currentColor" d="M15.41 7.41L14 6l-6 6 6 6 1.41-1.41L10.83 12z" />
    </svg>
  </button>
</div>
