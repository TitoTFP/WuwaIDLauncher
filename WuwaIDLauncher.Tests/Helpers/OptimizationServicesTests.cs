using FluentAssertions;
using System.Security.Cryptography;
using System.Text;
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
                new PatchAssetStatus(Helpers.PakFileName, pak, "pak"),
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
    public void PatchStatus_CachedFirst_IsLaunchableWhileRefreshing()
    {
        var path = Helpers.Method1PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "patch");

        var result = PatchStatusEvaluator.EvaluateCached(_root, "method1",
            new Dictionary<string, string>
            {
                ["_installMethod"] = "method1",
                [Helpers.PakFileName] = "cached"
            },
            [new PatchAssetStatus(Helpers.PakFileName, path, "cached")]);

        result.State.Should().Be("cached");
        result.CanLaunch.Should().BeTrue();
        result.IsRefreshing.Should().BeTrue();
    }

    [Fact]
    public void PatchStatus_InvalidCache_RemainsDisabled()
    {
        var path = Helpers.Method1PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "patch");

        var result = PatchStatusEvaluator.EvaluateCached(_root, "method1",
            new Dictionary<string, string>(),
            [new PatchAssetStatus(Helpers.PakFileName, path, "")]);

        result.CanLaunch.Should().BeFalse();
        result.State.Should().Be("offline");
    }

    [Fact]
    public void PatchStatus_RemoteFailure_PreservesVerifiedLaunch()
    {
        var path = Helpers.Method1PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "patch");
        var result = PatchStatusEvaluator.Evaluate(_root, "method1",
            new Dictionary<string, string>
            {
                ["_installMethod"] = "method1",
                [Helpers.PakFileName] = "cached"
            },
            [new PatchAssetStatus(Helpers.PakFileName, path, "cached")],
            remoteAvailable: false);

        result.State.Should().Be("offline");
        result.CanLaunch.Should().BeTrue();
    }

    [Fact]
    public void PatchStatus_LateResultDuringLaunch_IsDiscarded()
    {
        PatchStatusDelivery.ShouldPublish(4, 4, launchInProgress: true, externalGameActive: false)
            .Should().BeFalse();
        PatchStatusDelivery.ShouldPublish(3, 4, launchInProgress: false, externalGameActive: false)
            .Should().BeFalse();
    }

    [Fact]
    public void Method1_EarlyExit_RestoresAndShowsLauncher() =>
        LaunchLifecyclePolicy.Method1(gameRunningAtDeadline: false)
            .Should().Be(Method1Completion.RestoreAndShow);

    [Fact]
    public void Method1_Deadline_RestoresThenStaysInTray() =>
        LaunchLifecyclePolicy.Method1(gameRunningAtDeadline: true)
            .Should().Be(Method1Completion.RestoreAndTray);

    [Fact]
    public void Method1_RestoreFailure_BlocksClose() =>
        LaunchLifecyclePolicy.MayCloseAfterSignatureRestore(restoreSucceeded: false)
            .Should().BeFalse();

    [Theory]
    [InlineData(true, true, true)]
    [InlineData(true, false, false)]
    [InlineData(false, true, false)]
    public void Method1_CloseDefersOnlyWhileSignatureIsPending(
        bool signaturePending, bool gameRunning, bool expected) =>
        LaunchLifecyclePolicy.ShouldDeferClose(signaturePending, gameRunning)
            .Should().Be(expected);

    [Fact]
    public async Task Method2_TwoStableSamples_Succeeds()
    {
        var samples = new Queue<bool>([false, true, true]);
        var result = await LaunchLifecyclePolicy.WaitForStableGameStartAsync(
            () => samples.Count > 0 && samples.Dequeue(),
            TimeSpan.FromMilliseconds(1),
            TimeSpan.FromSeconds(1));

        result.Should().BeTrue();
    }

    [Fact]
    public async Task Method2_Timeout_ReturnsFalse()
    {
        var result = await LaunchLifecyclePolicy.WaitForStableGameStartAsync(
            () => false,
            TimeSpan.FromMilliseconds(1),
            TimeSpan.FromMilliseconds(5));

        result.Should().BeFalse();
    }

    [Fact]
    public void VersionCache_CorruptJson_FallsBackToEmpty()
    {
        var path = Path.Combine(_root, "versions.json");
        File.WriteAllText(path, "not json");

        PatchStatusEvaluator.ReadVersionCache(path).Should().BeEmpty();
    }

    [Fact]
    public void ReleaseChecksums_ParsesCanonicalPakAndLoader()
    {
        var pakHash = new string('a', 64);
        var loaderHash = new string('B', 64);

        var result = ReleaseChecksumManifest.Parse(
            $"{pakHash}  {Helpers.PakFileName}\n{loaderHash} *{Helpers.WinHttpLoaderFileName}\n");

        result[Helpers.PakFileName].Should().Be(pakHash);
        result[Helpers.WinHttpLoaderFileName].Should().Be(loaderHash.ToLowerInvariant());
    }

    [Theory]
    [InlineData("not-a-hash  patch.pak")]
    [InlineData("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  folder/patch.pak")]
    public void ReleaseChecksums_RejectsInvalidEntries(string content) =>
        FluentActions.Invoking(() => ReleaseChecksumManifest.Parse(content))
            .Should().Throw<InvalidDataException>();

    [Fact]
    public void VersionCache_LegacyMethod2Key_IsAvailableAsCanonicalAlias()
    {
        var path = Path.Combine(_root, "versions.json");
        File.WriteAllText(path, $"{{\"{Helpers.ManualPakFileName}\":\"legacy\"}}");

        var cache = PatchStatusEvaluator.ReadVersionCache(path);

        cache[Helpers.PakFileName].Should().Be("legacy");
        cache[Helpers.ManualPakFileName].Should().Be("legacy");
    }

    [Fact]
    public void LegacyMethod2Cache_RemainsLaunchableBeforeRemoteRefresh()
    {
        var pakPath = Helpers.Method2PakPath(_root);
        var loaderPath = Helpers.Method2LoaderPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(pakPath)!);
        File.WriteAllText(pakPath, "patch");
        File.WriteAllText(loaderPath, "loader");
        var cachePath = Path.Combine(_root, "versions.json");
        File.WriteAllText(cachePath,
            $"{{\"_installMethod\":\"method2\",\"{Helpers.ManualPakFileName}\":\"legacy-pak\",\"{Helpers.WinHttpLoaderFileName}\":\"legacy-loader\"}}");
        var cache = PatchStatusEvaluator.ReadVersionCache(cachePath);
        var assets = MainWindow.ExpectedPatchAssets(_root, "method2", cache, useCachedFingerprint: true);

        var result = PatchStatusEvaluator.EvaluateCached(_root, "method2", cache, assets);

        result.State.Should().Be("cached");
        result.CanLaunch.Should().BeTrue();
    }

    [Fact]
    public void PatchCache_MigratesMatchingLegacyPakWithoutDownload()
    {
        var path = Helpers.Method2PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "patch");
        var sha256 = Sha256("patch");
        var cache = new Dictionary<string, string>
        {
            [Helpers.PakFileName] = "legacy-fingerprint",
            [Helpers.ManualPakFileName] = "legacy-fingerprint"
        };

        PatchAssetCache.PromoteVerified(cache,
            [new PatchAssetStatus(Helpers.PakFileName, path, sha256)]).Should().BeTrue();

        cache[Helpers.PakFileName].Should().Be(sha256);
        cache.Should().NotContainKey(Helpers.ManualPakFileName);
    }

    [Fact]
    public void PatchCache_DoesNotPromoteCorruptPak()
    {
        var path = Helpers.Method2PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "corrupt");
        var cache = new Dictionary<string, string> { [Helpers.PakFileName] = "legacy-fingerprint" };

        PatchAssetCache.PromoteVerified(cache,
            [new PatchAssetStatus(Helpers.PakFileName, path, Sha256("patch"))]).Should().BeFalse();

        cache[Helpers.PakFileName].Should().Be("legacy-fingerprint");
    }

    [Fact]
    public void PatchCache_RechecksCanonicalPakWhenInstallMethodChanges()
    {
        var path = Helpers.Method2PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "corrupt");
        var expected = Sha256("patch");
        var cache = new Dictionary<string, string> { [Helpers.PakFileName] = expected };

        PatchAssetCache.PromoteVerified(cache,
            [new PatchAssetStatus(Helpers.PakFileName, path, expected)],
            verifyCanonicalPak: true).Should().BeTrue();

        cache.Should().NotContainKey(Helpers.PakFileName);
    }

    [Fact]
    public void PatchCache_MetadataSkipsUnchangedFileAndInvalidatesChangedFile()
    {
        var path = Helpers.Method1PakPath(_root);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, "patch");
        var asset = new PatchAssetStatus(Helpers.PakFileName, path, Sha256("patch"));
        var cache = new Dictionary<string, string>();

        PatchAssetCache.PromoteVerified(cache, [asset]).Should().BeTrue();
        PatchAssetCache.PromoteVerified(cache, [asset]).Should().BeFalse();
        File.AppendAllText(path, "corrupt");
        PatchAssetCache.PromoteVerified(cache, [asset]).Should().BeTrue();

        cache.Should().NotContainKey(Helpers.PakFileName);
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

    static string Sha256(string content) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(content))).ToLowerInvariant();
}
