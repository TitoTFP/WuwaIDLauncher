using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests.Packer;

public class Fnv64PathTests
{
    private ulong InvokeFnv64(string path, ulong seed = 0)
    {
        const ulong Off = 0xcbf29ce484222325UL;
        const ulong Prime = 0x00000100000001b3UL;
        ulong h = unchecked(Off + seed);
        foreach (char c in path.ToLowerInvariant())
        {
            ushort u = c;
            h ^= (byte)(u & 0xFF); h = unchecked(h * Prime);
            h ^= (byte)(u >> 8);   h = unchecked(h * Prime);
        }
        return h;
    }

    [Fact]
    public void Deterministic_SameInputSameOutput()
    {
        var a = InvokeFnv64("Client/Content/Test", 0);
        var b = InvokeFnv64("Client/Content/Test", 0);
        a.Should().Be(b);
    }

    [Fact]
    public void CaseInsensitive()
    {
        var a = InvokeFnv64("Client/Content/Test", 0);
        var b = InvokeFnv64("client/content/test", 0);
        a.Should().Be(b);
    }

    [Fact]
    public void DifferentPaths_DifferentHashes()
    {
        var a = InvokeFnv64("Client/Content/A", 0);
        var b = InvokeFnv64("Client/Content/B", 0);
        a.Should().NotBe(b);
    }

    [Fact]
    public void DifferentSeeds_DifferentHashes()
    {
        var a = InvokeFnv64("Client/Content/Test", 0);
        var b = InvokeFnv64("Client/Content/Test", 42);
        a.Should().NotBe(b);
    }
}
