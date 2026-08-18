<script lang="ts">
  import { bridge } from '../lib/bridge';
  import { appState } from '../lib/launcherState.svelte';

  async function restartAsAdmin() {
    appState.closeAdminPrompt();
    try {
      await bridge.restartAsAdmin();
    } catch (error) {
      appState.showToast(`Gagal menjalankan launcher sebagai admin: ${error instanceof Error ? error.message : String(error)}`, 'err');
    }
  }
</script>

{#if appState.adminPromptOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div id="adminModal" class="modal-backdrop" onclick={() => appState.closeAdminPrompt()}>
    <div class="modal-box admin-modal" onclick={(event) => event.stopPropagation()}>
      <div class="admin-modal__header">
        <div class="admin-modal__shield">
          <svg viewBox="0 0 24 24" width="28" height="28"><path fill="currentColor" d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm0 4l5 2.18V11c0 3.5-2.33 6.79-5 7.93C9.33 17.79 7 14.5 7 11V7.18L12 5z" /></svg>
        </div>
        <div class="admin-modal__titles">
          <div class="admin-modal__label">MEMBUTUHKAN IZIN</div>
          <div class="admin-modal__title">Administrator</div>
        </div>
      </div>
      <div class="admin-modal__sep"></div>
      <p class="admin-modal__reason">
        Folder game berada di lokasi yang dilindungi Windows
        <span class="admin-modal__path">{appState.adminPromptPath}</span>
        jadi Launcher memerlukan izin Admin untuk menulis file di sana.
      </p>
      <div class="admin-modal__sep"></div>
      <div class="admin-modal__actions">
        <button class="modal-btn modal-btn--cancel" id="adminModalCancel" onclick={() => appState.closeAdminPrompt()} type="button">Tidak perlu</button>
        <button class="modal-btn modal-btn--ok admin-modal__ok" id="adminModalOk" onclick={restartAsAdmin} type="button">
          <svg viewBox="0 0 24 24" width="14" height="14"><path fill="currentColor" d="M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4z" /></svg>
          Mulai ulang sebagai Admin
        </button>
      </div>
    </div>
  </div>
{/if}
