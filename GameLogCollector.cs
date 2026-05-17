using System.IO;
using System.Text;
using System.Text.RegularExpressions;

namespace WuwaIDLauncher;

internal sealed record LogUploadItem(string EntryName, byte[] Content);

internal static partial class GameLogCollector
{
    const int MaxClientBackups = 2;
    const int MaxCrashFolders = 1;
    const int MaxAuxLogsPerFolder = 3;

    internal static List<LogUploadItem> Collect(string? gamePath, long maxBytes)
    {
        var items = new List<LogUploadItem>();
        if (string.IsNullOrWhiteSpace(gamePath) || maxBytes <= 0)
            return items;

        var root = gamePath.Trim();
        if (!Directory.Exists(root))
            return items;

        var sanitizer = new GameLogSanitizer(root);
        var candidates = EnumerateCandidates(root);
        long used = 0;

        foreach (var candidate in candidates)
        {
            if (used >= maxBytes)
                break;

            try
            {
                var remaining = maxBytes - used;
                var content = ReadSanitizedTail(candidate.Path, remaining, sanitizer);
                if (content.Length == 0)
                    continue;

                used += content.Length;
                items.Add(new LogUploadItem(candidate.EntryName, content));
            }
            catch (Exception ex)
            {
                AppLogger.Exception(ex, "Failed to collect game log: " + candidate.EntryName);
            }
        }

        return items;
    }

    static IEnumerable<(string Path, string EntryName)> EnumerateCandidates(string gamePath)
    {
        var logsDir = Path.Combine(gamePath, "Client", "Saved", "Logs");
        var currentLog = Path.Combine(logsDir, "Client.log");
        if (File.Exists(currentLog))
            yield return (currentLog, "logs/Client.log");

        if (Directory.Exists(logsDir))
        {
            foreach (var file in Directory.EnumerateFiles(logsDir, "Client-backup-*.log")
                         .Select(path => new FileInfo(path))
                         .Where(file => file.Exists)
                         .OrderByDescending(file => file.LastWriteTimeUtc)
                         .Take(MaxClientBackups))
            {
                yield return (file.FullName, "logs/" + SafeName(file.Name));
            }
        }

        var crashesDir = Path.Combine(gamePath, "Client", "Saved", "Crashes");
        if (Directory.Exists(crashesDir))
        {
            foreach (var dir in Directory.EnumerateDirectories(crashesDir)
                         .Select(path => new DirectoryInfo(path))
                         .Where(dir => dir.Exists)
                         .OrderByDescending(dir => dir.LastWriteTimeUtc)
                         .Take(MaxCrashFolders))
            {
                foreach (var name in new[] { "Client.log", "CrashContext.runtime-xml", "CrashReportClient.ini" })
                {
                    var path = Path.Combine(dir.FullName, name);
                    if (File.Exists(path))
                        yield return (path, "crashes/" + SafeName(dir.Name) + "/" + SafeName(name));
                }
            }
        }

        var crashSightDir = Path.Combine(gamePath, "Client", "Binaries", "Win64", "CrashSightLog");
        foreach (var file in LatestFiles(crashSightDir, "*.log", MaxAuxLogsPerFolder))
            yield return (file.FullName, "aux/CrashSightLog/" + SafeName(file.Name));

        var pipeClientDir = Path.Combine(gamePath, "Client", "Binaries", "Win64", "pipe_client");
        foreach (var file in LatestFiles(pipeClientDir, "*.log", MaxAuxLogsPerFolder))
            yield return (file.FullName, "aux/pipe_client/" + SafeName(file.Name));

        var cgsdkLog = Path.Combine(gamePath, "Client", "Binaries", "Win64", "cgsdk_.log");
        if (File.Exists(cgsdkLog))
            yield return (cgsdkLog, "aux/cgsdk_.log");
    }

    static IEnumerable<FileInfo> LatestFiles(string folder, string pattern, int count)
    {
        if (!Directory.Exists(folder))
            return [];

        return Directory.EnumerateFiles(folder, pattern)
            .Select(path => new FileInfo(path))
            .Where(file => file.Exists && file.Length > 0)
            .OrderByDescending(file => file.LastWriteTimeUtc)
            .Take(count)
            .ToList();
    }

    static byte[] ReadSanitizedTail(string path, long maxBytes, GameLogSanitizer sanitizer)
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
        var text = Encoding.UTF8.GetString(buffer, 0, read);
        text = SanitizeTextLog(text, sanitizer);
        return Encoding.UTF8.GetBytes(text);
    }

    static string SanitizeTextLog(string text, GameLogSanitizer sanitizer) =>
        sanitizer.Sanitize(text);

    static string SafeName(string name) =>
        Path.GetFileName(name).Replace('\\', '_').Replace('/', '_');

    sealed partial class GameLogSanitizer
    {
        readonly string _gamePath;

        internal GameLogSanitizer(string gamePath)
        {
            _gamePath = gamePath.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        }

        internal string Sanitize(string text)
        {
            var result = text
                .Replace(_gamePath, "<GAME_PATH>", StringComparison.OrdinalIgnoreCase)
                .Replace(_gamePath.Replace('/', '\\'), "<GAME_PATH>", StringComparison.OrdinalIgnoreCase)
                .Replace(_gamePath.Replace('\\', '/'), "<GAME_PATH>", StringComparison.OrdinalIgnoreCase);

            result = HomePathRegex().Replace(result, "<USERPROFILE>");
            result = WindowsUserPathRegex().Replace(result, "<USERPROFILE>");
            result = GamePathRegex().Replace(result, "<GAME_PATH>");
            result = KeyValueLineRegex().Replace(result, "$1<REDACTED>");
            result = XmlFieldRegex().Replace(result, "<$1><REDACTED></$1>");
            result = GuidRegex().Replace(result, "<GUID>");
            result = LongBase64Regex().Replace(result, "<ENCODED_PAYLOAD>");
            return result;
        }

        [GeneratedRegex(@"(?i)(?:Z:)?[\\/]+home[\\/]+[^\\/\s""'<]+")]
        private static partial Regex HomePathRegex();

        [GeneratedRegex(@"(?i)[A-Z]:[\\/]Users[\\/][^\\/\s""'<]+")]
        private static partial Regex WindowsUserPathRegex();

        [GeneratedRegex(@"(?i)(?:[A-Z]:|Z:)?[\\/][^""'<\r\n]*Wuthering Waves(?: Game)?")]
        private static partial Regex GamePathRegex();

        [GeneratedRegex(@"(?im)^((?:.*\b)?(?:Computer|User|UserName|MachineId|DeviceId|LoginId)\s*[:=]\s*).*$")]
        private static partial Regex KeyValueLineRegex();

        [GeneratedRegex(@"(?is)<(Computer|User|UserName|MachineId|DeviceId|LoginId)>(.*?)</\1>")]
        private static partial Regex XmlFieldRegex();

        [GeneratedRegex(@"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b|\b[0-9a-f]{32}\b")]
        private static partial Regex GuidRegex();

        [GeneratedRegex(@"\b[A-Za-z0-9+/]{80,}={0,2}\b")]
        private static partial Regex LongBase64Regex();
    }
}
