using FluentAssertions;
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
    public void ManualPakFileName_MatchesCurrentReleaseAsset()
    {
        Helpers.ManualPakFileName.Should().Be("WuWa_ID_99_P.pak");
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

    public void Dispose()
    {
        if (Directory.Exists(_tempDir))
            Directory.Delete(_tempDir, true);
    }
}
