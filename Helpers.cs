using System.Diagnostics;
using System.IO;
using System.Security.Cryptography;

namespace WuwaIDLauncher;

internal static class Helpers
{
    internal const string PakFolderRelativePath = @"Client\Content\Paks";
    internal const string SigFileName = "pakchunk7-WindowsNoEditor.sig";
    internal const string SigBackupFileName = "pakchunk7-WindowsNoEditor_backup.sig";
    internal const string ModFolderName = "wuwaIndonesia";
    internal const string LegacyModFolderName = "wuwaVietHoa";
    internal const string PakFileName = "pakchunk0-ID-WindowsNoEditor_1000_P.pak";
    internal const string LegacyPakFileName = "WuWaID_99_P.pak";
    internal const string ManualPakFileName = "WuWa_ID_99_P.pak";
    internal const string WinHttpLoaderFileName = "winhttp.dll";
    internal const string VersionLoaderFileName = "version.dll";
    const string GameProcessName = "Client-Win64-Shipping";

    internal static readonly TimeSpan SigRestoreDelay = TimeSpan.FromSeconds(150);

    internal static string PakFolderPath(string gamePath) =>
        Path.Combine(gamePath, PakFolderRelativePath);

    internal static string Method1PakPath(string gamePath) =>
        Path.Combine(PakFolderPath(gamePath), PakFileName);

    internal static string SigPath(string gamePath) =>
        Path.Combine(PakFolderPath(gamePath), SigFileName);

    internal static string SigBackupPath(string gamePath) =>
        Path.Combine(PakFolderPath(gamePath), SigBackupFileName);

    internal static string GameBinaryFolderPath(string gamePath) =>
        Path.Combine(gamePath, @"Client\Binaries\Win64");

    internal static string Method2PakFolderPath(string gamePath) =>
        Path.Combine(GameBinaryFolderPath(gamePath), ModFolderName);

    internal static string Method2PakPath(string gamePath) =>
        Path.Combine(Method2PakFolderPath(gamePath), ManualPakFileName);

    internal static string Method2LoaderPath(string gamePath) =>
        Path.Combine(GameBinaryFolderPath(gamePath), WinHttpLoaderFileName);

    internal static string AlternatePakPathForMethod(string gamePath, string installMethod) =>
        string.Equals(installMethod, "method2", StringComparison.OrdinalIgnoreCase)
            ? Method1PakPath(gamePath)
            : Method2PakPath(gamePath);

    internal static void RestoreSigBackup(string gamePath)
    {
        var sigPath = SigPath(gamePath);
        var backupPath = SigBackupPath(gamePath);

        if (File.Exists(backupPath) && !File.Exists(sigPath))
            File.Move(backupPath, sigPath);
        else if (File.Exists(backupPath) && File.Exists(sigPath))
            File.Delete(backupPath);
    }

    internal static bool IsGameRunning()
    {
        try { return Process.GetProcessesByName(GameProcessName).Length > 0; }
        catch { return false; }
    }

    internal static bool VerifySha256(string path, string expected)
    {
        try
        {
            using var sha = SHA256.Create();
            using var fs = File.OpenRead(path);
            var hash = sha.ComputeHash(fs);
            return Convert.ToHexString(hash).Equals(expected, StringComparison.OrdinalIgnoreCase);
        }
        catch { return false; }
    }

    internal static bool IsSha256(string value) =>
        value.Length == 64 && value.All(Uri.IsHexDigit);

    internal static string GetMimeType(string path) => Path.GetExtension(path).ToLowerInvariant() switch
    {
        ".html" => "text/html; charset=utf-8",
        ".css"  => "text/css; charset=utf-8",
        ".js"   => "application/javascript; charset=utf-8",
        ".json" => "application/json",
        ".png"  => "image/png",
        ".jpg" or ".jpeg" => "image/jpeg",
        ".svg"  => "image/svg+xml",
        ".woff" => "font/woff",
        ".woff2" => "font/woff2",
        ".webp" => "image/webp",
        ".mp4"  => "video/mp4",
        ".mp3"  => "audio/mpeg",
        _       => "application/octet-stream"
    };

    internal static void DeleteLegacyLoaderFiles(string baseDir)
    {
        var modDir = Path.Combine(baseDir, ModFolderName);
        var legacyModDir = Path.Combine(baseDir, LegacyModFolderName);
        var versionDll = Path.Combine(baseDir, VersionLoaderFileName);
        var winhttpDll = Path.Combine(baseDir, WinHttpLoaderFileName);
        var hidDll = Path.Combine(baseDir, "hid.dll"); // Legacy loader

        if (Directory.Exists(modDir))
            Directory.Delete(modDir, true);
        if (Directory.Exists(legacyModDir))
            Directory.Delete(legacyModDir, true);
        if (File.Exists(versionDll))
            File.Delete(versionDll);
        if (File.Exists(winhttpDll))
            File.Delete(winhttpDll);
        if (File.Exists(hidDll))
            File.Delete(hidDll);
    }

    internal static void DeleteManualLoaderFiles(string gamePath, bool preservePak = false)
    {
        var modDir = Method2PakFolderPath(gamePath);
        var winhttpDll = Method2LoaderPath(gamePath);

        if (File.Exists(winhttpDll))
            File.Delete(winhttpDll);

        if (!Directory.Exists(modDir))
            return;

        if (!preservePak)
        {
            Directory.Delete(modDir, true);
            return;
        }

        foreach (var file in Directory.EnumerateFiles(modDir))
        {
            if (!Path.GetFileName(file).Equals(ManualPakFileName, StringComparison.OrdinalIgnoreCase))
                File.Delete(file);
        }
    }

    internal static void DeleteLegacyPakFile(string gamePath)
    {
        var legacyPakPath = Path.Combine(PakFolderPath(gamePath), LegacyPakFileName);
        if (File.Exists(legacyPakPath))
            File.Delete(legacyPakPath);

        var manualPakPath = Path.Combine(PakFolderPath(gamePath), ManualPakFileName);
        if (File.Exists(manualPakPath))
            File.Delete(manualPakPath);
    }


    static readonly Dictionary<string, string[]> _managedPerfKeys = new(StringComparer.OrdinalIgnoreCase)
    {
        ["SystemSettings"] = new[]
        {
            "r.VRS.EnableMaterial", "r.VRS.EnableMesh",
            "r.ParallelFrustumCull", "r.ParallelOcclusionCull",
            "a.URO.ForceAnimRate", "r.Upscale.Quality",
            "r.streaming.MeshMaxKeepMips", "r.streaming.TextureMaxKeepMips",
            "foliage.DensityScaleLOD.DrawCallOptimize", "r.SceneColorFringeQuality",
            "r.Shadow.MaxCSMResolution", "r.Shadow.MaxResolution", "r.Shadow.MinResolution",
            "r.Shadow.PerObjectShadowMapResolution", "r.Shadow.PerObjectResolutionMax",
            "r.Shadow.PerObjectResolutionMin", "r.Shadow.RadiusThreshold",
            "r.Shadow.DistanceScale", "r.Shadow.ForbidHISMShadowStartIndex",
            "r.SSR.MaxRoughness", "r.SSR.HalfResSceneColor",
            "r.AmbientOcclusionMaxQuality",
            "r.Kuro.KuroEnableFFTBloom", "r.Kuro.KuroEnableToonFFTBloom",
            "r.DrawKuroPPLensflare", "r.EnableLensflareSceneSample", "r.kuro.kuroEnableScreenLeak",
            "r.DepthOfFieldQuality",
            "r.KuroMaterialQualityLevel", "r.MaterialQualityLevel", "r.DetailMode",
            "r.Kuro.MaterialDesktopQualityShoulderRender",
            "r.SSS.Scale", "r.SSS.Quality",
            "r.ViewDistanceScale", "r.ScreenSizeCullRatioFactor", "r.StaticMeshLODDistanceScale",
            "wp.Runtime.PlannedLoadingRangeScale", "wp.Runtime.SoraGridBlackListHeight",
            "foliage.CullAll", "r.Kuro.Foliage.GrassCullDistanceMax", "r.Kuro.Foliage.Grass3_0CullDistanceMax",
            "r.Kuro.InteractionEffect.EnableFoliageEffect", "r.Kuro.InteractionEffect.UseCppWaterEffect",
            "r.EmitterSpawnRateScale", "fx.Niagara.QualityLevel", "r.ParticleLightQuality",
            "r.KuroVolumeCloudEnable",
            "r.KuroVolumetricLight.DownSampleFactor", "r.KuroVolumetricLight.ColorMaskDownSampleFactor",
            "r.LightShaftDownSampleFactor", "r.SSFS",
        },
        ["/Script/Engine.RendererSettings"] = new[] { "r.RayTracing.LoadConfig" },
    };

    internal static string PatchIniContent(string content, Dictionary<string, List<(string key, string value)>> toSet)
    {
        var allManaged = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var arr in _managedPerfKeys.Values)
            foreach (var k in arr) allManaged.Add(k);

        var raw = content.Replace("\r\n", "\n").Replace("\r", "\n").TrimEnd('\n');
        var lines = raw.Length > 0 ? raw.Split('\n').ToList() : new List<string>();

        var sectionOf = new string?[lines.Count];
        string? cur = null;
        for (int i = 0; i < lines.Count; i++)
        {
            var t = lines[i].Trim();
            if (t.StartsWith('[') && t.EndsWith(']') && t.Length > 2)
                cur = t.Substring(1, t.Length - 2);
            sectionOf[i] = cur;
        }

        var result = new List<(string text, string? sec)>();
        for (int i = 0; i < lines.Count; i++)
        {
            var t = lines[i].Trim();
            var eq = t.IndexOf('=');
            if (eq > 0 && allManaged.Contains(t.Substring(0, eq).Trim())) continue;
            result.Add((lines[i], sectionOf[i]));
        }

        foreach (var (sectionName, kvList) in toSet)
        {
            if (kvList.Count == 0) continue;

            int secIdx = -1;
            for (int i = 0; i < result.Count; i++)
            {
                if (result[i].text.Trim() == $"[{sectionName}]") { secIdx = i; break; }
            }

            if (secIdx < 0)
            {
                if (result.Count > 0 && result[result.Count - 1].text.Trim() != "")
                    result.Add(("", null));
                result.Add(($"[{sectionName}]", sectionName));
                foreach (var (k, v) in kvList)
                    result.Add(($"{k}={v}", sectionName));
            }
            else
            {
                int end = secIdx + 1;
                while (end < result.Count)
                {
                    var t = result[end].text.Trim();
                    if (t.StartsWith('[') && t.EndsWith(']') && t.Length > 2) break;
                    end++;
                }
                int insert = end;
                while (insert > secIdx + 1 && result[insert - 1].text.Trim() == "") insert--;

                for (int j = kvList.Count - 1; j >= 0; j--)
                    result.Insert(insert, ($"{kvList[j].key}={kvList[j].value}", sectionName));
            }
        }

        return string.Join("\n", result.ConvertAll(x => x.text)) + "\n";
    }

    internal static IReadOnlyDictionary<string, string[]> ManagedPerfKeys => _managedPerfKeys;
}
