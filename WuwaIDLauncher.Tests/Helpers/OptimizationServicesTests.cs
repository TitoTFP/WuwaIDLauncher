using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests;

public sealed class OptimizationServicesTests : IDisposable
{
    readonly string _root = Path.Combine(Path.GetTempPath(), "WuwaIDLauncherTests", Guid.NewGuid().ToString("N"));

    public OptimizationServicesTests() =>
        Directory.CreateDirectory(Helpers.GameBinaryFolderPath(_root));

    [Fact]
    public void PatchStatus_Current_WhenFilesAndFingerprintsMatch()
    {
        var path = Helpers.Method1PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "patch");
        var cache = new Dictionary<string, string>
        {
            ["_installMethod"] = "method1",
            [Helpers.PakFileName] = "remote-fingerprint"
        };

        var result = PatchStatusEvaluator.Evaluate(_root, "method1", cache,
            [new PatchAssetStatus(Helpers.PakFileName, path, "remote-fingerprint")], true);

        result.State.Should().Be("current");
        result.CanLaunch.Should().BeTrue();
    }

    [Fact]
    public void PatchStatus_UpdateAvailable_WhenFingerprintChanges()
    {
        var path = Helpers.Method1PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "patch");
        var cache = new Dictionary<string, string>
        {
            ["_installMethod"] = "method1",
            [Helpers.PakFileName] = "old"
        };

        var result = PatchStatusEvaluator.Evaluate(_root, "method1", cache,
            [new PatchAssetStatus(Helpers.PakFileName, path, "new")], true);

        result.State.Should().Be("update_available");
        result.CanLaunch.Should().BeFalse();
    }

    [Fact]
    public void PatchStatus_NotInstalled_WhenMethodTwoLoaderIsMissing()
    {
        var pak = Helpers.Method2PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(pak)!);
        File.WriteAllText(pak, "patch");

        var result = PatchStatusEvaluator.Evaluate(_root, "method2", new Dictionary<string, string>(),
            [
                new PatchAssetStatus(Helpers.ManualPakFileName, pak, "pak"),
                new PatchAssetStatus(Helpers.WinHttpLoaderFileName, Helpers.Method2LoaderPath(_root), "loader")
            ], true);

        result.State.Should().Be("not_installed");
    }

    [Fact]
    public void PatchStatus_InvalidPath_DoesNotAllowLaunch()
    {
        var missing = Path.Combine(_root, "missing-game");
        var result = PatchStatusEvaluator.Evaluate(missing, "method1", new Dictionary<string, string>(),
            [new PatchAssetStatus(Helpers.PakFileName, Helpers.Method1PakPath(missing), "remote")], true);

        result.State.Should().Be("invalid_path");
        result.CanLaunch.Should().BeFalse();
    }

    [Fact]
    public void PatchStatus_Offline_AllowsOnlyPreviouslyVerifiedFiles()
    {
        var path = Helpers.Method1PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "patch");
        var asset = new PatchAssetStatus(Helpers.PakFileName, path, "cached");

        PatchStatusEvaluator.Evaluate(_root, "method1", new Dictionary<string, string>(), [asset], false)
            .CanLaunch.Should().BeFalse();
        PatchStatusEvaluator.Evaluate(_root, "method1", new Dictionary<string, string>
        {
            ["_installMethod"] = "method1",
            [Helpers.PakFileName] = "cached"
        }, [asset], false).CanLaunch.Should().BeTrue();
    }

    [Fact]
    public void VersionCache_CorruptJson_FallsBackToEmpty()
    {
        var path = Path.Combine(_root, "versions.json");
        File.WriteAllText(path, "not json");

        PatchStatusEvaluator.ReadVersionCache(path).Should().BeEmpty();
    }

    [Fact]
    public void MediaCache_SkipsHashOnlyWhileMetadataMatches()
    {
        var path = Path.Combine(_root, "bgm.mp3");
        File.WriteAllText(path, "media");
        var state = new MediaCacheState();
        state.Record("bgm.mp3", path, "ABC");

        state.CanSkipHash("bgm.mp3", path, "abc").Should().BeTrue();
        File.AppendAllText(path, "changed");
        state.CanSkipHash("bgm.mp3", path, "abc").Should().BeFalse();
    }

    [Fact]
    public void MediaCache_MissingOrCorruptJson_FallsBackToEmpty()
    {
        var path = Path.Combine(_root, "media-cache.json");
        MediaCacheState.Load(path).Assets.Should().BeEmpty();
        File.WriteAllText(path, "{");
        MediaCacheState.Load(path).Assets.Should().BeEmpty();
    }

    public void Dispose()
    {
        if (Directory.Exists(_root)) Directory.Delete(_root, true);
    }
}
