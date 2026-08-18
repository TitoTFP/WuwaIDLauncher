<script lang="ts">
  import { appState } from '../lib/launcherState.svelte';

  function dismiss(id: number) {
    appState.toasts = appState.toasts.filter((toast) => toast.id !== id);
  }
</script>

<div id="toasts" aria-live="polite" aria-atomic="true">
  {#each appState.toasts as toast (toast.id)}
    <button
      class="toast show"
      class:toast--ok={toast.kind === 'ok'}
      class:toast--err={toast.kind === 'err'}
      class:toast--info={toast.kind === 'info'}
      onclick={() => dismiss(toast.id)}
      type="button"
      aria-label="Tutup notifikasi"
    >{toast.message}</button>
  {/each}
</div>
