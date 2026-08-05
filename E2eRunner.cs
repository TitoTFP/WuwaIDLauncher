using System.IO;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace WuwaIDLauncher;

// End-to-end test mode. Invoked as `WuwaIDLauncher.exe --e2e`; drives the REAL
// install/update pipeline (download -> SHA-256 verify -> place -> cache -> cleanup)
// against an in-process stub server, asserts on the on-disk side effects, and
// exits 0/1 so CI can gate on it. No WebView2, no UI clicks.
internal static class E2eRunner
{
    internal static bool IsEnabled(string[] args) =>
        args.Any(a => string.Equals(a, "--e2e", StringComparison.OrdinalIgnoreCase));

    internal static async Task<int> RunCoreAsync()
    {
        var results = new List<string>();
        var failures = new List<string>();
        var game = Path.Combine(Path.GetTempPath(), "wuwaid-e2e-game-" + Guid.NewGuid().ToString("N")[..8]);

        void Check(string name, bool condition, string detail)
        {
            var line = condition ? $"[e2e] PASS {name}" : $"[e2e] FAIL {name}: {detail}";
            Console.WriteLine(line);
            results.Add(line);
            if (!condition) failures.Add($"{name}: {detail}");
        }

        try
        {
            CreateGameTree(game);
            using var stub = new E2eStubServer();
            stub.AddAsset(Helpers.PakFileName, PatternBytes("IDPAK", 4096));
            stub.AddAsset(Helpers.WinHttpLoaderFileName, PatternBytes("WINHTTP", 512));
            stub.AddAsset("WuwaIDLauncher-v9.9.9.zip", BuildUpdateZip());
            E2eConfig.BaseUrlOverride = stub.BaseUrl;

            // MainWindow is constructed but never shown; its instance flows run for real,
            // RunScript no-ops because the process is in E2E mode.
            var window = new MainWindow();
            var updateDir = Path.Combine(Path.GetTempPath(), "WuwaIDLauncher_update");

            // S1 — method1: fresh install of the canonical pak.
            await window.RunInstallation(game, "vh", backup: false, "method1");
            Check("S1.install-pak",
                File.Exists(Helpers.Method1PakPath(game)) &&
                Helpers.VerifySha256(Helpers.Method1PakPath(game), stub.ShaOf(Helpers.PakFileName)),
                "pak method1 tidak ada / hash salah");
            Check("S1.cache-method1",
                ReadCache()["_installMethod"] == "method1" &&
                ReadCache().TryGetValue(Helpers.PakFileName, out var fp) &&
                fp == stub.ShaOf(Helpers.PakFileName),
                "versions.json salah untuk method1");

            // S2 — method2: manual loader; must remove the method1 pak.
            await window.RunInstallation(game, "vh", backup: false, "method2");
            Check("S2.install-loader",
                File.Exists(Helpers.Method2LoaderPath(game)) &&
                Helpers.VerifySha256(Helpers.Method2LoaderPath(game), stub.ShaOf(Helpers.WinHttpLoaderFileName)),
                "winhttp.dll method2 tidak ada / hash salah");
            Check("S2.install-pak",
                File.Exists(Helpers.Method2PakPath(game)) &&
                Helpers.VerifySha256(Helpers.Method2PakPath(game), stub.ShaOf(Helpers.PakFileName)),
                "pak method2 tidak ada / hash salah");
            Check("S2.cleaned-method1", !File.Exists(Helpers.Method1PakPath(game)),
                "pak method1 masih ada setelah switch ke method2");
            Check("S2.cache-method2", ReadCache()["_installMethod"] == "method2",
                "versions.json tidak menyimpan method2");

            // S3 — tampered manifest: method1 install must be rejected, no file left behind.
            stub.TamperPak = true;
            await window.RunInstallation(game, "vh", backup: false, "method1");
            stub.TamperPak = false;
            Check("S3.reject-tampered", !File.Exists(Helpers.Method1PakPath(game)),
                "pak method1 terpasang walau checksum release salah");

            // S4 — method3: resource mount against a resource-ready fake game.
            await window.RunInstallation(game, "vh", backup: false, "method3");
            var plan = ResourceMountInstaller.Probe(game);
            Check("S4.probe", plan.Conflicts.Count == 0, "konflik: " + string.Join(", ", plan.Conflicts));
            Check("S4.install-artifacts",
                File.Exists(plan.PakPath) &&
                Helpers.VerifySha256(plan.PakPath, stub.ShaOf(Helpers.PakFileName)) &&
                File.Exists(plan.MountPath) &&
                File.ReadAllText(plan.MountPath).StartsWith("::Mount::", StringComparison.Ordinal),
                "artefak resource mount tidak lengkap");
            Check("S4.managed", ResourceMountInstaller.IsManaged(plan),
                "resource mount tidak terdeteksi managed");
            Check("S4.cleaned-method2",
                !File.Exists(Helpers.Method2LoaderPath(game)) &&
                !File.Exists(Helpers.Method1PakPath(game)),
                "artefak metode lain masih ada setelah method3");
            Check("S4.cache-method3", ReadCache()["_installMethod"] == "method3",
                "versions.json tidak menyimpan method3");

            // S5 — self-update: zip download + checksum verify + extract (restart skipped in E2E).
            await window.PerformLauncherUpdate("9.9.9", stub.BaseUrl + "WuwaIDLauncher-v9.9.9.zip");
            Check("S5.zip-verified",
                File.Exists(Path.Combine(updateDir, "update.zip")) &&
                Helpers.VerifySha256(Path.Combine(updateDir, "update.zip"), stub.ShaOf("WuwaIDLauncher-v9.9.9.zip")),
                "update.zip tidak ada / hash salah");
            Check("S5.extracted",
                File.Exists(Path.Combine(updateDir, "extracted", "WuwaIDLauncher.exe")),
                "zip tidak terekstrak dengan WuwaIDLauncher.exe");

            // S6 — tampered zip manifest: update must be rejected before extraction.
            stub.TamperZip = true;
            await window.PerformLauncherUpdate("9.9.9", stub.BaseUrl + "WuwaIDLauncher-v9.9.9.zip");
            stub.TamperZip = false;
            Check("S6.reject-tampered", !Directory.Exists(Path.Combine(updateDir, "extracted")),
                "update diekstrak walau checksum zip salah");
        }
        catch (Exception ex)
        {
            Console.WriteLine("[e2e] CRASHED: " + ex);
            results.Add("[e2e] CRASHED: " + ex);
            failures.Add("crash: " + ex.Message);
        }
        finally
        {
            TryDeleteDirectory(game);
            E2eConfig.BaseUrlOverride = null;
        }

        File.WriteAllLines(Path.Combine(MainWindow.AppDataFolder, "e2e-results.txt"), results);
        Console.WriteLine(failures.Count == 0
            ? "[e2e] ALL PASS"
            : $"[e2e] {failures.Count} FAILURE(S)");
        return failures.Count == 0 ? 0 : 1;
    }

    static Dictionary<string, string> ReadCache() =>
        PatchStatusEvaluator.ReadVersionCache(Path.Combine(MainWindow.AppDataFolder, "versions.json"));

    // Fake game tree: binaries + paks for methods 1/2, plus a resource-ready
    // version dir (ResManifest + Mount/MountResource.txt + Resource/Base sig/pak)
    // for method 3's probe to accept.
    static void CreateGameTree(string game)
    {
        var baseDir = Path.Combine(game, "Client", "Binaries", "Win64");
        var paks = Path.Combine(game, "Client", "Content", "Paks");
        var ver = Path.Combine(game, "Client", "Saved", "Resources", "2.0.0");
        var mount = Path.Combine(ver, "Mount");
        var baseRes = Path.Combine(ver, "Resource", "Base");
        Directory.CreateDirectory(baseDir);
        Directory.CreateDirectory(paks);
        Directory.CreateDirectory(mount);
        Directory.CreateDirectory(baseRes);
        File.WriteAllText(Path.Combine(baseDir, "Client-Win64-Shipping.exe"), "MZ");

        var pak = new byte[512];
        var sig = new byte[512];
        Random.Shared.NextBytes(pak);
        Random.Shared.NextBytes(sig);
        File.WriteAllBytes(Path.Combine(baseRes, "pakchunk7-WindowsNoEditor.pak"), pak);
        File.WriteAllBytes(Path.Combine(baseRes, "pakchunk7-WindowsNoEditor.sig"), sig);
        File.WriteAllText(Path.Combine(ver, "ResManifest"), "e2e-res-manifest");
        File.WriteAllText(Path.Combine(mount, "MountResource.txt"),
            $"Resource/Base/pakchunk7-WindowsNoEditor,,{Sha1Hex(pak)},{Sha1Hex(sig)},,\n");
    }

    static string Sha1Hex(byte[] bytes) => Convert.ToHexString(SHA1.HashData(bytes));

    static byte[] PatternBytes(string tag, int size)
    {
        var tagBytes = Encoding.UTF8.GetBytes(tag);
        var bytes = new byte[size];
        for (var i = 0; i < size; i++)
            bytes[i] = tagBytes[i % tagBytes.Length];
        return bytes;
    }

    static byte[] BuildUpdateZip()
    {
        using var ms = new MemoryStream();
        using (var archive = new ZipArchive(ms, ZipArchiveMode.Create, leaveOpen: true))
        {
            var entry = archive.CreateEntry("WuwaIDLauncher.exe");
            using var stream = entry.Open();
            stream.Write(Encoding.UTF8.GetBytes("MZ-e2e"));
        }
        return ms.ToArray();
    }

    static void TryDeleteDirectory(string path)
    {
        try { if (Directory.Exists(path)) Directory.Delete(path, recursive: true); } catch { }
    }
}
