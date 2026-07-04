using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Net.Http;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using System.Xml.Linq;
using System.Windows;
using System.ComponentModel;
using System.Windows.Interop;
using Microsoft.Web.WebView2.Core;
using Microsoft.Win32;

namespace WuwaIDLauncher;

public partial class MainWindow : Window
{
    internal static readonly string AppDataFolder = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "WuwaIDLauncher");
    internal static readonly string CacheFolder = Path.Combine(AppDataFolder, "Cache");
    internal static readonly string SettingsPath = Path.Combine(AppDataFolder, "settings.json");
    const string AssetsUrl = "https://raw.githubusercontent.com/TitoTFP/WuwaID/refs/heads/main/Web/assets.json";
    internal const string ModFolderName = Helpers.ModFolderName;
    internal const string LegacyModFolderName = Helpers.LegacyModFolderName;
    internal const string PakFileName = Helpers.PakFileName;
    internal const string LegacyPakFileName = Helpers.LegacyPakFileName;
    internal const string PakFolderRelativePath = Helpers.PakFolderRelativePath;
    internal const string SigFileName = Helpers.SigFileName;
    internal const string SigBackupFileName = Helpers.SigBackupFileName;
    const string GameExeName = "Client-Win64-Shipping.exe";
    const string GameProcessName = "Client-Win64-Shipping";
    const string WuwaIDLatestDownloadBaseUrl = "https://github.com/TitoTFP/WuwaID/releases/latest/download/";
    static readonly TimeSpan SigRestoreDelay = Helpers.SigRestoreDelay;

    volatile bool _pageReady;
    volatile bool _launchInProgress;
    volatile bool _signatureRestorePending;
    volatile bool _gameProcessRunning;
    volatile bool _updateInProgress;
    string? _launchGamePath;
    string? _pendingBgm, _pendingVideo, _pendingUpdateDate;
    SplashWindow? _splash;

    public MainWindow()
    {
        InitializeComponent();
        Directory.CreateDirectory(CacheFolder);
        AppLogger.Info("Main window initialized");
        ActivePlayerService.Start();
        Loaded += OnLoaded;
        Closing += OnClosing;
    }

    void OnClosing(object? sender, CancelEventArgs e)
    {
        if (_signatureRestorePending)
        {
            if (_gameProcessRunning)
            {
                AppLogger.Info("Close canceled while game is running and signature restore is pending");
                e.Cancel = true;
                WindowState = WindowState.Minimized;
                return;
            }

            if (!string.IsNullOrWhiteSpace(_launchGamePath))
            {
                try
                {
                    AppLogger.Info("Restoring signature during close");
                    Helpers.RestoreSigBackup(_launchGamePath);
                }
                catch (Exception ex)
                {
                    AppLogger.Exception(ex, "Failed to restore signature during close");
                }
                _signatureRestorePending = false;
            }
        }

        try
        {
            webView.CoreWebView2?.Profile.ClearBrowsingDataAsync();
            webView.Dispose();
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Failed to dispose WebView2");
        }

        try
        {
            var wv2Dir = Path.Combine(AppDataFolder, "WebView2");
            if (Directory.Exists(wv2Dir))
                Directory.Delete(wv2Dir, true);
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Failed to delete WebView2 user data");
        }
        AppLogger.Info("Main window closing");
        Environment.Exit(0);
    }

    internal void RequestCloseWindow()
    {
        if (_signatureRestorePending && _gameProcessRunning)
        {
            AppLogger.Info("Close requested while game is running; minimizing launcher");
            WindowState = WindowState.Minimized;
            return;
        }

        Application.Current.Shutdown();
    }

    async void OnLoaded(object sender, RoutedEventArgs e)
    {
        AppLogger.Info("Main window loaded");
        _splash = new SplashWindow();
        _splash.Show();

        try
        {
            
            var opts = new CoreWebView2EnvironmentOptions(
                "--disable-background-networking " +
                "--disable-features=Translate,AutofillServerCommunication,OptimizationHints,msSmartScreen " +
                "--disable-extensions " +
                "--renderer-process-limit=1");

            var env = await CoreWebView2Environment.CreateAsync(
                browserExecutableFolder: null,
                userDataFolder: Path.Combine(AppDataFolder, "WebView2"),
                options: opts);
            await webView.EnsureCoreWebView2Async(env);
            App.WebView2BrowserPid = webView.CoreWebView2.BrowserProcessId;
            AppLogger.Info("WebView2 initialized with browser process id " + App.WebView2BrowserPid);

            webView.CoreWebView2.Settings.IsStatusBarEnabled = false;
            webView.CoreWebView2.Settings.AreDefaultContextMenusEnabled = false;
#if DEBUG
            webView.CoreWebView2.Settings.AreDevToolsEnabled = true;
#else
            webView.CoreWebView2.Settings.AreDevToolsEnabled = false;
#endif
            webView.CoreWebView2.Settings.IsGeneralAutofillEnabled = false;
            webView.CoreWebView2.Settings.AreBrowserAcceleratorKeysEnabled = false;

            webView.CoreWebView2.AddHostObjectToScript("launcher", new LauncherBridge(this));
            webView.CoreWebView2.WebMessageReceived += OnWebMessage;

            webView.CoreWebView2.AddWebResourceRequestedFilter("https://app.local/*", CoreWebView2WebResourceContext.All);
            webView.CoreWebView2.WebResourceRequested += OnWebResourceRequested;

            webView.CoreWebView2.SetVirtualHostNameToFolderMapping(
                "cache.local", CacheFolder, CoreWebView2HostResourceAccessKind.Allow);

            webView.CoreWebView2.DOMContentLoaded += OnDOMContentLoaded;
            webView.CoreWebView2.NavigationStarting += OnNavigationStarting;
            webView.CoreWebView2.NewWindowRequested += OnNewWindowRequested;
            webView.CoreWebView2.Navigate("https://app.local/index.html");

#if DEBUG
            webView.CoreWebView2.OpenDevToolsWindow();
#endif
            
            _ = Task.Run(CheckAndDownloadMedia);
            _ = Task.Run(CheckLauncherVersion);
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "WebView2 initialization failed");
            MessageBox.Show("Gagal menginisialisasi WebView2: " + ex.Message);
            _splash?.FadeOutAndClose();
            _splash = null;
            Application.Current.Shutdown(1);
        }
    }

    void OnNewWindowRequested(object? sender, CoreWebView2NewWindowRequestedEventArgs e)
    {
        e.Handled = true;
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = e.Uri,
                UseShellExecute = true
            });
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Failed to open link in system browser: " + e.Uri);
        }
    }

    void OnDOMContentLoaded(object? sender, CoreWebView2DOMContentLoadedEventArgs e)
    {
        _pageReady = true;
        AppLogger.Info("Web UI DOM loaded");
        Dispatcher.Invoke(() =>
        {
            Opacity = 1;
            Activate();
            Focus();
            _splash?.FadeOutAndClose();
            _splash = null;
        });

        RunScript(@"
            (function(){
                document.addEventListener('selectstart', e => e.preventDefault());
                document.addEventListener('dragstart', e => e.preventDefault());
                document.addEventListener('keydown', function(e){
                    if(e.key==='F12') { e.preventDefault(); return; }
                    if(e.ctrlKey && e.shiftKey && 'IJC'.includes(e.key.toUpperCase())) { e.preventDefault(); return; }
                    if(e.ctrlKey && 'USus'.includes(e.key)) { e.preventDefault(); return; }
                });
                ['log','warn','error','info','debug','table','dir','trace'].forEach(function(m){
                    console[m] = function(){};
                });
            })();
        ");

        DetectGamePath();
        RestoreStaleSignatureFromSettings();
        FlushPendingMedia();
    }

    void OnNavigationStarting(object? sender, CoreWebView2NavigationStartingEventArgs e)
    {
        var uri = new Uri(e.Uri);
        
        if (uri.Host != "app.local" && uri.Host != "cache.local")
        {
            AppLogger.Warn("Blocked navigation to " + e.Uri);
            e.Cancel = true;
        }
    }

    [DllImport("user32.dll")]
    static extern nint SendMessage(nint hWnd, int Msg, nint wParam, nint lParam);
    [DllImport("user32.dll")]
    static extern bool ReleaseCapture();
    const int WM_NCLBUTTONDOWN = 0x00A1;
    const int HT_CAPTION = 0x0002;

    void OnWebMessage(object? sender, CoreWebView2WebMessageReceivedEventArgs e)
    {
        if (e.TryGetWebMessageAsString() == "drag")
        {
            Dispatcher.Invoke(() =>
            {
                try
                {
                    var hwnd = new WindowInteropHelper(this).Handle;
                    ReleaseCapture();
                    SendMessage(hwnd, WM_NCLBUTTONDOWN, HT_CAPTION, 0);
                }
                catch (Exception ex)
                {
                    AppLogger.Exception(ex, "Failed to process window drag message");
                }
            });
        }
    }


    static readonly Assembly Asm = Assembly.GetExecutingAssembly();
    const string ResPrefix = "WuwaIDLauncher.Resources.Web.";
    static readonly byte[] XorKey = "WuwaID@2026!xK9#mQ"u8.ToArray();

    void OnWebResourceRequested(object? sender, CoreWebView2WebResourceRequestedEventArgs e)
    {
        var uri = new Uri(e.Request.Uri);
        var path = uri.AbsolutePath.TrimStart('/');
        var resName = ResPrefix + path.Replace('/', '.');

        var encStream = Asm.GetManifestResourceStream(resName);
        if (encStream == null)
        {
            AppLogger.Warn("Web resource not found: " + path);
            e.Response = webView.CoreWebView2.Environment.CreateWebResourceResponse(
                null, 404, "Not Found", "");
            return;
        }

        var enc = new byte[encStream.Length];
        encStream.ReadExactly(enc);
        encStream.Dispose();
        for (int i = 0; i < enc.Length; i++)
            enc[i] ^= XorKey[i % XorKey.Length];

        var mime = Helpers.GetMimeType(path);
        var ms = new MemoryStream(enc);
        e.Response = webView.CoreWebView2.Environment.CreateWebResourceResponse(
            ms, 200, "OK",
            $"Content-Type: {mime}\r\n" +
            "Cache-Control: no-store\r\n" +
            "Content-Security-Policy: default-src 'self' https://app.local https://cache.local; " +
            "script-src 'self' https://app.local 'unsafe-inline'; " +
            "style-src 'self' https://app.local 'unsafe-inline'; " +
            "img-src 'self' https://app.local https://cache.local data:; " +
            "media-src 'self' https://cache.local blob:; " +
            
            "connect-src 'self' https://app.local");
    }

    static string JsStr(string s) => JsonSerializer.Serialize(s);

    internal void RunScript(string js)
    {
        Dispatcher.InvokeAsync(async () =>
        {
            try { await webView.CoreWebView2.ExecuteScriptAsync(js); }
            catch (Exception ex)
            {
                AppLogger.Exception(ex, "Failed to execute WebView script");
            }
        });
    }


    void DetectGamePath()
    {
        string[] paths =
        [
            @"C:\Wuthering Waves", @"D:\Wuthering Waves", @"E:\Wuthering Waves",
            @"C:\Program Files\Wuthering Waves", @"D:\Program Files\Wuthering Waves"
        ];
        foreach (var p in paths)
        {
            var full = Path.Combine(p, "Wuthering Waves Game");
            if (Directory.Exists(full))
            {
                AppLogger.SetGamePath(full);
                AppLogger.Info("Detected game path: " + full);
                RunScript($"window.onGamePathDetected({JsStr(full)})");
                return;
            }
        }
    }

    void RestoreStaleSignatureFromSettings()
    {
        try
        {
            if (!File.Exists(SettingsPath) || Helpers.IsGameRunning())
                return;

            var json = File.ReadAllText(SettingsPath);
            using var doc = JsonDocument.Parse(json);
            if (!doc.RootElement.TryGetProperty("gamePath", out var pathProp))
                return;

            var gamePath = pathProp.GetString();
            if (!string.IsNullOrWhiteSpace(gamePath))
            {
                AppLogger.SetGamePath(gamePath);
                AppLogger.Info("Restoring stale signature from settings");
                Helpers.RestoreSigBackup(gamePath);
            }
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Failed to restore stale signature from settings");
        }
    }


    static string NormalizeInstallMethod(string? installMethod) =>
        string.Equals(installMethod, "method2", StringComparison.OrdinalIgnoreCase)
            ? "method2"
            : "method1";

    static bool UsesManualLoaderMethod(string? installMethod) =>
        NormalizeInstallMethod(installMethod) == "method2";

    internal async Task RunInstallation(string gamePath, string vhMode, bool backup, string installMethod = "method1")
    {
        AppLogger.SetGamePath(gamePath);
        var method = NormalizeInstallMethod(installMethod);
        AppLogger.Info("Installation started; mode=" + vhMode + "; backup=" + backup + "; installMethod=" + method);
        try
        {
            var baseDir = Helpers.GameBinaryFolderPath(gamePath);
            var pakDir = UsesManualLoaderMethod(method)
                ? Helpers.Method2PakFolderPath(gamePath)
                : Path.Combine(gamePath, PakFolderRelativePath);
            
            if (!Directory.Exists(baseDir))
            {
                AppLogger.Warn("Game directory missing: " + baseDir);
                throw new Exception("Direktori game tidak ditemukan. Silakan periksa kembali pathnya.");
            }

            try
            {
                var writeCheckDirs = UsesManualLoaderMethod(method)
                    ? new[] { baseDir, pakDir }
                    : new[] { pakDir };

                foreach (var dir in writeCheckDirs.Distinct(StringComparer.OrdinalIgnoreCase))
                {
                    Directory.CreateDirectory(dir);
                    var testFile = Path.Combine(dir, "vh_write_test.tmp");
                    File.WriteAllText(testFile, "test");
                    File.Delete(testFile);
                }
            }
            catch (UnauthorizedAccessException)
            {
                AppLogger.Warn("Installation needs admin permission for pak directory: " + pakDir);
                RunScript("if(window.onAdminRequired) window.onAdminRequired(); else window.onInstallError('Folder game dikunci oleh Windows. Jalankan Launcher sebagai Admin.');");
                return;
            }
            catch (Exception ex)
            {
                AppLogger.Exception(ex, "Pak directory write test failed");
                throw new Exception("Tidak dapat menulis file ke direktori game: " + ex.Message);
            }

            if (UsesManualLoaderMethod(method))
            {
                Helpers.RestoreSigBackup(gamePath);
                var oldVersionLoader = Path.Combine(baseDir, Helpers.VersionLoaderFileName);
                var legacyModDir = Path.Combine(baseDir, Helpers.LegacyModFolderName);

                Helpers.DeleteLegacyPakFile(gamePath);
                if (File.Exists(oldVersionLoader))
                    File.Delete(oldVersionLoader);
                if (Directory.Exists(legacyModDir))
                    Directory.Delete(legacyModDir, true);
            }
            else
            {
                Helpers.DeleteManualLoaderFiles(gamePath, preservePak: true);
                var oldVersionLoader = Path.Combine(baseDir, Helpers.VersionLoaderFileName);
                var legacyModDir = Path.Combine(baseDir, Helpers.LegacyModFolderName);

                if (File.Exists(oldVersionLoader))
                    File.Delete(oldVersionLoader);
                if (Directory.Exists(legacyModDir))
                    Directory.Delete(legacyModDir, true);
                AppLogger.Info("Legacy loader cleanup completed");
            }

            var expectedAssets = UsesManualLoaderMethod(method)
                ? new[] { Helpers.ManualPakFileName, Helpers.WinHttpLoaderFileName }
                : new[] { PakFileName };
            var toDownload = new List<(string Name, string Url, long Size, string Fingerprint, string DestPath)>();

            using var http = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("WuwaIDLauncher/1.0");
            var tagName = "latest";

            foreach (var name in expectedAssets)
            {
                var url = WuwaIDLatestDownloadBaseUrl + Uri.EscapeDataString(name);
                var metadata = await GetReleaseAssetMetadata(http, url);
                var destPath = name.Equals(Helpers.WinHttpLoaderFileName, StringComparison.OrdinalIgnoreCase)
                    ? Helpers.Method2LoaderPath(gamePath)
                    : Path.Combine(pakDir, name);
                toDownload.Add((name, url, metadata.Size, metadata.Fingerprint, destPath));
            }

            var versionCachePath = Path.Combine(AppDataFolder, "versions.json");
            var localCache = new Dictionary<string, string>();
            if (File.Exists(versionCachePath))
            {
                try
                {
                    var cacheJson = File.ReadAllText(versionCachePath);
                    localCache = JsonSerializer.Deserialize<Dictionary<string, string>>(cacheJson) ?? new();
                }
                catch (Exception ex)
                {
                    AppLogger.Exception(ex, "Failed to read version cache");
                }
            }

            if (!UsesManualLoaderMethod(method))
            {
                Helpers.DeleteLegacyPakFile(gamePath);
                localCache.Remove(LegacyPakFileName);
            }

            bool allFilesUpToDate = true;
            foreach (var (name, _, _, fingerprint, destPath) in toDownload)
            {

                if (!File.Exists(destPath))
                {
                    allFilesUpToDate = false;
                    break;
                }
                
                if (!string.IsNullOrEmpty(fingerprint))
                {
                    if (!localCache.TryGetValue(name, out var cachedFingerprint) || cachedFingerprint != fingerprint)
                    {
                        allFilesUpToDate = false;
                        break;
                    }

                    localCache[name] = fingerprint;
                }
                else if (!localCache.TryGetValue("_installMethod", out var cachedMethod) || cachedMethod != method)
                {
                    allFilesUpToDate = false;
                    break;
                }
            }

            if (allFilesUpToDate)
            {
                CleanupInactiveMethodFiles(gamePath, method);
                AppLogger.Info("Installation skipped; local files already current");
                if (!string.IsNullOrEmpty(tagName))
                {
                    localCache["_vhVersion"] = tagName;
                    localCache["_installMethod"] = method;
                    File.WriteAllText(versionCachePath, JsonSerializer.Serialize(localCache));
                }
                RunScript($"window.onProgressUpdate(100, {JsStr("Anda sudah menggunakan versi terbaru!")}, '', '')");
                await Task.Delay(1500);
                RunScript("window.onInstallComplete()");
                return;
            }

            
            var needsUpdateSet = new HashSet<string>();
            long totalBytes = 0;
            foreach (var (name, _, size, fingerprint, destPath) in toDownload)
            {

                bool needsUpdate = !File.Exists(destPath) ||
                                   (!string.IsNullOrEmpty(fingerprint)
                                       ? !localCache.TryGetValue(name, out var cachedFingerprint) || cachedFingerprint != fingerprint
                                       : !localCache.TryGetValue("_installMethod", out var cachedMethod) || cachedMethod != method);
                if (needsUpdate)
                {
                    needsUpdateSet.Add(name);
                    totalBytes += size;
                }
            }

            long totalDownloaded = 0;
            var sw = Stopwatch.StartNew();
            long lastDownloaded = 0;

            foreach (var (name, url, size, fingerprint, destPath) in toDownload)
            {

                if (!needsUpdateSet.Contains(name))
                    continue;

                if (IsPakAsset(name))
                {
                    var alternatePakPath = Helpers.AlternatePakPathForMethod(gamePath, method);
                    if (IsSha256(fingerprint) && TryCopyReusablePak(alternatePakPath, destPath, fingerprint))
                    {
                        AppLogger.Info("Reused existing pak for installer asset: " + name);
                        needsUpdateSet.Remove(name);
                        localCache[name] = fingerprint;
                        RunScript($"window.onProgressUpdate(100, " +
                                  $"{JsStr("Menggunakan file patch yang sudah ada...")}, '', '')");
                        continue;
                    }
                }
                
                var tmpPath = destPath + ".tmp";
                Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);

                using var resp = await http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead);
                resp.EnsureSuccessStatusCode();
                AppLogger.Info("Downloading installer asset: " + name);

                await using var netStream = await resp.Content.ReadAsStreamAsync();
                await using var fileStream = new FileStream(tmpPath, FileMode.Create, FileAccess.Write, FileShare.None, 65536, useAsync: true);

                var buffer = new byte[65536];
                int bytesRead;

                while ((bytesRead = await netStream.ReadAsync(buffer)) > 0)
                {
                    await fileStream.WriteAsync(buffer.AsMemory(0, bytesRead));
                    totalDownloaded += bytesRead;

                    if (sw.ElapsedMilliseconds >= 350)
                    {
                        var pct = totalBytes > 0 ? (int)((totalDownloaded * 100) / totalBytes) : 0;
                        var speed = (totalDownloaded - lastDownloaded) / sw.Elapsed.TotalSeconds / 1_048_576.0;
                        var progressText = $"{totalDownloaded / 1_048_576.0:F1} / {totalBytes / 1_048_576.0:F1} MB";
                        
                        RunScript($"window.onProgressUpdate({pct}, " +
                                  $"{JsStr($"Mengunduh: {name}")}, " +
                                  $"{JsStr($"{speed:F1} MB/s")}, {JsStr(progressText)})");

                        lastDownloaded = totalDownloaded;
                        sw.Restart();
                    }
                }
                
                fileStream.Close(); File.Move(tmpPath, destPath, true);
                if (IsSha256(fingerprint) && !Helpers.VerifySha256(destPath, fingerprint))
                {
                    AppLogger.Error("Downloaded asset hash mismatch: " + name);
                    try { File.Delete(destPath); } catch (Exception ex) { AppLogger.Exception(ex, "Failed to delete hash-mismatched asset"); }
                    throw new Exception($"Hash file {name} tidak cocok. Unduhan dibatalkan.");
                }

                if (!string.IsNullOrEmpty(fingerprint))
                    localCache[name] = fingerprint;
            }

            if (!string.IsNullOrEmpty(tagName))
                localCache["_vhVersion"] = tagName;
            localCache["_installMethod"] = method;
            File.WriteAllText(versionCachePath, JsonSerializer.Serialize(localCache));

            CleanupInactiveMethodFiles(gamePath, method);

            AppLogger.Info("Installation completed");
            RunScript($"window.onProgressUpdate(100, {JsStr("Instalasi selesai!")}, '', '')");
            await Task.Delay(1000);
            RunScript("window.onInstallComplete()");
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Installation failed");
            RunScript($"window.onInstallError({JsStr(ex.Message)})");
        }
    }

    static async Task<(long Size, string Fingerprint)> GetReleaseAssetMetadata(HttpClient http, string url)
    {
        using var req = new HttpRequestMessage(HttpMethod.Head, url);
        using var resp = await http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead);
        resp.EnsureSuccessStatusCode();

        var size = resp.Content.Headers.ContentLength ?? 0;
        var etag = resp.Headers.ETag?.Tag?.Trim('"') ?? "";
        var lastModified = resp.Content.Headers.LastModified?.UtcDateTime.ToString("O") ?? "";
        var fingerprint = string.Join("|", new[] { etag, lastModified, size.ToString() });
        return (size, fingerprint);
    }

    static async Task<string> GetLatestReleaseTag(HttpClient http, string latestReleaseUrl)
    {
        using var req = new HttpRequestMessage(HttpMethod.Head, latestReleaseUrl);
        using var resp = await http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead);
        resp.EnsureSuccessStatusCode();

        var uri = resp.RequestMessage?.RequestUri;
        var tag = uri?.Segments.LastOrDefault()?.Trim('/') ?? "";
        return tag == "latest" ? "" : tag;
    }

    static async Task<(string Tag, string Date, string Body, string Name)> FetchLatestReleaseNotesFromAtom(HttpClient http, string atomUrl)
    {
        var xml = await http.GetStringAsync(atomUrl);
        var doc = XDocument.Parse(xml);
        XNamespace atom = "http://www.w3.org/2005/Atom";
        var entry = doc.Root?.Element(atom + "entry")
            ?? throw new Exception("Release notes tidak ditemukan.");

        var title = entry.Element(atom + "title")?.Value ?? "";
        var body = entry.Element(atom + "content")?.Value ?? "";
        var date = entry.Element(atom + "updated")?.Value ?? "";
        var href = entry.Elements(atom + "link")
            .FirstOrDefault(e => string.Equals((string?)e.Attribute("rel"), "alternate", StringComparison.OrdinalIgnoreCase))
            ?.Attribute("href")?.Value ?? "";
        var tag = href.Split('/', StringSplitOptions.RemoveEmptyEntries).LastOrDefault() ?? "";

        return (tag, date, body, title);
    }

    static bool IsSha256(string value) =>
        value.Length == 64 && value.All(Uri.IsHexDigit);

    static bool IsPakAsset(string name) =>
        name.Equals(Helpers.PakFileName, StringComparison.OrdinalIgnoreCase) ||
        name.Equals(Helpers.ManualPakFileName, StringComparison.OrdinalIgnoreCase) ||
        name.Equals(Helpers.LegacyPakFileName, StringComparison.OrdinalIgnoreCase);

    static bool TryCopyReusablePak(string sourcePath, string destPath, string hash)
    {
        if (!File.Exists(sourcePath))
            return false;

        if (!string.IsNullOrEmpty(hash) && !Helpers.VerifySha256(sourcePath, hash))
            return false;

        Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);
        File.Copy(sourcePath, destPath, overwrite: true);
        return string.IsNullOrEmpty(hash) || Helpers.VerifySha256(destPath, hash);
    }

    static void CleanupInactiveMethodFiles(string gamePath, string method)
    {
        if (UsesManualLoaderMethod(method))
        {
            var method1PakPath = Helpers.Method1PakPath(gamePath);
            if (File.Exists(method1PakPath))
                File.Delete(method1PakPath);
            Helpers.RestoreSigBackup(gamePath);
        }
        else
        {
            Helpers.DeleteManualLoaderFiles(gamePath);
            Helpers.DeleteLegacyPakFile(gamePath);
        }
    }


    static void PrepareSigBypass(string gamePath)
    {
        Helpers.RestoreSigBackup(gamePath);
        AppLogger.SetGamePath(gamePath);

        var sigPath = Helpers.SigPath(gamePath);
        var backupPath = Helpers.SigBackupPath(gamePath);

        if (File.Exists(sigPath))
        {
            AppLogger.Info("Preparing signature bypass");
            File.Move(sigPath, backupPath, true);
        }
    }

    async Task MonitorLaunchStateAsync(string gamePath, Process? process, bool restoreSignature = true)
    {
        AppLogger.SetGamePath(gamePath);
        try
        {
            AppLogger.Info("Launch monitor started");
            if (!restoreSignature)
            {
                await WaitForGameExitAsync(process);
                return;
            }

            var gameExitTask = WaitForGameExitAsync(process);
            var restoreDelayTask = Task.Delay(SigRestoreDelay);
            var first = await Task.WhenAny(gameExitTask, restoreDelayTask);

            if (first == gameExitTask)
            {
                _gameProcessRunning = false;
                AppLogger.Info("Game exited before signature restore delay");
                RunScript("window.onGameLaunchWaitingRestore()");
            }

            Helpers.RestoreSigBackup(gamePath);
            AppLogger.Info("Signature restore completed");
            _signatureRestorePending = false;

            if (first != gameExitTask)
                await gameExitTask;
        }
        catch (UnauthorizedAccessException)
        {
            AppLogger.Warn("Signature restore needs admin permission");
            RunScript($"window.onInstallError({JsStr("Tidak memiliki izin memulihkan signature game. Jalankan sebagai Admin.")})");
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Signature restore failed");
            RunScript($"window.onInstallError({JsStr("Gagal memulihkan signature game: " + ex.Message)})");
        }
        finally
        {
            _gameProcessRunning = false;
            _signatureRestorePending = false;
            _launchInProgress = false;
            _launchGamePath = null;
            AppLogger.Info("Launch monitor finished");
            RunScript("window.onGameLaunchFinished()");
        }
    }

    static async Task WaitForGameExitAsync(Process? process)
    {
        try
        {
            if (process != null)
                await process.WaitForExitAsync();
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Failed to wait for direct game process exit");
        }

        await Task.Delay(3000);

        while (Helpers.IsGameRunning())
            await Task.Delay(1000);
    }

    internal void LaunchGame(string gamePath, bool dx11, string installMethod = "method1")
    {
        AppLogger.SetGamePath(gamePath);
        var method = NormalizeInstallMethod(installMethod);
        AppLogger.Info("Game launch requested; dx11=" + dx11 + "; installMethod=" + method);
        _ = ActivePlayerService.SendLaunchHeartbeatAsync(method);
        try
        {
            if (_launchInProgress)
            {
                AppLogger.Info("Duplicate game launch request ignored");
                RunScript("window.onGameLaunchStarted()");
                return;
            }

            var full = Path.Combine(gamePath, @"Client\Binaries\Win64", GameExeName);
            if (File.Exists(full))
            {
                if (!UsesManualLoaderMethod(method))
                {
                    Helpers.RestoreSigBackup(gamePath);

                    if (!File.Exists(Helpers.SigPath(gamePath)) && !File.Exists(Helpers.SigBackupPath(gamePath)))
                    {
                        AppLogger.Warn("Game signature file missing before launch");
                        RunScript($"window.onInstallError({JsStr("Signature file tidak terdeteksi, jalankan Wuthering Waves dulu tanpa mod atau launcher ini.")})");
                        return;
                    }

                    PrepareSigBypass(gamePath);
                }

                _launchInProgress = true;
                _signatureRestorePending = !UsesManualLoaderMethod(method);
                _gameProcessRunning = true;
                _launchGamePath = gamePath;

                var process = Process.Start(new ProcessStartInfo
                {
                    FileName = full,
                    Arguments = dx11 ? "-dx11" : "",
                    UseShellExecute = true,
                    Verb = "runas"
                });
                AppLogger.Info("Game process start requested");
                RunScript("window.onGameLaunchStarted()");
                Dispatcher.Invoke(() => WindowState = WindowState.Minimized);
                _ = MonitorLaunchStateAsync(gamePath, process, !UsesManualLoaderMethod(method));
            }
            else
            {
                AppLogger.Warn("Game executable missing: " + full);
                RunScript($"window.onInstallError({JsStr("File game tidak ditemukan: " + GameExeName)})");
            }
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Game launch failed");
            _launchInProgress = false;
            _signatureRestorePending = false;
            _gameProcessRunning = false;
            _launchGamePath = null;
            if (!UsesManualLoaderMethod(method))
                Helpers.RestoreSigBackup(gamePath);
            RunScript("window.onGameLaunchFinished()");
            RunScript($"window.onInstallError({JsStr("Gagal menjalankan game: " + ex.Message)})");
        }
    }


    const string LauncherLatestReleaseUrl = "https://github.com/TitoTFP/WuwaIDLauncher/releases/latest";
    const string LauncherReleasesPageUrl = "https://github.com/TitoTFP/WuwaIDLauncher/releases";

    internal async Task CheckLauncherVersion()
    {
        try
        {
            AppLogger.Info("Checking launcher version");
            using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("WuwaIDLauncher/1.0");
            var tag = (await GetLatestReleaseTag(http, LauncherLatestReleaseUrl)).TrimStart('v', 'V');
            if (string.IsNullOrEmpty(tag)) return;

            var current = Assembly.GetExecutingAssembly().GetName().Version ?? new Version(1, 0, 0);
            if (!Version.TryParse(tag, out var latest)) return;

            if (latest > current)
            {
                AppLogger.Info($"Launcher update available: v{tag}");
                while (!_pageReady) await Task.Delay(100);
                var downloadUrl = $"https://github.com/TitoTFP/WuwaIDLauncher/releases/download/v{tag}/WuwaIDLauncher-v{tag}.zip";
                RunScript($"window.onLauncherUpdateAvailable({JsStr('v' + tag)}, {JsStr(downloadUrl)})");
            }
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Launcher update check failed");
        }
    }


    const string VHLatestReleaseUrl = "https://github.com/TitoTFP/WuwaID/releases/latest";
    const string VHReleasesAtomUrl = "https://github.com/TitoTFP/WuwaID/releases.atom";

    internal async Task FetchVHReleaseNotes()
    {
        try
        {
            AppLogger.Info("Fetching WuwaID release notes");
            using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("WuwaIDLauncher/1.0");
            var (tag, date, body, name) = await FetchLatestReleaseNotesFromAtom(http, VHReleasesAtomUrl);

            while (!_pageReady) await Task.Delay(100);
            RunScript($"window.onVHReleaseNotes({JsStr(tag)}, {JsStr(date)}, {JsStr(body)}, {JsStr(name)})");
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Failed to fetch WuwaID release notes");
        }
    }


    internal async Task PerformLauncherUpdate(string version, string zipUrl)
    {
        if (_updateInProgress)
        {
            AppLogger.Warn("Duplicate launcher update request ignored");
            return;
        }
        _updateInProgress = true;
        try
        {
            AppLogger.Info("Launcher update started: " + version);
            var updateDir = Path.Combine(Path.GetTempPath(), "WuwaIDLauncher_update");
            
            if (Directory.Exists(updateDir))
            {
                for (int i = 0; i < 5; i++)
                {
                    try
                    {
                        Directory.Delete(updateDir, true);
                        break;
                    }
                    catch (IOException ex) when (i < 4)
                    {
                        AppLogger.Warn($"Failed to delete update directory (attempt {i + 1}): {ex.Message}. Retrying...");
                        await Task.Delay(500);
                    }
                }
            }
            
            Directory.CreateDirectory(updateDir);
            var zipPath = Path.Combine(updateDir, "update.zip");

            RunScript("window.onLauncherUpdateProgress(0, 'Mengunduh...')");
            using var http = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("WuwaIDLauncher/1.0");

            using var resp = await http.GetAsync(zipUrl, HttpCompletionOption.ResponseHeadersRead);
            resp.EnsureSuccessStatusCode();
            long total = resp.Content.Headers.ContentLength ?? -1;
            AppLogger.Info("Launcher update download started; bytes=" + total);

            await using (var net = await resp.Content.ReadAsStreamAsync())
            {
                await using (var fs = new FileStream(zipPath, FileMode.Create, FileAccess.Write, FileShare.None, 65536, true))
                {
                    var buf = new byte[65536];
                    long got = 0;
                    var sw = Stopwatch.StartNew();
                    int read;
                    while ((read = await net.ReadAsync(buf)) > 0)
                    {
                        await fs.WriteAsync(buf.AsMemory(0, read));
                        got += read;
                        if (sw.ElapsedMilliseconds >= 300)
                        {
                            int pct = total > 0 ? (int)(got * 100 / total) : 0;
                            var sizeText = total > 0
                                ? $"{got / 1_048_576.0:F1} / {total / 1_048_576.0:F1} MB"
                                : $"{got / 1_048_576.0:F1} MB";
                            RunScript($"window.onLauncherUpdateProgress({pct}, {JsStr(sizeText)})");
                            sw.Restart();
                        }
                    }
                }
            }

            RunScript("window.onLauncherUpdateProgress(95, 'Mengekstrak...')");
            var extractDir = Path.Combine(updateDir, "extracted");
            
            for (int i = 0; i < 5; i++)
            {
                try
                {
                    ZipFile.ExtractToDirectory(zipPath, extractDir);
                    break;
                }
                catch (IOException ex) when (i < 4)
                {
                    AppLogger.Warn($"Failed to extract update.zip (attempt {i + 1}): {ex.Message}. Retrying...");
                    await Task.Delay(500);
                }
            }

            var newExe = Directory.GetFiles(extractDir, "WuwaIDLauncher.exe", SearchOption.AllDirectories)
                                   .FirstOrDefault()
                         ?? throw new Exception("WuwaIDLauncher.exe tidak ditemukan dalam file zip.");
            AppLogger.Info("Launcher update executable extracted");

            var currentExe = Process.GetCurrentProcess().MainModule?.FileName
                             ?? throw new Exception("Direktori exe saat ini tidak diketahui.");
            var currentPid = Environment.ProcessId;

            var scriptPath = Path.Combine(Path.GetTempPath(), "WuwaIDLauncher_updater.ps1");
            var newExeEscaped = newExe.Replace("'", "''");
            var currentExeEscaped = currentExe.Replace("'", "''");
            var scriptContent =
                $"$launcherPid = {currentPid}\n" +
                $"$newExe     = '{newExeEscaped}'\n" +
                $"$targetExe  = '{currentExeEscaped}'\n" +
                "# Wait for launcher process to fully exit\n" +
                "while ($null -ne (Get-Process -Id $launcherPid -ErrorAction SilentlyContinue)) {\n" +
                "    Start-Sleep -Milliseconds 300\n" +
                "}\n" +
                "Start-Sleep -Milliseconds 500\n" +
                "try {\n" +
                "    Copy-Item -Path $newExe -Destination $targetExe -Force\n" +
                "    Start-Process -FilePath $targetExe\n" +
                "} catch {\n" +
                "    Add-Type -AssemblyName PresentationFramework\n" +
                "    [System.Windows.MessageBox]::Show(\"Pembaruan gagal: $_\", \"WuwaIDLauncher Updater\")\n" +
                "}\n" +
                "# Cleanup\n" +
                "Start-Sleep -Seconds 2\n" +
                $"Remove-Item -Recurse -Force '{updateDir.Replace("'", "''")}' -ErrorAction SilentlyContinue\n" +
                $"Remove-Item -Force $MyInvocation.MyCommand.Path -ErrorAction SilentlyContinue\n";
            File.WriteAllText(scriptPath, scriptContent, System.Text.Encoding.UTF8);
            AppLogger.Info("Launcher updater script written to: " + scriptPath);

            RunScript("window.onLauncherUpdateProgress(100, 'Memulai ulang...')");

            // Show restart warning countdown before shutting down
            RunScript("window.onLauncherUpdateRestarting()");
            await Task.Delay(12000);

            Process.Start(new ProcessStartInfo
            {
                FileName = "powershell.exe",
                Arguments = $"-ExecutionPolicy Bypass -WindowStyle Hidden -NonInteractive -File \"{scriptPath}\"",
                UseShellExecute = true,
                WindowStyle = ProcessWindowStyle.Hidden
            });
            AppLogger.Info("Launcher updater handoff started");

            Dispatcher.Invoke(() => Application.Current.Shutdown());
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Launcher update failed");
            RunScript($"window.onLauncherUpdateError({JsStr(ex.Message)})");
        }
        finally
        {
            _updateInProgress = false;
        }
    }

    async Task CheckAndDownloadMedia()
    {
        AppLogger.Info("Media check started");
        SignalMediaReady();

        RunScript("window.onMediaStatus('checking', '')");
        var toDownload = new List<(string Name, string Url, string Hash)>();

        try
        {
            using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(20) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("WuwaIDLauncher/1.0");
            var json = await http.GetStringAsync(AssetsUrl);
            using var doc = JsonDocument.Parse(json);

            if (doc.RootElement.TryGetProperty("update_date", out var updateDateProp))
            {
                var updateDate = updateDateProp.GetString() ?? "";
                if (!string.IsNullOrEmpty(updateDate))
                {
                    if (_pageReady)
                        RunScript($"window.onUpdateDate({JsStr(updateDate)})");
                    else
                        _pendingUpdateDate = updateDate;
                }
            }

            foreach (var item in doc.RootElement.GetProperty("assets").EnumerateArray())
            {
                var name = item.GetProperty("name").GetString() ?? "";
                if (name is "bgm.mp3" or "bg-video.mp4")
                {
                    var url = item.GetProperty("url").GetString() ?? "";
                    var hash = item.GetProperty("sha256").GetString() ?? "";
                    var dest = Path.Combine(CacheFolder, name);
                    if (!File.Exists(dest) || !Helpers.VerifySha256(dest, hash))
                        toDownload.Add((name, url, hash));
                }
            }
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Media manifest check failed");
        }

        if (toDownload.Count > 0)
        {
            AppLogger.Info("Media download queue count: " + toDownload.Count);
            using var http = new HttpClient { Timeout = TimeSpan.FromMinutes(30) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("WuwaIDLauncher/1.0");
            foreach (var (name, url, _) in toDownload)
            {
                try
                {
                    await DownloadWithProgress(http, url, Path.Combine(CacheFolder, name), name);
                }
                catch (Exception ex)
                {
                    AppLogger.Exception(ex, "Media download failed: " + name);
                    RunScript($"window.onMediaStatus('error', " +
                              $"{JsStr("Gagal mengunduh " + name + ": " + ex.Message)})");
                }
            }
            SignalMediaReady();
        }

        AppLogger.Info("Media check finished");
        RunScript("window.onMediaStatus('ready', '')");
    }

    async Task DownloadWithProgress(HttpClient http, string url, string dest, string name)
    {
        AppLogger.Info("Downloading media asset: " + name);
        using var resp = await http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead);
        resp.EnsureSuccessStatusCode();
        long total = resp.Content.Headers.ContentLength ?? -1;
        var tmp = dest + ".tmp";

        await using (var net = await resp.Content.ReadAsStreamAsync())
        await using (var fs = new FileStream(tmp, FileMode.Create, FileAccess.Write,
                                             FileShare.None, 65536, useAsync: true))
        {
            var buf = new byte[65536];
            long got = 0, lastGot = 0;
            var sw = Stopwatch.StartNew();
            int read;
            while ((read = await net.ReadAsync(buf)) > 0)
            {
                await fs.WriteAsync(buf.AsMemory(0, read));
                got += read;
                if (sw.ElapsedMilliseconds >= 350)
                {
                    int pct = total > 0 ? (int)(got * 100 / total) : 0;
                    var spd = (got - lastGot) / sw.Elapsed.TotalSeconds / 1_048_576.0;
                    var size = total > 0
                        ? $"{got / 1_048_576.0:F1} / {total / 1_048_576.0:F1} MB"
                        : $"{got / 1_048_576.0:F1} MB";
                    RunScript($"window.onMediaProgress({pct}, " +
                              $"{JsStr("Mengunduh " + name + "...")}, " +
                              $"{JsStr($"{spd:F1} MB/s")}, {JsStr(size)})");
                    lastGot = got;
                    sw.Restart();
                }
            }
        }

        if (File.Exists(dest)) File.Delete(dest);
        File.Move(tmp, dest);
        AppLogger.Info("Media asset ready: " + name);
    }

    void SignalMediaReady()
    {
        var bgm = File.Exists(Path.Combine(CacheFolder, "bgm.mp3")) ? "https://cache.local/bgm.mp3" : "";
        var video = File.Exists(Path.Combine(CacheFolder, "bg-video.mp4")) ? "https://cache.local/bg-video.mp4" : "";

        if (_pageReady)
            RunScript($"window.onMediaReady({JsStr(bgm)}, {JsStr(video)})");
        else
            (_pendingBgm, _pendingVideo) = (bgm, video);
    }

    void FlushPendingMedia()
    {        if (_pendingUpdateDate != null)
        {
            RunScript($"window.onUpdateDate({JsStr(_pendingUpdateDate)})");
            _pendingUpdateDate = null;
        }        if (_pendingBgm != null || _pendingVideo != null)
        {
            RunScript($"window.onMediaReady({JsStr(_pendingBgm ?? "")}, {JsStr(_pendingVideo ?? "")})");
            _pendingBgm = _pendingVideo = null;
        }
    }
}


[ClassInterface(ClassInterfaceType.AutoDual)]
[ComVisible(true)]
public class LauncherBridge
{
    readonly MainWindow _w;
    internal LauncherBridge(MainWindow w) => _w = w;

    public void MinimizeWindow() =>
        _w.Dispatcher.Invoke(() => _w.WindowState = WindowState.Minimized);

    public void CloseWindow() =>
        _w.Dispatcher.Invoke(() => _w.RequestCloseWindow());

        public string BrowseGameFolder() =>
        _w.Dispatcher.Invoke(() =>
        {
            var dlg = new OpenFolderDialog
            {
                Title = "Pilih folder instalasi Wuthering Waves"
            };
            
            if (dlg.ShowDialog(_w) == true)
            {
                var path = dlg.FolderName;
                var exe = @"Client\Binaries\Win64\Client-Win64-Shipping.exe";
                
                string? Check(string p) => System.IO.File.Exists(Path.Combine(p, exe)) ? p : null;
                
                var valid = Check(path) ?? Check(Path.Combine(path, "Wuthering Waves Game"));
                if (valid == null)
                {
                    var parent = new DirectoryInfo(path).Parent;
                    while (parent != null && valid == null)
                    {
                        valid = Check(parent.FullName) ?? Check(Path.Combine(parent.FullName, "Wuthering Waves Game"));
                        parent = parent.Parent;
                    }
                }
                
                return valid ?? "?INVALID";
            }
            return "";
        });

    public void OpenUrl(string url)
    {
        if (Uri.TryCreate(url, UriKind.Absolute, out var uri) &&
            (uri.Scheme == "https" || uri.Scheme == "http"))
        {
            AppLogger.Info("Opening external URL: " + uri.Host);
            Process.Start(new ProcessStartInfo { FileName = url, UseShellExecute = true });
        }
    }

    public void SaveSettings(string json)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(MainWindow.SettingsPath)!);
            File.WriteAllText(MainWindow.SettingsPath, json);
            AppLogger.Info("Settings saved");
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Failed to save settings");
        }
    }

    public string LoadSettings()
    {
        try
        {
            return File.Exists(MainWindow.SettingsPath)
                ? File.ReadAllText(MainWindow.SettingsPath) : "";
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Failed to load settings");
            return "";
        }
    }

    public bool ShowConfirm(string message) =>
        _w.Dispatcher.Invoke(() =>
        {
            var dlg = new ConfirmDialog(message, _w);
            dlg.ShowDialog();
            return dlg.Confirmed;
        });

    public string GetAppVersion() =>
        Assembly.GetExecutingAssembly().GetName().Version?.ToString(3) ?? "1.0.0";

    public string GetVhVersion()
    {
        try
        {
            var path = Path.Combine(MainWindow.AppDataFolder, "versions.json");
            if (!File.Exists(path)) return "";
            var json = File.ReadAllText(path);
            var dict = JsonSerializer.Deserialize<Dictionary<string, string>>(json);
            return dict?.TryGetValue("_vhVersion", out var v) == true ? v ?? "" : "";
        }
        catch { return ""; }
    }

    public void CheckLauncherUpdate() => Task.Run(() => _w.CheckLauncherVersion());

    public void GetVHReleaseNotes() => Task.Run(() => _w.FetchVHReleaseNotes());

    public void PerformLauncherUpdate(string version, string zipUrl) =>
        Task.Run(() => _w.PerformLauncherUpdate(version, zipUrl));

    public bool GetLogUploadEnabled() => LogUploadService.IsEnabled();

    public void UploadLogs(string gamePath) =>
        Task.Run(async () =>
        {
            _w.RunScript("window.onLogUploadStarted && window.onLogUploadStarted()");
            var result = await LogUploadService.UploadLatestLogsAsync(gamePath);
            var escaped = JsonSerializer.Serialize(result);
            _w.RunScript($"window.onLogUploadFinished && window.onLogUploadFinished({escaped})");
        });

    public void StartInstallation(string gamePath, string vhMode, bool backup, string installMethod) =>
        Task.Run(() => _w.RunInstallation(gamePath, vhMode, backup, installMethod));

    public void LaunchGame(string gamePath, bool dx11, string installMethod) =>
        _w.LaunchGame(gamePath, dx11, installMethod);

    public void ForceQuitGame()
    {
        AppLogger.Info("Force quit game requested");
        var names = new[] { "WutheringWaves", "Client-Win64-Shipping", "Wuthering Waves" };
        foreach (var name in names)
            foreach (var p in Process.GetProcessesByName(name))
                try { p.Kill(true); }
                catch (Exception ex) { AppLogger.Exception(ex, "Failed to kill game process: " + name); }
    }

    public string Uninstall(string gamePath)
    {
        AppLogger.SetGamePath(gamePath);
        AppLogger.Info("Uninstall started");
        try
        {
            var baseDir = Helpers.GameBinaryFolderPath(gamePath);
            var pakPath = Path.Combine(Helpers.PakFolderPath(gamePath), Helpers.PakFileName);
            var method2PakPath = Helpers.Method2PakPath(gamePath);

            Helpers.RestoreSigBackup(gamePath);

            if (File.Exists(pakPath))
                File.Delete(pakPath);
            if (File.Exists(method2PakPath))
                File.Delete(method2PakPath);
            Helpers.DeleteLegacyPakFile(gamePath);
            Helpers.DeleteLegacyLoaderFiles(baseDir);

            var versionCache = Path.Combine(MainWindow.AppDataFolder, "versions.json");
            if (File.Exists(versionCache))
                File.Delete(versionCache);

            AppLogger.Info("Uninstall completed");
            return "ok";
        }
        catch (UnauthorizedAccessException)
        {
            AppLogger.Warn("Uninstall needs admin permission");
            return "Tidak memiliki izin menghapus file. Jalankan sebagai Admin.";
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Uninstall failed");
            return ex.Message;
        }
    }

    public void RestartAsAdmin()
    {
        _w.Dispatcher.Invoke(() =>
        {
            try
            {
                var exe = Process.GetCurrentProcess().MainModule?.FileName;
                if (!string.IsNullOrEmpty(exe))
                {
                    Process.Start(new ProcessStartInfo
                    {
                        FileName = exe,
                        UseShellExecute = true,
                        Verb = "runas"
                    });
                    AppLogger.Info("Restart as admin requested");
                    Application.Current.Shutdown();
                }
            }
            catch (Exception ex)
            {
                AppLogger.Exception(ex, "Failed to restart as admin");
            }
        });
    }






    static string GetPerfIniPath(string gamePath) =>
        Path.Combine(gamePath, @"Client\Saved\Config\WindowsNoEditor\Engine.ini");

    static string GetPerfIniBackupPath(string gamePath) =>
        Path.Combine(gamePath, @"Client\Saved\Config\WindowsNoEditor\Engine.ini.backup");

    public string ApplyPerformanceConfig(string gamePath, string settingsJson)
    {
        AppLogger.SetGamePath(gamePath);
        AppLogger.Info("Applying performance config");
        try
        {
            var iniPath    = GetPerfIniPath(gamePath);
            var backupPath = GetPerfIniBackupPath(gamePath);
            Directory.CreateDirectory(Path.GetDirectoryName(iniPath)!);

            if (!File.Exists(backupPath) && File.Exists(iniPath))
                File.Copy(iniPath, backupPath, overwrite: false);

            var originalContent = File.Exists(backupPath)
                ? File.ReadAllText(backupPath, System.Text.Encoding.UTF8)
                : "";

            using var doc = JsonDocument.Parse(settingsJson);
            var r = doc.RootElement;
            bool Get(string key) => r.TryGetProperty(key, out var v) && v.GetBoolean();

            var ss = new List<(string key, string value)>
            {
                ("r.VRS.EnableMaterial", "false"),
                ("r.VRS.EnableMesh", "false"),
                ("r.ParallelFrustumCull", "1"),
                ("r.ParallelOcclusionCull", "1"),
                ("a.URO.ForceAnimRate", "1"),
                ("r.Upscale.Quality", "3"),
                ("r.streaming.MeshMaxKeepMips", "15"),
                ("r.streaming.TextureMaxKeepMips", "15"),
                ("foliage.DensityScaleLOD.DrawCallOptimize", "1"),
                ("r.SceneColorFringeQuality", "0"),
            };

            if (Get("shadows"))
            {
                ss.Add(("r.Shadow.MaxCSMResolution", "256"));
                ss.Add(("r.Shadow.MaxResolution", "256"));
                ss.Add(("r.Shadow.MinResolution", "256"));
                ss.Add(("r.Shadow.PerObjectShadowMapResolution", "256"));
                ss.Add(("r.Shadow.PerObjectResolutionMax", "256"));
                ss.Add(("r.Shadow.PerObjectResolutionMin", "256"));
                ss.Add(("r.Shadow.RadiusThreshold", "0.06"));
                ss.Add(("r.Shadow.DistanceScale", "0.5"));
                ss.Add(("r.Shadow.ForbidHISMShadowStartIndex", "0"));
            }
            if (Get("ssr"))
            {
                ss.Add(("r.SSR.MaxRoughness", "0.1"));
                ss.Add(("r.SSR.HalfResSceneColor", "1"));
            }
            if (Get("ao"))   ss.Add(("r.AmbientOcclusionMaxQuality", "0"));
            if (Get("bloom"))
            {
                ss.Add(("r.Kuro.KuroEnableFFTBloom", "0"));
                ss.Add(("r.Kuro.KuroEnableToonFFTBloom", "0"));
            }
            if (Get("lensFlare"))
            {
                ss.Add(("r.DrawKuroPPLensflare", "0"));
                ss.Add(("r.EnableLensflareSceneSample", "0"));
                ss.Add(("r.kuro.kuroEnableScreenLeak", "0"));
            }
            if (Get("dof"))  ss.Add(("r.DepthOfFieldQuality", "0"));
            if (Get("materials"))
            {
                ss.Add(("r.KuroMaterialQualityLevel", "2"));
                ss.Add(("r.MaterialQualityLevel", "2"));
                ss.Add(("r.DetailMode", "0"));
                ss.Add(("r.Kuro.MaterialDesktopQualityShoulderRender", "0"));
            }
            if (Get("sss"))
            {
                ss.Add(("r.SSS.Scale", "0"));
                ss.Add(("r.SSS.Quality","0"));
            }
            if (Get("viewDist"))
            {
                ss.Add(("r.ViewDistanceScale", "0.8"));
                ss.Add(("r.ScreenSizeCullRatioFactor", "10"));
                ss.Add(("r.StaticMeshLODDistanceScale", "0.7"));
                ss.Add(("wp.Runtime.PlannedLoadingRangeScale", "0.4"));
                ss.Add(("wp.Runtime.SoraGridBlackListHeight", "5000"));
            }
            if (Get("foliage"))
            {
                ss.Add(("foliage.CullAll", "1"));
                ss.Add(("r.Kuro.Foliage.GrassCullDistanceMax", "2000"));
                ss.Add(("r.Kuro.Foliage.Grass3_0CullDistanceMax", "2000"));
            }
            if (Get("foliageInteract"))
            {
                ss.Add(("r.Kuro.InteractionEffect.EnableFoliageEffect", "0"));
                ss.Add(("r.Kuro.InteractionEffect.UseCppWaterEffect", "0"));
            }
            if (Get("particles"))
            {
                ss.Add(("r.EmitterSpawnRateScale", "0.125"));
                ss.Add(("fx.Niagara.QualityLevel", "0"));
                ss.Add(("r.ParticleLightQuality", "0"));
            }
            if (Get("clouds")) ss.Add(("r.KuroVolumeCloudEnable", "0"));
            if (Get("volumetric"))
            {
                ss.Add(("r.KuroVolumetricLight.DownSampleFactor", "4"));
                ss.Add(("r.KuroVolumetricLight.ColorMaskDownSampleFactor", "4"));
                ss.Add(("r.LightShaftDownSampleFactor", "2"));
                ss.Add(("r.SSFS", "0"));
            }

            var toSet = new Dictionary<string, List<(string key, string value)>>
            {
                ["SystemSettings"] = ss,
                ["/Script/Engine.RendererSettings"] = new List<(string, string)>
                {
                    ("r.RayTracing.LoadConfig", "0"),
                },
            };

            File.WriteAllText(iniPath, Helpers.PatchIniContent(originalContent, toSet), System.Text.Encoding.UTF8);
            AppLogger.Info("Performance config applied");
            return "ok";
        }
        catch (UnauthorizedAccessException)
        {
            AppLogger.Warn("Performance config needs admin permission");
            return "Tidak memiliki izin menulis file. Jalankan Launcher sebagai Admin.";
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Performance config failed");
            return ex.Message;
        }
    }

    public string ClearPerformanceConfig(string gamePath)
    {
        AppLogger.SetGamePath(gamePath);
        AppLogger.Info("Clearing performance config");
        try
        {
            var iniPath    = GetPerfIniPath(gamePath);
            var backupPath = GetPerfIniBackupPath(gamePath);
            if (!File.Exists(backupPath)) return "no_backup";
            File.Copy(backupPath, iniPath, overwrite: true);
            File.Delete(backupPath);
            AppLogger.Info("Performance config cleared");
            return "ok";
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Clear performance config failed");
            return ex.Message;
        }
    }

    public bool GetPerformanceConfigActive(string gamePath) =>
        File.Exists(GetPerfIniBackupPath(gamePath));
}
