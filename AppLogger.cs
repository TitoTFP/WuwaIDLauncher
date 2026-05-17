using System.IO;
using System.Text;

namespace WuwaIDLauncher;

internal static class AppLogger
{
    static readonly object Sync = new();
    static readonly List<(string Value, string Replacement)> Redactions = new();
    static string? _logFolder;

    internal static void Initialize(string appDataFolder)
    {
        try
        {
            _logFolder = Path.Combine(appDataFolder, "Logs");
            Directory.CreateDirectory(_logFolder);
            AddRedaction(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "<USERPROFILE>");
            DeleteOldLogs(TimeSpan.FromDays(14));
            Info("Logger initialized");
        }
        catch { }
    }

    internal static void SetGamePath(string? gamePath)
    {
        AddRedaction(gamePath, "<GAME_PATH>");
    }

    internal static void Info(string message) => Write("INFO", message);

    internal static void Warn(string message) => Write("WARN", message);

    internal static void Error(string message) => Write("ERROR", message);

    internal static void Exception(Exception exception, string message) =>
        Write("ERROR", $"{message}: {exception}");

    static void Write(string level, string message)
    {
        try
        {
            var folder = _logFolder;
            if (string.IsNullOrWhiteSpace(folder))
                return;

            Directory.CreateDirectory(folder);
            var line = $"[{DateTimeOffset.Now:yyyy-MM-dd HH:mm:ss.fff zzz}] {level} {Redact(message)}{Environment.NewLine}";
            var path = Path.Combine(folder, $"launcher-{DateTime.Now:yyyyMMdd}.log");

            lock (Sync)
            {
                File.AppendAllText(path, line, Encoding.UTF8);
            }
        }
        catch { }
    }

    static void AddRedaction(string? value, string replacement)
    {
        try
        {
            if (string.IsNullOrWhiteSpace(value))
                return;

            var normalized = value.Trim().TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
            if (normalized.Length == 0)
                return;

            lock (Sync)
            {
                Redactions.RemoveAll(x => string.Equals(x.Value, normalized, StringComparison.OrdinalIgnoreCase));
                Redactions.Add((normalized, replacement));
            }
        }
        catch { }
    }

    static string Redact(string message)
    {
        try
        {
            var redacted = message;
            List<(string Value, string Replacement)> redactions;
            lock (Sync)
            {
                redactions = Redactions.ToList();
            }

            foreach (var (value, replacement) in redactions.OrderByDescending(x => x.Value.Length))
            {
                redacted = redacted.Replace(value, replacement, StringComparison.OrdinalIgnoreCase);
            }

            return redacted;
        }
        catch
        {
            return message;
        }
    }

    static void DeleteOldLogs(TimeSpan maxAge)
    {
        try
        {
            var folder = _logFolder;
            if (string.IsNullOrWhiteSpace(folder) || !Directory.Exists(folder))
                return;

            var cutoff = DateTime.UtcNow - maxAge;
            foreach (var file in Directory.EnumerateFiles(folder, "launcher-*.log"))
            {
                try
                {
                    if (File.GetLastWriteTimeUtc(file) < cutoff)
                        File.Delete(file);
                }
                catch { }
            }
        }
        catch { }
    }
}
