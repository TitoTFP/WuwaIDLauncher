using System.Net.Http;
using System.Reflection;
using System.Text;
using System.Text.Json;
using System.IO;

namespace WuwaIDLauncher;

internal static class ActivePlayerService
{
    internal const string ActiveHeartbeatEndpoint = "https://logs.titotfp.my.id/api/active/heartbeat";
    static readonly TimeSpan HeartbeatInterval = TimeSpan.FromMinutes(5);
    static readonly object Gate = new();
    static Timer? _timer;
    static CancellationTokenSource? _work;

    internal static void Start(string? installMethod = null)
    {
        lock (Gate)
        {
            if (_timer != null) return;
            var work = _work = new CancellationTokenSource();
            var token = work.Token;
            _ = SendHeartbeatAsync("open", installMethod, token);
            _timer = new Timer(_ => _ = SendHeartbeatAsync("heartbeat", installMethod, token),
                null, HeartbeatInterval, HeartbeatInterval);
        }
    }

    internal static void Stop()
    {
        lock (Gate)
        {
            _timer?.Dispose();
            _timer = null;
            _work?.Cancel();
            _work?.Dispose();
            _work = null;
        }
    }

    internal static Task SendLaunchHeartbeatAsync(string? installMethod) =>
        SendHeartbeatAsync("launch", installMethod);

    internal static async Task SendHeartbeatAsync(string eventName, string? installMethod, CancellationToken cancellationToken = default)
    {
        try
        {
            if (string.IsNullOrWhiteSpace(ActiveHeartbeatEndpoint))
                return;

            var clientId = GetOrCreateClientId();
            var json = BuildHeartbeatJson(clientId, GetAppVersion(), NormalizeInstallMethod(installMethod), eventName);
            using var timeout = LauncherHttp.TimeoutAfter(TimeSpan.FromSeconds(8), cancellationToken);
            using var content = new StringContent(json, Encoding.UTF8, "application/json");
            using var response = await LauncherHttp.Client.PostAsync(ActiveHeartbeatEndpoint, content, timeout.Token);
            if (response.IsSuccessStatusCode)
                AppLogger.Info("Active heartbeat sent: " + eventName);
            else
                AppLogger.Warn("Active heartbeat failed: HTTP " + (int)response.StatusCode);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested) { }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Active heartbeat failed");
        }
    }

    internal static string BuildHeartbeatJson(string clientId, string launcherVersion, string installMethod, string eventName) =>
        JsonSerializer.Serialize(new Dictionary<string, string>
        {
            ["client_id"] = clientId,
            ["launcher_version"] = launcherVersion,
            ["install_method"] = NormalizeInstallMethod(installMethod),
            ["event"] = string.IsNullOrWhiteSpace(eventName) ? "heartbeat" : eventName
        });

    internal static string NormalizeInstallMethod(string? installMethod) =>
        string.Equals(installMethod, "method2", StringComparison.OrdinalIgnoreCase)
            ? "method2"
            : "method1";

    static string GetOrCreateClientId()
    {
        Directory.CreateDirectory(MainWindow.AppDataFolder);
        var path = Path.Combine(MainWindow.AppDataFolder, "active-client-id.txt");
        if (File.Exists(path))
        {
            var existing = File.ReadAllText(path).Trim();
            if (!string.IsNullOrWhiteSpace(existing) && existing.Length <= 128)
                return existing;
        }

        var clientId = Guid.NewGuid().ToString("N");
        File.WriteAllText(path, clientId);
        return clientId;
    }

    static string GetAppVersion() =>
        Assembly.GetExecutingAssembly().GetName().Version?.ToString(3) ?? "1.0.0";
}
