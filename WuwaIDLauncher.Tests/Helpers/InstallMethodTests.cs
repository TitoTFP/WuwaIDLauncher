using FluentAssertions;
using System.Security.Cryptography;
using System.Text;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests;

public class InstallMethodTests : IDisposable
{
    private readonly string _tempDir;
    private readonly string _gamePath;
    private readonly string _baseDir;

    public InstallMethodTests()
    {
        _tempDir = Path.Combine(Path.GetTempPath(), $"install_method_test_{Guid.NewGuid()}");
        _gamePath = Path.Combine(_tempDir, "Game");
        _baseDir = Path.Combine(_gamePath, @"Client\Binaries\Win64");
        Directory.CreateDirectory(_baseDir);
    }

    [Fact]
    public void Method2PakPath_UsesWin64WuwaIndonesiaFolder()
    {
        var expected = Path.Combine(_baseDir, Helpers.ModFolderName, Helpers.ManualPakFileName);

        Helpers.Method2PakPath(_gamePath).Should().Be(expected);
    }

    [Fact]
    public void ManualPakFileName_RemainsMethod2LocalName()
    {
        Helpers.ManualPakFileName.Should().Be("WuWa_ID_99_P.pak");
    }

    [Fact]
    public void Method2_RemotePakIsCanonicalButLocalNameIsPreserved()
    {
        var assets = MainWindow.ExpectedPatchAssets(
            _gamePath, "method2", new Dictionary<string, string>(), useCachedFingerprint: false);

        assets.Should().ContainInOrder(
            new PatchAssetStatus(Helpers.PakFileName, Helpers.Method2PakPath(_gamePath), ""),
            new PatchAssetStatus(Helpers.WinHttpLoaderFileName, Helpers.Method2LoaderPath(_gamePath), ""));
    }

    [Fact]
    public void Method1PakPath_UsesContentPaksFolder()
    {
        var expected = Path.Combine(_gamePath, Helpers.PakFolderRelativePath, Helpers.PakFileName);

        Helpers.Method1PakPath(_gamePath).Should().Be(expected);
    }

    [Fact]
    public void AlternatePakPathForMethod_ReturnsOtherMethodPak()
    {
        Helpers.AlternatePakPathForMethod(_gamePath, "method1")
            .Should().Be(Helpers.Method2PakPath(_gamePath));
        Helpers.AlternatePakPathForMethod(_gamePath, "method2")
            .Should().Be(Helpers.Method1PakPath(_gamePath));
    }

    [Fact]
    public void Method2LoaderPath_UsesWinHttpDllNextToGameExe()
    {
        var expected = Path.Combine(_baseDir, Helpers.WinHttpLoaderFileName);

        Helpers.Method2LoaderPath(_gamePath).Should().Be(expected);
    }

    [Fact]
    public void DeleteLegacyLoaderFiles_RemovesWinHttpDll()
    {
        var winhttpPath = Path.Combine(_baseDir, Helpers.WinHttpLoaderFileName);
        File.WriteAllText(winhttpPath, "loader");

        Helpers.DeleteLegacyLoaderFiles(_baseDir);

        File.Exists(winhttpPath).Should().BeFalse();
    }

    [Fact]
    public void DeleteManualLoaderFiles_PreservesMethod2PakWhenRequested()
    {
        var manualPakPath = Helpers.Method2PakPath(_gamePath);
        Directory.CreateDirectory(Path.GetDirectoryName(manualPakPath)!);
        File.WriteAllText(manualPakPath, "pak");
        File.WriteAllText(Helpers.Method2LoaderPath(_gamePath), "loader");

        Helpers.DeleteManualLoaderFiles(_gamePath, preservePak: true);

        File.Exists(manualPakPath).Should().BeTrue();
        File.Exists(Helpers.Method2LoaderPath(_gamePath)).Should().BeFalse();
    }

    [Theory]
    [InlineData("method1")]
    [InlineData("method2")]
    public void MatchingPak_IsReusedInBothDirections(string targetMethod)
    {
        var source = Helpers.AlternatePakPathForMethod(_gamePath, targetMethod);
        var destination = targetMethod == "method2"
            ? Helpers.Method2PakPath(_gamePath)
            : Helpers.Method1PakPath(_gamePath);
        Directory.CreateDirectory(Path.GetDirectoryName(source)!);
        File.WriteAllText(source, "patch");
        var sha256 = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes("patch")));

        PatchAssetCache.TryCopyVerified(source, destination, sha256).Should().BeTrue();

        File.ReadAllText(destination).Should().Be("patch");
        File.Exists(source).Should().BeTrue("the previous method remains intact until cleanup succeeds");
    }

    [Fact]
    public void CorruptPak_IsNotReused()
    {
        var source = Helpers.Method1PakPath(_gamePath);
        var destination = Helpers.Method2PakPath(_gamePath);
        Directory.CreateDirectory(Path.GetDirectoryName(source)!);
        File.WriteAllText(source, "corrupt");
        var expected = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes("patch")));

        PatchAssetCache.TryCopyVerified(source, destination, expected).Should().BeFalse();

        File.Exists(destination).Should().BeFalse();
    }

    public void Dispose()
    {
        if (Directory.Exists(_tempDir))
            Directory.Delete(_tempDir, true);
    }
}
