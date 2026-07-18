using System.Diagnostics;
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
            (string.IsNullOrEmpty(asset.Fingerprint) || cachedFingerprint == asset.Fingerprint));

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
            return File.Exists(path)
                ? JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText(path)) ?? new()
                : new();
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
