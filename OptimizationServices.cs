using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Text.Json;

namespace WuwaIDLauncher;

internal enum PatchState
{
    Cached,
    Current,
    UpdateAvailable,
    NotInstalled,
    Offline,
    InvalidPath,
    Error
}

internal sealed record PatchStatusResult(
    string State,
    bool CanLaunch,
    string Message,
    string InstallMethod = "",
    bool IsRefreshing = false)
{
    internal static PatchStatusResult From(
        PatchState state,
        bool canLaunch,
        string message,
        bool isRefreshing = false) =>
        new(state switch
        {
            PatchState.Cached => "cached",
            PatchState.Current => "current",
            PatchState.UpdateAvailable => "update_available",
            PatchState.NotInstalled => "not_installed",
            PatchState.Offline => "offline",
            PatchState.InvalidPath => "invalid_path",
            _ => "error"
        }, canLaunch, message, IsRefreshing: isRefreshing);
}

internal sealed record PatchAssetStatus(string Name, string Path, string Fingerprint);

internal static class ReleaseChecksumManifest
{
    internal static Dictionary<string, string> Parse(string content)
    {
        var checksums = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var rawLine in content.Replace("\r", "").Split('\n'))
        {
            var parts = rawLine.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length == 0)
                continue;
            if (parts.Length != 2 || !Helpers.IsSha256(parts[0]))
                throw new InvalidDataException("Format SHA256sums.txt tidak valid.");

            var name = parts[1].TrimStart('*');
            if (string.IsNullOrWhiteSpace(name) || name.Contains('/') || name.Contains('\\') ||
                !checksums.TryAdd(name, parts[0].ToLowerInvariant()))
                throw new InvalidDataException("Nama asset pada SHA256sums.txt tidak valid atau duplikat.");
        }

        return checksums;
    }
}

internal static class PatchAssetCache
{
    internal static bool PromoteVerified(
        IDictionary<string, string> cache,
        IReadOnlyList<PatchAssetStatus> assets,
        bool verifyCanonicalPak = false)
    {
        var changed = false;
        var legacyPakCache = cache.ContainsKey(Helpers.ManualPakFileName);
        var canonicalPakCurrent = false;
        foreach (var asset in assets)
        {
            if (!File.Exists(asset.Path) || !Helpers.IsSha256(asset.Fingerprint))
                continue;

            var isCanonicalPak = asset.Name.Equals(Helpers.PakFileName, StringComparison.OrdinalIgnoreCase);
            var cacheMatches = cache.TryGetValue(asset.Name, out var cachedFingerprint) &&
                               string.Equals(cachedFingerprint, asset.Fingerprint, StringComparison.OrdinalIgnoreCase);
            var mustVerify = !cacheMatches || !MetadataMatches(cache, asset) ||
                             (isCanonicalPak && (legacyPakCache || verifyCanonicalPak));
            if (mustVerify)
            {
                if (!Helpers.VerifySha256(asset.Path, asset.Fingerprint))
                {
                    if (cacheMatches)
                    {
                        cache.Remove(asset.Name);
                        RemoveMetadata(cache, asset.Name);
                        changed = true;
                    }
                    continue;
                }

                changed |= RecordVerified(cache, asset);
            }

            canonicalPakCurrent |= isCanonicalPak;
        }

        if (canonicalPakCurrent && cache.Remove(Helpers.ManualPakFileName))
            changed = true;

        return changed;
    }

    internal static bool RecordVerified(IDictionary<string, string> cache, PatchAssetStatus asset)
    {
        var info = new FileInfo(asset.Path);
        var changed = Set(cache, asset.Name, asset.Fingerprint) |
                      Set(cache, SizeKey(asset.Name), info.Length.ToString(CultureInfo.InvariantCulture)) |
                      Set(cache, MtimeKey(asset.Name), info.LastWriteTimeUtc.Ticks.ToString(CultureInfo.InvariantCulture));
        if (asset.Name.Equals(Helpers.PakFileName, StringComparison.OrdinalIgnoreCase))
        {
            changed |= cache.Remove(Helpers.ManualPakFileName);
            changed |= RemoveMetadata(cache, Helpers.ManualPakFileName);
        }

        return changed;
    }

    internal static bool TryCopyVerified(string sourcePath, string destPath, string sha256)
    {
        if (!File.Exists(sourcePath) || !Helpers.VerifySha256(sourcePath, sha256))
            return false;

        Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);
        var tempPath = destPath + ".reuse.tmp";
        File.Copy(sourcePath, tempPath, overwrite: true);
        if (!Helpers.VerifySha256(tempPath, sha256))
        {
            File.Delete(tempPath);
            return false;
        }

        File.Move(tempPath, destPath, overwrite: true);
        return true;
    }

    static bool MetadataMatches(IDictionary<string, string> cache, PatchAssetStatus asset)
    {
        var info = new FileInfo(asset.Path);
        return cache.TryGetValue(SizeKey(asset.Name), out var size) &&
               size == info.Length.ToString(CultureInfo.InvariantCulture) &&
               cache.TryGetValue(MtimeKey(asset.Name), out var mtime) &&
               mtime == info.LastWriteTimeUtc.Ticks.ToString(CultureInfo.InvariantCulture);
    }

    static bool Set(IDictionary<string, string> cache, string key, string value)
    {
        if (cache.TryGetValue(key, out var current) && current == value)
            return false;
        cache[key] = value;
        return true;
    }

    static bool RemoveMetadata(IDictionary<string, string> cache, string name) =>
        cache.Remove(SizeKey(name)) | cache.Remove(MtimeKey(name));

    static string SizeKey(string name) => name + "#size";
    static string MtimeKey(string name) => name + "#mtime";
}

internal static class PatchStatusEvaluator
{
    internal static PatchStatusResult EvaluateCached(
        string gamePath,
        string installMethod,
        IReadOnlyDictionary<string, string> localCache,
        IReadOnlyList<PatchAssetStatus> assets)
    {
        var local = Evaluate(gamePath, installMethod, localCache, assets, remoteAvailable: false);
        return local.CanLaunch
            ? PatchStatusResult.From(
                PatchState.Cached,
                true,
                "Patch lokal siap; memeriksa pembaruan di latar belakang.",
                isRefreshing: true)
            : local;
    }

    internal static PatchStatusResult Evaluate(
        string gamePath,
        string installMethod,
        IReadOnlyDictionary<string, string> localCache,
        IReadOnlyList<PatchAssetStatus> assets,
        bool remoteAvailable)
    {
        if (string.IsNullOrWhiteSpace(gamePath) || !Directory.Exists(Helpers.GameBinaryFolderPath(gamePath)))
            return PatchStatusResult.From(PatchState.InvalidPath, false, "Direktori game tidak valid.");

        if (assets.Count == 0)
            return PatchStatusResult.From(PatchState.Error, false, "Status patch tidak dapat ditentukan.");

        if (assets.Any(asset => !File.Exists(asset.Path)))
            return PatchStatusResult.From(PatchState.NotInstalled, false, "Patch ID belum terpasang untuk metode ini.");

        var normalizedMethod = NormalizeInstallMethod(installMethod);
        var cachedMethodMatches = localCache.TryGetValue("_installMethod", out var cachedMethod) &&
                                  string.Equals(cachedMethod, normalizedMethod, StringComparison.OrdinalIgnoreCase);
        var cachedAssetsMatch = assets.All(asset =>
            localCache.TryGetValue(asset.Name, out var cachedFingerprint) &&
            !string.IsNullOrWhiteSpace(cachedFingerprint) &&
            (string.IsNullOrEmpty(asset.Fingerprint) ||
             string.Equals(cachedFingerprint, asset.Fingerprint, StringComparison.OrdinalIgnoreCase)));

        if (!remoteAvailable)
        {
            var canLaunch = cachedMethodMatches && cachedAssetsMatch;
            return PatchStatusResult.From(PatchState.Offline, canLaunch,
                canLaunch
                    ? "Tidak dapat memeriksa pembaruan; patch lokal terakhir masih dapat digunakan."
                    : "Tidak dapat memeriksa pembaruan dan status patch lokal belum terverifikasi.");
        }

        if (cachedMethodMatches && cachedAssetsMatch)
            return PatchStatusResult.From(PatchState.Current, true, "Patch ID sudah versi terbaru.");

        return PatchStatusResult.From(PatchState.UpdateAvailable, false, "Pembaruan Patch ID tersedia.");
    }

    internal static Dictionary<string, string> ReadVersionCache(string path)
    {
        try
        {
            var cache = File.Exists(path)
                ? JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(path)) ?? new()
                : new();

            if (!cache.ContainsKey(Helpers.PakFileName) &&
                cache.TryGetValue(Helpers.ManualPakFileName, out var legacyFingerprint))
                cache[Helpers.PakFileName] = legacyFingerprint;

            return cache;
        }
        catch
        {
            return new();
        }
    }

    internal static string NormalizeInstallMethod(string? method) =>
        string.Equals(method, "method2", StringComparison.OrdinalIgnoreCase) ? "method2" : "method1";
}

internal enum Method1Completion
{
    RestoreAndShow,
    RestoreAndTray
}

internal static class LaunchLifecyclePolicy
{
    internal static Method1Completion Method1(bool gameRunningAtDeadline) =>
        gameRunningAtDeadline ? Method1Completion.RestoreAndTray : Method1Completion.RestoreAndShow;

    internal static bool MayCloseAfterSignatureRestore(bool restoreSucceeded) => restoreSucceeded;

    internal static bool ShouldDeferClose(bool signatureRestorePending, bool gameRunning) =>
        signatureRestorePending && gameRunning;

    internal static async Task<bool> WaitForStableGameStartAsync(
        Func<bool> isGameRunning,
        TimeSpan sampleInterval,
        TimeSpan timeout,
        CancellationToken cancellationToken = default)
    {
        var deadline = Stopwatch.StartNew();
        var consecutiveSamples = 0;
        while (deadline.Elapsed < timeout)
        {
            cancellationToken.ThrowIfCancellationRequested();
            consecutiveSamples = isGameRunning() ? consecutiveSamples + 1 : 0;
            if (consecutiveSamples >= 2)
                return true;
            await Task.Delay(sampleInterval, cancellationToken);
        }
        return false;
    }
}

internal static class PatchStatusDelivery
{
    internal static bool ShouldPublish(
        int requestId,
        int latestRequestId,
        bool launchInProgress,
        bool externalGameActive) =>
        requestId == latestRequestId && !launchInProgress && !externalGameActive;
}

internal sealed record MediaCacheEntry(string Sha256, long Size, long LastWriteUtcTicks);

internal sealed class MediaCacheState
{
    public Dictionary<string, MediaCacheEntry> Assets { get; init; } = new(StringComparer.OrdinalIgnoreCase);

    internal static MediaCacheState Load(string path)
    {
        try
        {
            return File.Exists(path)
                ? JsonSerializer.Deserialize<MediaCacheState>(File.ReadAllText(path)) ?? new()
                : new();
        }
        catch
        {
            return new();
        }
    }

    internal bool CanSkipHash(string name, string path, string expectedSha256)
    {
        if (!File.Exists(path) || !Assets.TryGetValue(name, out var entry))
            return false;

        var info = new FileInfo(path);
        return entry.Sha256.Equals(expectedSha256, StringComparison.OrdinalIgnoreCase) &&
               entry.Size == info.Length &&
               entry.LastWriteUtcTicks == info.LastWriteTimeUtc.Ticks;
    }

    internal void Record(string name, string path, string sha256)
    {
        var info = new FileInfo(path);
        Assets[name] = new MediaCacheEntry(sha256, info.Length, info.LastWriteTimeUtc.Ticks);
    }

    internal void Save(string path)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, JsonSerializer.Serialize(this));
    }
}

internal static class LauncherHttp
{
    static int _startupRequestCount;
    static readonly SocketsHttpHandler Handler = new()
    {
        AutomaticDecompression = System.Net.DecompressionMethods.All,
        PooledConnectionLifetime = TimeSpan.FromMinutes(10)
    };

    internal static readonly HttpClient Client = CreateClient();
    internal static int StartupRequestCount => Volatile.Read(ref _startupRequestCount);

    static HttpClient CreateClient()
    {
        var client = new HttpClient(new CountingHandler(Handler), disposeHandler: false)
        {
            Timeout = Timeout.InfiniteTimeSpan
        };
        client.DefaultRequestHeaders.UserAgent.ParseAdd("WuwaIDLauncher/1.0");
        return client;
    }

    internal static CancellationTokenSource TimeoutAfter(TimeSpan timeout, CancellationToken cancellationToken = default)
    {
        var source = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        source.CancelAfter(timeout);
        return source;
    }

    sealed class CountingHandler(HttpMessageHandler innerHandler) : DelegatingHandler(innerHandler)
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request, CancellationToken cancellationToken)
        {
            if (App.StartupClock.Elapsed <= TimeSpan.FromSeconds(5))
                Interlocked.Increment(ref _startupRequestCount);
            return base.SendAsync(request, cancellationToken);
        }
    }
}
