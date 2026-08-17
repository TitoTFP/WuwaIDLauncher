<script lang="ts">
  import { bridge } from '../lib/bridge';
  import { appState } from '../lib/launcherState.svelte';

  function errorMessage(error: unknown): string { return error instanceof Error ? error.message : String(error); }

  async function upload() {
    if (!appState.gamePath || appState.logUploadActive) return;
    try { await bridge.uploadLogs(appState.gamePath); }
    catch (error) { appState.setStatus('Gagal memulai upload log.', errorMessage(error)); }
  }

  async function resetCache() {
    try { await bridge.resetWebViewCache(); appState.setStatus('Cache WebView berhasil direset.'); }
    catch (error) { appState.setStatus('Gagal mereset cache WebView.', errorMessage(error)); }
  }
</script>

<main class="launcher-page" aria-labelledby="logs-title">
  <div class="launcher-page__head">
    <div><p class="launcher-page__eyebrow">DIAGNOSTICS</p><h1 id="logs-title">Log & Diagnostics</h1><p>Upload hanya dimulai melalui aksi eksplisit dan statusnya ditampilkan di sini.</p></div>
    <button type="button" onclick={() => (appState.page = 'home')}>Kembali</button>
  </div>
  <section class="log-card">
    <h2>Status upload</h2>
    <p role="status">{appState.logUploadActive ? 'Sedang mengunggah log...' : appState.logUploadStatus || 'Belum ada upload pada sesi ini.'}</p>
    {#if !appState.config.diagnosticsUploadEnabled}<p class="log-note">Upload dinonaktifkan. Aktifkan izin di Pengaturan untuk mengirim bundle diagnostik.</p>{/if}
    {#if appState.logUploadLocalPath}<p class="log-note">Bundle lokal tersedia: <code>{appState.logUploadLocalPath}</code></p>{/if}
    <div class="log-actions"><button type="button" onclick={upload} disabled={!appState.gamePath || appState.logUploadActive || !appState.config.diagnosticsUploadEnabled}>Kirim log diagnostik</button><button type="button" onclick={resetCache}>Reset cache WebView</button></div>
  </section>
  <section class="log-card">
    <h2>Informasi sesi</h2>
    <dl><dt>Folder game</dt><dd>{appState.gamePath || 'Belum dipilih'}</dd><dt>Status patch</dt><dd>{appState.patchState}</dd><dt>Versi patch</dt><dd>{appState.vhVersion || 'Belum diketahui'}</dd><dt>Telemetry</dt><dd>{appState.telemetryStatusMessage || (appState.config.telemetryEnabled ? 'Menunggu status.' : 'Nonaktif')}</dd></dl>
  </section>
</main>

<style>
  .launcher-page { position: absolute; inset: 76px 24px 24px; overflow: auto; padding: 28px; color: #f6f0df; background: rgba(12,16,25,.88); border: 1px solid rgba(220,188,112,.28); border-radius: 14px; }
  .launcher-page__head { display:flex; justify-content:space-between; gap:24px; margin-bottom:26px; } .launcher-page__eyebrow { margin:0 0 6px; color:#dcb86b; letter-spacing:.18em; font-size:11px; } h1,h2,p { margin-top:0; } h1{margin-bottom:8px;font-size:28px}.launcher-page__head p:last-child{color:#aeb2bd;font-size:13px} button{border:1px solid rgba(220,188,112,.45);background:rgba(220,188,112,.1);color:#f6f0df;border-radius:7px;padding:9px 14px;cursor:pointer}button:hover{background:rgba(220,188,112,.22)}button:disabled{opacity:.5;cursor:not-allowed}.log-card{max-width:720px;margin-bottom:16px;padding:18px;background:rgba(255,255,255,.035);border:1px solid rgba(255,255,255,.1);border-radius:10px}.log-card h2{font-size:16px;color:#e2c780}.log-actions{display:flex;gap:10px;flex-wrap:wrap}.log-note{color:#aeb2bd;font-size:12px;overflow-wrap:anywhere}code{color:#dcb86b}dl{display:grid;grid-template-columns:140px 1fr;gap:9px;font-size:13px}dt{color:#aeb2bd}dd{margin:0;overflow-wrap:anywhere}@media(max-width:700px){.launcher-page{inset:62px 12px 12px;padding:18px}.launcher-page__head{flex-direction:column}}
</style>
