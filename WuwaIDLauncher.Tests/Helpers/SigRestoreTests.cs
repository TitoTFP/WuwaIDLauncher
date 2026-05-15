using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests;

public class SigRestoreTests : IDisposable
{
    private readonly string _tempDir;
    private readonly string _fakeGamePath;
    private readonly string _pakDir;
    private readonly string _sigPath;
    private readonly string _sigBackupPath;

    public SigRestoreTests()
    {
        _tempDir = Path.Combine(Path.GetTempPath(), $"sig_test_{Guid.NewGuid()}");
        _fakeGamePath = Path.Combine(_tempDir, "Game");
        _pakDir = Path.Combine(_fakeGamePath, Helpers.PakFolderRelativePath);
        Directory.CreateDirectory(_pakDir);
        _sigPath = Path.Combine(_pakDir, Helpers.SigFileName);
        _sigBackupPath = Path.Combine(_pakDir, Helpers.SigBackupFileName);
    }

    [Fact]
    public void BackupExists_SigMissing_Restores()
    {
        File.WriteAllText(_sigBackupPath, "backup data");

        Helpers.RestoreSigBackup(_fakeGamePath);

        File.Exists(_sigPath).Should().BeTrue();
        File.Exists(_sigBackupPath).Should().BeFalse();
        File.ReadAllText(_sigPath).Should().Be("backup data");
    }

    [Fact]
    public void BothExist_DeletesBackup()
    {
        File.WriteAllText(_sigPath, "sig data");
        File.WriteAllText(_sigBackupPath, "backup data");

        Helpers.RestoreSigBackup(_fakeGamePath);

        File.Exists(_sigPath).Should().BeTrue();
        File.Exists(_sigBackupPath).Should().BeFalse();
        File.ReadAllText(_sigPath).Should().Be("sig data");
    }

    [Fact]
    public void NeitherExist_NoOp()
    {
        Action act = () => Helpers.RestoreSigBackup(_fakeGamePath);
        act.Should().NotThrow();
        File.Exists(_sigPath).Should().BeFalse();
        File.Exists(_sigBackupPath).Should().BeFalse();
    }

    [Fact]
    public void SigExists_NoBackup_NoOp()
    {
        File.WriteAllText(_sigPath, "sig data");

        Action act = () => Helpers.RestoreSigBackup(_fakeGamePath);
        act.Should().NotThrow();
        File.Exists(_sigPath).Should().BeTrue();
        File.Exists(_sigBackupPath).Should().BeFalse();
    }

    public void Dispose()
    {
        if (Directory.Exists(_tempDir))
            Directory.Delete(_tempDir, true);
    }
}
