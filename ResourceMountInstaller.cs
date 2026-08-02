using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Security.Cryptography;
using System.Text;

namespace WuwaIDLauncher;

internal sealed record ResourceMountPlan(
    string ResourceVersion,
    string VersionDirectory,
    string MountDirectory,
    string SourceSignaturePath,
    string PakPath,
    string SignaturePath,
    string MountPath,
    IReadOnlyList<string> Conflicts)
{
    internal string OwnerMarkerPath => Path.Combine(Path.GetDirectoryName(PakPath)!, ResourceMountInstaller.OwnerMarkerFileName);
    internal string StagedPakPath => PakPath + ".new";
    internal string StagedSignaturePath => SignaturePath + ".new";
    internal string StagedMountPath => MountPath + ".new";
    internal string StagedOwnerMarkerPath => OwnerMarkerPath + ".new";
}

internal static class ResourceMountInstaller
{
    internal const string PatchFolderName = "wuwaindonesia";
    internal const string PatchPakFileName = "WuWaID_99_P.pak";
    internal const string PatchSignatureFileName = "WuWaID_99_P.sig";
    internal const string MountFileName = "wuwaindonesia.txt";
    internal const string OwnerMarkerFileName = ".wuwaid-resource-mount";

    const int MountPriority = 99;
    static readonly Encoding Utf8NoBom = new UTF8Encoding(false);

    internal static string ResourcesRootPath(string gamePath) =>
        Path.Combine(gamePath, "Client", "Saved", "Resources");

    internal static string ExpectedPakPath(string gamePath) =>
        TryProbe(gamePath, out var plan, out _)
            ? plan!.PakPath
            : Path.Combine(ResourcesRootPath(gamePath), PatchFolderName, PatchPakFileName);

    internal static ResourceMountPlan Probe(string gamePath)
    {
        if (!TryProbe(gamePath, out var plan, out var error))
            throw new InvalidDataException(error);
        return plan!;
    }

    internal static bool TryProbe(string gamePath, out ResourceMountPlan? plan, out string error)
    {
        plan = null;
        error = "";

        try
        {
            var resourcesRoot = ResourcesRootPath(gamePath);
            if (!Directory.Exists(resourcesRoot))
            {
                error = "Folder resource game tidak ditemukan.";
                return false;
            }

            var readyVersions = ResourceVersions(resourcesRoot)
                .Where(version => Exists(Path.Combine(version.Directory, "ResManifest")))
                .OrderByDescending(version => version.Version)
                .ToList();
            if (readyVersions.Count == 0)
            {
                error = "Resource game belum siap; ResManifest tidak ditemukan.";
                return false;
            }

            var active = readyVersions[0];
            var mountDirectory = Path.Combine(active.Directory, "Mount");
            if (!Directory.Exists(mountDirectory))
            {
                error = "Folder Mount tidak ditemukan pada resource game aktif.";
                return false;
            }

            var sourceSignature = FindOfficialSignature(active.Directory);
            if (sourceSignature == null)
            {
                error = "Signature resmi tidak ditemukan pada resource game aktif.";
                return false;
            }

            var artifacts = PathsForVersion(active.Directory);
            plan = new ResourceMountPlan(
                active.Name,
                active.Directory,
                mountDirectory,
                sourceSignature,
                artifacts.Pak,
                artifacts.Signature,
                artifacts.Mount,
                DetectConflicts(active.Directory, mountDirectory));
            return true;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or ArgumentException)
        {
            error = "Gagal memeriksa resource game: " + ex.Message;
            return false;
        }
    }

    internal static void EnsureWritable(ResourceMountPlan plan)
    {
        foreach (var directory in new[] { plan.VersionDirectory, plan.MountDirectory }
                     .Distinct(StringComparer.OrdinalIgnoreCase))
        {
            var testPath = Path.Combine(directory, $".wuwaid-write-test-{Guid.NewGuid():N}.tmp");
            try { File.WriteAllText(testPath, "test", Utf8NoBom); }
            finally { DeleteFile(testPath); }
        }
    }

    internal static bool IsHealthy(ResourceMountPlan plan) => IsHealthy(PathsForVersion(plan.VersionDirectory));

    static bool IsHealthy(ArtifactPaths artifacts)
    {
        try
        {
            if (!File.Exists(artifacts.Pak) || !File.Exists(artifacts.Signature) || !File.Exists(artifacts.Mount))
                return false;

            var expected = MountContent(Sha1(artifacts.Pak), Sha1(artifacts.Signature));
            var actual = File.ReadAllText(artifacts.Mount, Utf8NoBom)
                .Replace("\r\n", "\n", StringComparison.Ordinal);
            return IsOwnedMount(artifacts.Mount) && string.Equals(actual, expected, StringComparison.Ordinal);
        }
        catch
        {
            return false;
        }
    }

    internal static void Install(ResourceMountPlan plan, string sourcePakPath, string expectedSha256)
    {
        EnsureGameStopped();
        if (plan.Conflicts.Count > 0)
            throw new InvalidDataException("Konflik mod terdeteksi: " + string.Join(", ", plan.Conflicts));
        if (!Helpers.IsSha256(expectedSha256))
            throw new InvalidDataException("Checksum PAK resource mount tidak valid.");
        if (!File.Exists(sourcePakPath) || !Helpers.VerifySha256(sourcePakPath, expectedSha256))
            throw new InvalidDataException("PAK resource mount gagal diverifikasi.");
        if (!File.Exists(plan.SourceSignaturePath))
            throw new FileNotFoundException("Signature resmi tidak ditemukan.", plan.SourceSignaturePath);

        var artifacts = new[]
        {
            new Artifact(plan.OwnerMarkerPath, plan.StagedOwnerMarkerPath),
            new Artifact(plan.PakPath, plan.StagedPakPath),
            new Artifact(plan.SignaturePath, plan.StagedSignaturePath),
            new Artifact(plan.MountPath, plan.StagedMountPath)
        };
        if (!CanManageExistingArtifacts(plan, artifacts, expectedSha256))
            throw new InvalidDataException("Artefak Resource Mount yang ada bukan instalasi WuwaID yang tervalidasi.");

        RecoverAndClean(plan, artifacts);
        if (artifacts.Any(ArtifactExists) && !IsManaged(plan) &&
            !IsLegacyWuwaIDInstall(plan, expectedSha256))
            throw new InvalidDataException("Artefak Resource Mount yang ada tidak lengkap atau tidak tervalidasi.");
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(plan.PakPath)!);
            Directory.CreateDirectory(Path.GetDirectoryName(plan.MountPath)!);

            File.WriteAllText(plan.StagedOwnerMarkerPath, OwnerMarkerContent(expectedSha256), Utf8NoBom);
            File.Copy(sourcePakPath, plan.StagedPakPath, overwrite: true);
            if (!Helpers.VerifySha256(plan.StagedPakPath, expectedSha256))
                throw new InvalidDataException("PAK staging gagal diverifikasi.");

            File.Copy(plan.SourceSignaturePath, plan.StagedSignaturePath, overwrite: true);
            var mount = MountContent(Sha1(plan.StagedPakPath), Sha1(plan.StagedSignaturePath));
            File.WriteAllText(plan.StagedMountPath, mount, Utf8NoBom);
            if (!string.Equals(File.ReadAllText(plan.StagedMountPath, Utf8NoBom), mount, StringComparison.Ordinal))
                throw new IOException("Mount staging gagal diverifikasi.");
            var committed = new List<Artifact>();
            try
            {
                foreach (var artifact in artifacts)
                {
                    if (File.Exists(artifact.Target))
                        File.Move(artifact.Target, artifact.Backup, overwrite: true);
                    File.Move(artifact.Staged, artifact.Target, overwrite: true);
                    committed.Add(artifact);
                }

                if (!IsManaged(plan))
                    throw new IOException("Instalasi resource mount gagal diverifikasi.");
            }
            catch (Exception ex)
            {
                var rollbackError = Rollback(artifacts, committed);
                if (rollbackError != null)
                    throw new IOException("Instalasi gagal dan rollback tidak selesai; backup dipertahankan.",
                        new AggregateException(ex, rollbackError));
                throw;
            }

            CleanupTemporaryArtifacts(artifacts, includeBackups: true);
        }
        finally
        {
            CleanupTemporaryArtifacts(artifacts, includeBackups: false);
        }
    }

    internal static int CleanupInactiveVersions(string gamePath, string activeVersion)
    {
        EnsureGameStopped();
        var removed = 0;
        foreach (var version in ResourceVersions(ResourcesRootPath(gamePath)))
        {
            if (string.Equals(version.Name, activeVersion, StringComparison.OrdinalIgnoreCase))
                continue;
            try { removed += RemoveOwnedArtifacts(version.Directory); }
            catch { }
        }
        return removed;
    }

    internal static int RemoveAllOwnedArtifacts(string gamePath)
    {
        EnsureGameStopped();
        var removed = 0;
        foreach (var version in ResourceVersions(ResourcesRootPath(gamePath)))
            removed += RemoveOwnedArtifacts(version.Directory);
        return removed;
    }

    internal static string MountContent(string pakSha1, string signatureSha1) =>
        $"::Mount::\n{PatchFolderName}/{Path.GetFileNameWithoutExtension(PatchPakFileName)},{MountPriority},{pakSha1},{signatureSha1},,\n::Del::\n";

    static IEnumerable<ResourceVersion> ResourceVersions(string resourcesRoot)
    {
        if (!Directory.Exists(resourcesRoot))
            yield break;

        foreach (var directory in Directory.EnumerateDirectories(resourcesRoot))
        {
            var name = Path.GetFileName(directory);
            if (TryParseSemVer(name, out var version))
                yield return new ResourceVersion(name, directory, version!);
        }
    }

    static bool TryParseSemVer(string value, out Version? version)
    {
        version = null;
        var parts = value.Split('.');
        if (parts.Length != 3 || parts.Any(part => !int.TryParse(part, out var number) || number < 0))
            return false;
        return Version.TryParse(value, out version);
    }

    static string? FindOfficialSignature(string versionDirectory)
    {
        var englishRoot = Path.Combine(versionDirectory, "Lang_en");
        var candidates = Directory.Exists(englishRoot)
            ? Directory.EnumerateDirectories(englishRoot).OrderBy(path => path, StringComparer.OrdinalIgnoreCase).ToList()
            : [];
        candidates.Add(Path.Combine(versionDirectory, "Resource", "Base"));

        var ownStem = Path.GetFileNameWithoutExtension(PatchPakFileName);
        foreach (var directory in candidates)
        {
            if (!Directory.Exists(directory))
                continue;

            var signature = Directory.EnumerateFiles(directory)
                .OrderBy(path => path, StringComparer.OrdinalIgnoreCase)
                .FirstOrDefault(path =>
                    path.EndsWith(".sig", StringComparison.OrdinalIgnoreCase) &&
                    !Path.GetFileName(path).StartsWith(ownStem, StringComparison.OrdinalIgnoreCase) &&
                    IsOfficialSignature(versionDirectory, path));
            if (signature != null)
                return signature;
        }

        return null;
    }

    static IReadOnlyList<string> DetectConflicts(string versionDirectory, string mountDirectory)
    {
        var conflicts = new List<string>();
        if (Directory.Exists(Path.Combine(versionDirectory, "wuwaviethoa")))
            conflicts.Add("folder wuwaviethoa");

        foreach (var mount in Directory.EnumerateFiles(mountDirectory)
                     .Where(path => path.EndsWith(".txt", StringComparison.OrdinalIgnoreCase)))
        {
            var name = Path.GetFileName(mount);
            if (name.Equals(MountFileName, StringComparison.OrdinalIgnoreCase))
                continue;

            var content = File.ReadAllText(mount, Utf8NoBom);
            if (ContainsVietnamMod(name) || ContainsVietnamMod(content))
                conflicts.Add("mount Vietnam: " + name);
            if (name.Equals("MountLang_en.txt", StringComparison.OrdinalIgnoreCase) &&
                content.Contains("WuWaVH_99_P", StringComparison.OrdinalIgnoreCase))
                conflicts.Add("MountLang_en legacy WuWaVH");
            if (!name.StartsWith("MountLang_", StringComparison.OrdinalIgnoreCase) &&
                HasPriorityAtLeast(content, MountPriority))
                conflicts.Add("mount prioritas tinggi: " + name);
        }

        return conflicts.Distinct(StringComparer.OrdinalIgnoreCase).ToList();
    }

    static bool ContainsVietnamMod(string value) =>
        value.Contains("wuwaviethoa", StringComparison.OrdinalIgnoreCase) ||
        value.Contains("wuwavh", StringComparison.OrdinalIgnoreCase);

    static bool HasPriorityAtLeast(string content, int priority)
    {
        foreach (var line in content.Replace("\r", "", StringComparison.Ordinal).Split('\n'))
        {
            var fields = line.Split(',');
            if (fields.Length > 1 && int.TryParse(fields[1], out var value) && value >= priority)
                return true;
        }
        return false;
    }

    static bool IsOfficialSignature(string versionDirectory, string signaturePath)
    {
        var pairedPak = Path.ChangeExtension(signaturePath, ".pak");
        if (!File.Exists(pairedPak))
            return false;

        var relative = Path.GetRelativePath(versionDirectory, signaturePath).Replace('\\', '/');
        var mountName = relative.StartsWith("Lang_en/", StringComparison.OrdinalIgnoreCase)
            ? "MountLang_en.txt"
            : relative.StartsWith("Resource/", StringComparison.OrdinalIgnoreCase)
                ? "MountResource.txt"
                : null;
        if (mountName == null)
            return false;

        var mountPath = Path.Combine(versionDirectory, "Mount", mountName);
        if (!File.Exists(mountPath))
            return false;

        var mountEntry = relative[..^4];
        var pakSha1 = Sha1(pairedPak);
        var signatureSha1 = Sha1(signaturePath);
        return File.ReadLines(mountPath, Utf8NoBom).Any(line =>
        {
            var fields = line.Split(',');
            return fields.Length >= 4 &&
                   string.Equals(fields[0], mountEntry, StringComparison.OrdinalIgnoreCase) &&
                   string.Equals(fields[2], pakSha1, StringComparison.OrdinalIgnoreCase) &&
                   string.Equals(fields[3], signatureSha1, StringComparison.OrdinalIgnoreCase);
        });
    }

    static bool IsOwnedMount(string mountPath)
    {
        try
        {
            var lines = File.ReadAllText(mountPath, Utf8NoBom)
                .Replace("\r\n", "\n", StringComparison.Ordinal)
                .Split('\n', StringSplitOptions.None);
            if (lines.Length != 4 || lines[0] != "::Mount::" || lines[2] != "::Del::" || lines[3] != "")
                return false;

            var fields = lines[1].Split(',');
            return fields.Length == 6 &&
                   string.Equals(fields[0], $"{PatchFolderName}/{Path.GetFileNameWithoutExtension(PatchPakFileName)}", StringComparison.Ordinal) &&
                   fields[1] == MountPriority.ToString() &&
                   IsSha1(fields[2]) && IsSha1(fields[3]) && fields[4] == "" && fields[5] == "";
        }
        catch
        {
            return false;
        }
    }

    internal static bool IsManaged(ResourceMountPlan plan) =>
        IsHealthy(plan) &&
        HasVerifiedOwnerMarker(PathsForVersion(plan.VersionDirectory)) &&
        SignatureMatchesSource(plan);

    static bool CanManageExistingArtifacts(ResourceMountPlan plan, IEnumerable<Artifact> artifacts, string expectedSha256)
    {
        var patchDirectory = Path.GetDirectoryName(plan.PakPath)!;
        var hasFiles = artifacts.Any(ArtifactExists) ||
                       (Directory.Exists(patchDirectory) && Directory.EnumerateFileSystemEntries(patchDirectory).Any());
        if (!hasFiles || IsManaged(plan))
            return true;
        if (IsLegacyWuwaIDInstall(plan, expectedSha256))
            return true; // Safe migration from the pre-owner-marker format.
        var paths = PathsForVersion(plan.VersionDirectory);
        return HasStagedOwnerMarker(paths) || HasRecoverableOwnerMarker(paths);
    }

    static bool ArtifactExists(Artifact artifact) =>
        File.Exists(artifact.Target) || Directory.Exists(artifact.Target) ||
        File.Exists(artifact.Staged) || Directory.Exists(artifact.Staged) ||
        File.Exists(artifact.Backup) || Directory.Exists(artifact.Backup);

    static void RecoverAndClean(ResourceMountPlan plan, IEnumerable<Artifact> artifacts)
    {
        var all = artifacts.ToList();
        var paths = PathsForVersion(plan.VersionDirectory);
        var hasBackups = all.Any(artifact => File.Exists(artifact.Backup));
        if (hasBackups && !IsManaged(plan) && HasRecoverableOwnerMarker(paths))
        {
            foreach (var artifact in all.AsEnumerable().Reverse())
            {
                if (!File.Exists(artifact.Backup))
                    continue;
                DeleteFile(artifact.Target);
                File.Move(artifact.Backup, artifact.Target, overwrite: true);
            }
        }
        else if (!hasBackups && !IsManaged(plan) && HasRecoverableOwnerMarker(paths))
        {
            foreach (var artifact in all)
                DeleteFile(artifact.Target);
        }
        else if (hasBackups && IsManaged(plan))
        {
            foreach (var artifact in all)
                DeleteFile(artifact.Backup);
        }

        foreach (var artifact in all)
            DeleteFile(artifact.Staged);
    }

    static Exception? Rollback(IEnumerable<Artifact> artifacts, IEnumerable<Artifact> committed)
    {
        var failures = new List<Exception>();
        foreach (var artifact in committed.Reverse())
        {
            try { DeleteFile(artifact.Target); }
            catch (Exception ex) { failures.Add(ex); }
        }

        foreach (var artifact in artifacts.Reverse())
        {
            try
            {
                if (File.Exists(artifact.Backup))
                    File.Move(artifact.Backup, artifact.Target, overwrite: true);
            }
            catch (Exception ex) { failures.Add(ex); }
        }

        return failures.Count == 0 ? null : new AggregateException(failures);
    }

    static void CleanupTemporaryArtifacts(IEnumerable<Artifact> artifacts, bool includeBackups)
    {
        foreach (var artifact in artifacts)
        {
            DeleteFile(artifact.Staged);
            if (includeBackups)
                DeleteFile(artifact.Backup);
        }
    }

    static int RemoveOwnedArtifacts(string versionDirectory)
    {
        var artifacts = PathsForVersion(versionDirectory);
        var managed = IsHealthy(artifacts) && HasVerifiedOwnerMarker(artifacts);
        var partial = !IsHealthy(artifacts) &&
                      (HasRecoverableOwnerMarker(artifacts) || HasStagedOwnerMarker(artifacts));
        if (!managed && !partial)
            return 0;

        var removed = 0;
        foreach (var path in new[]
                 {
                     artifacts.Pak, artifacts.Signature, artifacts.Mount, artifacts.OwnerMarker,
                     artifacts.Pak + ".new", artifacts.Signature + ".new", artifacts.Mount + ".new", artifacts.OwnerMarker + ".new",
                     artifacts.Pak + ".bak", artifacts.Signature + ".bak", artifacts.Mount + ".bak", artifacts.OwnerMarker + ".bak"
                 })
        {
            if (!File.Exists(path))
                continue;
            File.Delete(path);
            removed++;
        }

        if (Directory.Exists(artifacts.Directory) && !Directory.EnumerateFileSystemEntries(artifacts.Directory).Any())
        {
            Directory.Delete(artifacts.Directory);
            removed++;
        }

        return removed;
    }

    static ArtifactPaths PathsForVersion(string versionDirectory)
    {
        var directory = Path.Combine(versionDirectory, PatchFolderName);
        return new ArtifactPaths(
            directory,
            Path.Combine(directory, PatchPakFileName),
            Path.Combine(directory, PatchSignatureFileName),
            Path.Combine(versionDirectory, "Mount", MountFileName),
            Path.Combine(directory, OwnerMarkerFileName));
    }

    static bool IsLegacyWuwaIDInstall(ResourceMountPlan plan, string expectedSha256) =>
        IsHealthy(plan) &&
        Helpers.VerifySha256(plan.PakPath, expectedSha256) &&
        SignatureMatchesSource(plan);

    static bool SignatureMatchesSource(ResourceMountPlan plan) =>
        File.Exists(plan.SourceSignaturePath) &&
        string.Equals(Sha1(plan.SignaturePath), Sha1(plan.SourceSignaturePath), StringComparison.OrdinalIgnoreCase);

    static bool HasVerifiedOwnerMarker(ArtifactPaths artifacts) =>
        TryReadOwnerMarker(artifacts.OwnerMarker, out var sha256) && Helpers.VerifySha256(artifacts.Pak, sha256);

    static bool HasRecoverableOwnerMarker(ArtifactPaths artifacts) =>
        TryReadOwnerMarker(artifacts.OwnerMarker, out _) ||
        TryReadOwnerMarker(artifacts.OwnerMarker + ".bak", out _);

    static bool HasStagedOwnerMarker(ArtifactPaths artifacts) =>
        TryReadOwnerMarker(artifacts.OwnerMarker + ".new", out _);

    static bool TryReadOwnerMarker(string path, out string sha256)
    {
        sha256 = "";
        try
        {
            var lines = File.ReadAllText(path, Utf8NoBom)
                .Replace("\r\n", "\n", StringComparison.Ordinal)
                .Split('\n', StringSplitOptions.None);
            if (lines.Length != 3 || lines[0] != "WuwaID Resource Mount v1" || !lines[1].StartsWith("sha256=", StringComparison.Ordinal) || lines[2] != "")
                return false;
            sha256 = lines[1]["sha256=".Length..];
            return Helpers.IsSha256(sha256);
        }
        catch
        {
            return false;
        }
    }

    static string OwnerMarkerContent(string sha256) =>
        $"WuwaID Resource Mount v1\nsha256={sha256.ToLowerInvariant()}\n";

    static void EnsureGameStopped()
    {
        if (Helpers.IsGameRunning())
            throw new InvalidOperationException("Tutup game sebelum mengubah Resource Mount.");
    }

    static bool IsSha1(string value) => value.Length == 40 && value.All(Uri.IsHexDigit);

    static bool Exists(string path) => File.Exists(path) || Directory.Exists(path);

    static string Sha1(string path)
    {
        using var stream = File.OpenRead(path);
        return Convert.ToHexString(SHA1.HashData(stream));
    }

    static void DeleteFile(string path)
    {
        if (File.Exists(path))
            File.Delete(path);
    }

    sealed record Artifact(string Target, string Staged)
    {
        internal string Backup => Target + ".bak";
    }

    sealed record ArtifactPaths(string Directory, string Pak, string Signature, string Mount, string OwnerMarker);

    sealed record ResourceVersion(string Name, string Directory, Version Version);
}
