using FluentAssertions;
using System.IO;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests.Packer;

public class PackFontTests
{
    [Fact]
    public void PackFont_CreatesOutputFile()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"pak_test_{Guid.NewGuid()}");
        try
        {
            Directory.CreateDirectory(tempDir);
            var fontData = new byte[] { 0x00, 0x01, 0x02, 0x03 };

            var result = WuwaPakPacker.PackFont(tempDir, "TestFont", fontData);

            File.Exists(result).Should().BeTrue();
        }
        finally
        {
            if (Directory.Exists(tempDir))
                Directory.Delete(tempDir, true);
        }
    }

    [Fact]
    public void PackFooter_ContainsMagicNumber()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"pak_test_{Guid.NewGuid()}");
        try
        {
            Directory.CreateDirectory(tempDir);
            var fontData = new byte[] { 0xAA, 0xBB, 0xCC };

            var pakPath = WuwaPakPacker.PackFont(tempDir, "MagicTest", fontData);

            using var fs = File.OpenRead(pakPath);
            fs.Seek(-200, SeekOrigin.End);
            using var br = new BinaryReader(fs);
            br.ReadUInt64(); br.ReadUInt64(); br.ReadByte();
            var magic = br.ReadUInt32();
            magic.Should().Be(0x5A6F12E1);
        }
        finally
        {
            if (Directory.Exists(tempDir))
                Directory.Delete(tempDir, true);
        }
    }

    [Fact]
    public void PackFooter_ContainsVersion12()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"pak_test_{Guid.NewGuid()}");
        try
        {
            Directory.CreateDirectory(tempDir);
            var fontData = new byte[] { 0xDD, 0xEE, 0xFF };

            var pakPath = WuwaPakPacker.PackFont(tempDir, "VersionTest", fontData);

            using var fs = File.OpenRead(pakPath);
            fs.Seek(-196, SeekOrigin.End);
            using var br = new BinaryReader(fs);
            var version = br.ReadUInt32();
            version.Should().Be(12);
        }
        finally
        {
            if (Directory.Exists(tempDir))
                Directory.Delete(tempDir, true);
        }
    }

    [Fact]
    public void PackFont_EmptyFontData_CreatesValidPak()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"pak_test_{Guid.NewGuid()}");
        try
        {
            Directory.CreateDirectory(tempDir);
            var fontData = Array.Empty<byte>();

            var pakPath = WuwaPakPacker.PackFont(tempDir, "EmptyTest", fontData);

            File.Exists(pakPath).Should().BeTrue();
            new FileInfo(pakPath).Length.Should().BeGreaterThan(0);
        }
        finally
        {
            if (Directory.Exists(tempDir))
                Directory.Delete(tempDir, true);
        }
    }
}
