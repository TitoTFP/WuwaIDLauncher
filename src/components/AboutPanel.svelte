<script lang="ts">
  import { appState } from '../lib/launcherState.svelte';
</script>

<main class="launcher-page" aria-labelledby="about-title">
  <div class="launcher-page__head">
    <div><p class="launcher-page__eyebrow">WUWAID LAUNCHER</p><h1 id="about-title">Tentang Launcher</h1><p>Launcher Tauri untuk patch terjemahan Bahasa Indonesia Wuthering Waves.</p></div>
    <button type="button" onclick={() => (appState.page = 'home')}>Kembali</button>
  </div>
  <section class="about-card"><h2>Versi terpasang</h2><p class="version">v{appState.appVersion}</p><p>Versi Patch ID: {appState.vhVersion || 'Belum diketahui'}</p><p>Method aktif: {appState.config.installMethod}</p></section>
  <section class="about-card"><h2>Status fitur</h2><ul><li>Installer dua metode dengan validasi integritas dan rollback.</li><li>Status patch, media, dan operasi diteruskan ke UI.</li><li>Verifikasi packaging Windows tetap menjadi release gate.</li></ul></section>
  <section class="about-card"><h2>Privasi</h2><p>Launcher tidak mengirim log atau isi diagnostics. Statistik active player hanya memakai heartbeat minimal berisi client ID acak, versi launcher, metode instalasi, dan event.</p></section>
</main>

<style>
  .launcher-page{position:absolute;inset:76px 24px 24px;overflow:auto;padding:28px;color:#f6f0df;background:rgba(12,16,25,.88);border:1px solid rgba(220,188,112,.28);border-radius:14px}.launcher-page__head{display:flex;justify-content:space-between;gap:24px;margin-bottom:26px}.launcher-page__eyebrow{margin:0 0 6px;color:#dcb86b;letter-spacing:.18em;font-size:11px}h1,h2,p{margin-top:0}h1{margin-bottom:8px;font-size:28px}.launcher-page__head p:last-child,.about-card p{color:#aeb2bd;font-size:13px}button{border:1px solid rgba(220,188,112,.45);background:rgba(220,188,112,.1);color:#f6f0df;border-radius:7px;padding:9px 14px;cursor:pointer}.about-card{max-width:720px;margin-bottom:16px;padding:18px;background:rgba(255,255,255,.035);border:1px solid rgba(255,255,255,.1);border-radius:10px}.about-card h2{font-size:16px;color:#e2c780}.about-card .version{font-size:25px;color:#f6f0df}.about-card li{margin:8px 0;color:#c5c8d0}@media(max-width:700px){.launcher-page{inset:62px 12px 12px;padding:18px}.launcher-page__head{flex-direction:column}}
</style>
