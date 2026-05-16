using System.IO;
using System.IO.Compression;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;

namespace WuwaIDLauncher;

internal static class LogUploadService
{
    internal const string LogUploadEndpoint = "https://logs.titotfp.my.id/api/logs";
    const int MaxLogFiles = 3;
    const long MaxLogBytes = 2 * 1024 * 1024;
    const long MaxGameLogBytes = 4 * 1024 * 1024;

    internal static bool IsEnabled() =>
        !string.IsNullOrWhiteSpace(LogUploadEndpoint);

    internal static async Task<string> UploadLatestLogsAsync(string? gamePath = null, CancellationToken cancellationToken = default)
    {
        if (!IsEnabled())
            return "Log upload belum dikonfigurasi.";

        try
        {
            AppLogger.Info("Manual log upload started");
            var archive = CreateLogArchive(gamePath);
            if (archive.Length == 0)
                return "Belum ada log untuk dikirim.";

            using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };
            http.DefaultRequestHeaders.UserAgent.ParseAdd("WuwaIDLauncher/1.0");

            using var content = new MultipartFormDataContent();
            var zipContent = new ByteArrayContent(archive);
            zipContent.Headers.ContentType = new MediaTypeHeaderValue("application/zip");
            content.Add(zipContent, "logs", $"wuwaidlauncher-logs-{DateTime.UtcNow:yyyyMMddHHmmss}.zip");
            content.Add(new StringContent(GetAppVersion(), Encoding.UTF8), "appVersion");
            content.Add(new StringContent(DateTimeOffset.UtcNow.ToString("O"), Encoding.UTF8), "timestamp");
            content.Add(new StringContent(RuntimeInformation.OSDescription, Encoding.UTF8), "os");

            using var response = await http.PostAsync(LogUploadEndpoint, content, cancellationToken);
            response.EnsureSuccessStatusCode();

            AppLogger.Info("Manual log upload completed");
            return "Log berhasil dikirim.";
        }
        catch (Exception ex)
        {
            AppLogger.Exception(ex, "Manual log upload failed");
            return "Gagal mengirim log: " + ex.Message;
        }
    }

    static byte[] CreateLogArchive(string? gamePath)
    {
        var files = CollectLauncherLogs();
        var gameLogs = GameLogCollector.Collect(gamePath, MaxGameLogBytes);
        if (files.Count == 0 && gameLogs.Count == 0)
            return [];

        using var archiveStream = new MemoryStream();
        using (var archive = new ZipArchive(archiveStream, ZipArchiveMode.Create, leaveOpen: true))
        {
            long total = 0;
            foreach (var file in files)
            {
                if (total >= MaxLogBytes)
                    break;

                var remaining = MaxLogBytes - total;
                var bytes = ReadBoundedLogBytes(file.FullName, remaining);
                if (bytes.Length == 0)
                    continue;

                total += bytes.Length;
                WriteEntry(archive, "launcher/" + SafeEntryName(file.Name), bytes);
            }

            foreach (var item in gameLogs)
            {
                if (total >= MaxLogBytes + MaxGameLogBytes)
                    break;

                var remaining = MaxLogBytes + MaxGameLogBytes - total;
                var bytes = item.Content.Length > remaining
                    ? item.Content.Take((int)remaining).ToArray()
                    : item.Content;
                if (bytes.Length == 0)
                    continue;

                total += bytes.Length;
                WriteEntry(archive, "game/" + item.EntryName, bytes);
            }
        }

        return archiveStream.ToArray();
    }

    static List<FileInfo> CollectLauncherLogs()
    {
        var logsDir = Path.Combine(MainWindow.AppDataFolder, "Logs");
        if (!Directory.Exists(logsDir))
            return [];

        return Directory.EnumerateFiles(logsDir, "launcher-*.log")
            .Select(path => new FileInfo(path))
            .Where(file => file.Exists && file.Length > 0)
            .OrderByDescending(file => file.LastWriteTimeUtc)
            .Take(MaxLogFiles)
            .ToList();
    }

    static byte[] ReadBoundedLogBytes(string path, long maxBytes)
    {
        if (maxBytes <= 0)
            return [];

        var info = new FileInfo(path);
        var bytesToRead = (int)Math.Min(info.Length, maxBytes);
        var buffer = new byte[bytesToRead];

        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
        if (info.Length > bytesToRead)
            stream.Seek(-bytesToRead, SeekOrigin.End);
        var read = stream.Read(buffer, 0, bytesToRead);
        return read == bytesToRead ? buffer : buffer.Take(read).ToArray();
    }

    static string SafeEntryName(string fileName) =>
        Path.GetFileName(fileName).Replace('\\', '_').Replace('/', '_');

    static void WriteEntry(ZipArchive archive, string entryName, byte[] bytes)
    {
        var entry = archive.CreateEntry(entryName, CompressionLevel.Fastest);
        using var entryStream = entry.Open();
        entryStream.Write(bytes, 0, bytes.Length);
    }

    static string GetAppVersion() =>
        Assembly.GetExecutingAssembly().GetName().Version?.ToString(3) ?? "1.0.0";
}
