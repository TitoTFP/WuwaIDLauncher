<script lang="ts">
  import { bridge } from '../lib/bridge';
  import { appState } from '../lib/launcherState.svelte';
  import { INSTALL_METHOD_OPTIONS } from '../lib/types';
  import type { InstallMethod, VisualMode } from '../lib/types';

  let saving = $state(false);

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  async function persist(message: string): Promise<boolean> {
    saving = true;
    try {
      await appState.saveConfig();
      appState.setStatus(message);
      return true;
    } catch (error) {
      appState.setStatus('Pengaturan tidak dapat disimpan.', errorMessage(error));
      return false;
    } finally {
      saving = false;
    }
  }

  async function changeMethod(event: Event) {
    const method = (event.currentTarget as HTMLSelectElement).value as InstallMethod;
    const previous = appState.config.installMethod;
    try {
      if (appState.gamePath) {
        const report = await bridge.switchMethod(appState.gamePath, method);
        if (report.failures.length || report.preserved.length) {
          throw new Error([...report.failures, ...report.preserved].join('; '));
        }
      }
      appState.config.installMethod = method;
      if (!(await persist('Metode instalasi diperbarui.'))) {
        appState.config.installMethod = previous;
        return;
      }
      if (appState.gamePath) {
        await bridge.checkPatchStatus(appState.gamePath, method);
      }
    } catch (error) {
      appState.config.installMethod = previous;
      appState.setStatus('Gagal mengganti metode instalasi.', errorMessage(error));
    }
  }

  async function changeVisual(event: Event) {
    const previous = appState.config.launcherVisualMode;
    appState.config.launcherVisualMode = (event.currentTarget as HTMLSelectElement).value as VisualMode;
    appState.visualMode = appState.config.launcherVisualMode;
    if (!(await persist('Mode visual diperbarui.'))) {
      appState.config.launcherVisualMode = previous;
      appState.visualMode = previous;
    }
  }

  async function changeVolume(event: Event) {
    const previous = appState.config.bgmVolume;
    const next = Number((event.currentTarget as HTMLInputElement).value);
    appState.config.bgmVolume = next;
    appState.bgmVolume = appState.config.bgmVolume;
    if (!(await persist('Volume BGM diperbarui.'))) {
      appState.config.bgmVolume = previous;
      appState.bgmVolume = previous;
    }
  }

  async function changeBoolean(
    key: 'dx11' | 'bgmEnabled' | 'autoCheckUpdate' | 'diagnosticsUploadEnabled' | 'telemetryEnabled',
    event: Event,
    message: string,
  ) {
    const previous = appState.config[key];
    appState.config[key] = (event.currentTarget as HTMLInputElement).checked;
    if (!(await persist(message))) appState.config[key] = previous;
  }

  async function chooseGameFolder() {
    try {
      const selected = await bridge.browseGameFolder();
      if (selected === '?INVALID') {
        appState.setStatus('Folder game yang dipilih tidak valid.');
        return;
      }
      if (selected) {
        const previous = appState.gamePath;
        appState.gamePath = selected;
        try {
          await appState.saveConfig();
          await bridge.checkPatchStatus(selected, appState.config.installMethod);
          appState.setStatus('Folder game diperbarui.');
        } catch (error) {
          appState.gamePath = previous;
          appState.config.gamePath = previous;
          throw error;
        }
      }
    } catch (error) {
      appState.setStatus('Gagal memilih folder game.', errorMessage(error));
    }
  }
</script>

<main class="settings-page launcher-page" aria-labelledby="settings-title">
  <div class="launcher-page__head">
    <div>
      <p class="launcher-page__eyebrow">PREFERENCES</p>
      <h1 id="settings-title">Pengaturan Launcher</h1>
      <p>Semua pilihan di sini disimpan melalui schema konfigurasi yang sama dengan backend.</p>
    </div>
    <button type="button" class="launcher-page__back" onclick={() => (appState.page = 'home')}>Kembali</button>
  </div>

  <div class="settings-grid">
    <section class="settings-card">
      <h2>Patch & game</h2>
      <label>
        <span>Folder game</span>
        <div class="settings-inline">
          <input value={appState.gamePath || 'Belum dipilih'} readonly aria-label="Folder game" />
          <button type="button" onclick={chooseGameFolder}>Pilih</button>
        </div>
      </label>
      <label>
        <span>Metode instalasi</span>
        <select value={appState.config.installMethod} onchange={changeMethod} disabled={appState.gameRunning || saving}>
          {#each INSTALL_METHOD_OPTIONS as option}
            <option value={option.value}>{option.title} — {option.description}</option>
          {/each}
        </select>
      </label>
      <label class="settings-check">
        <input type="checkbox" checked={appState.config.dx11} onchange={(event) => changeBoolean('dx11', event, 'Mode DX11 diperbarui.')} />
        <span>Gunakan DX11 saat menjalankan game</span>
      </label>
    </section>

    <section class="settings-card">
      <h2>Visual & audio</h2>
      <label>
        <span>Mode visual</span>
        <select value={appState.config.launcherVisualMode} onchange={changeVisual}>
          <option value="full">Full — video dan efek</option>
          <option value="light">Light — efek ringan</option>
          <option value="off">Off — tanpa efek</option>
        </select>
      </label>
      <label>
        <span>Volume BGM: {Math.round(appState.config.bgmVolume * 100)}%</span>
        <input type="range" min="0" max="1" step="0.01" value={appState.config.bgmVolume} onchange={changeVolume} />
      </label>
      <label class="settings-check">
        <input type="checkbox" checked={appState.config.bgmEnabled} onchange={(event) => changeBoolean('bgmEnabled', event, 'Preferensi BGM diperbarui.')} />
        <span>Aktifkan musik latar</span>
      </label>
    </section>

    <section class="settings-card">
      <h2>Update</h2>
      <label class="settings-check">
        <input type="checkbox" checked={appState.config.autoCheckUpdate} onchange={(event) => changeBoolean('autoCheckUpdate', event, 'Preferensi update diperbarui.')} />
        <span>Periksa update launcher otomatis</span>
      </label>
      <p class="settings-note">Saat game berjalan, perubahan metode dan halaman konfigurasi ditahan sampai game selesai.</p>
    </section>

    <section class="settings-card">
      <h2>Diagnostics & privacy</h2>
      <label class="settings-check">
        <input type="checkbox" checked={appState.config.diagnosticsUploadEnabled} onchange={(event) => changeBoolean('diagnosticsUploadEnabled', event, 'Preferensi upload diagnostics diperbarui.')} />
        <span>Izinkan upload log diagnostik</span>
      </label>
      <label class="settings-check">
        <input type="checkbox" checked={appState.config.telemetryEnabled} onchange={(event) => changeBoolean('telemetryEnabled', event, 'Preferensi telemetry diperbarui.')} />
        <span>Izinkan telemetry anonim</span>
      </label>
      <p class="settings-note" role="status">Status telemetry: {appState.telemetryStatusMessage || (appState.config.telemetryEnabled ? 'Menunggu event game.' : 'Nonaktif.')}</p>
      <p class="settings-note">Default kedua opsi ini nonaktif. Payload log disensor sebelum upload dan kegagalan tetap menghasilkan bundle lokal.</p>
    </section>
  </div>

  {#if saving}<p class="settings-saving" role="status">Menyimpan...</p>{/if}
</main>

<style>
  .launcher-page { position: absolute; inset: 76px 24px 24px 24px; overflow: auto; padding: 28px; color: #f6f0df; background: rgba(12, 16, 25, .88); border: 1px solid rgba(220, 188, 112, .28); border-radius: 14px; }
  .launcher-page__head { display: flex; justify-content: space-between; gap: 24px; align-items: flex-start; margin-bottom: 26px; }
  .launcher-page__eyebrow { margin: 0 0 6px; color: #dcb86b; letter-spacing: .18em; font-size: 11px; }
  h1, h2, p { margin-top: 0; }
  h1 { margin-bottom: 8px; font-size: 28px; }
  .launcher-page__head p:last-child, .settings-note { color: #aeb2bd; font-size: 13px; }
  .launcher-page__back, button { border: 1px solid rgba(220, 188, 112, .45); background: rgba(220, 188, 112, .1); color: #f6f0df; border-radius: 7px; padding: 9px 14px; cursor: pointer; }
  button:hover { background: rgba(220, 188, 112, .22); }
  button:disabled { opacity: .5; cursor: not-allowed; }
  .settings-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 16px; }
  .settings-card { padding: 18px; background: rgba(255,255,255,.035); border: 1px solid rgba(255,255,255,.1); border-radius: 10px; }
  .settings-card h2 { font-size: 16px; color: #e2c780; }
  label { display: block; margin: 16px 0; color: #d6d8df; font-size: 13px; }
  label > span { display: block; margin-bottom: 7px; }
  select, input[readonly] { width: 100%; box-sizing: border-box; background: #151a25; color: #f6f0df; border: 1px solid #3d4657; border-radius: 6px; padding: 9px; }
  input[type='range'] { width: 100%; accent-color: #dcb86b; }
  .settings-inline { display: flex; gap: 8px; }
  .settings-inline input { min-width: 0; }
  .settings-inline button { flex: 0 0 auto; }
  .settings-check { display: flex; gap: 9px; align-items: center; }
  .settings-check input { accent-color: #dcb86b; }
  .settings-check span { margin: 0; }
  .settings-saving { color: #dcb86b; font-size: 12px; }
  @media (max-width: 700px) { .launcher-page { inset: 62px 12px 12px; padding: 18px; } .launcher-page__head { flex-direction: column; } }
</style>
