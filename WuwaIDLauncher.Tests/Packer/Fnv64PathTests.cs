using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests.Packer;

public class Fnv64PathTests
{
    [Theory]
    [InlineData("Client/Content/Test", 0UL, 0x70f0e2a404d3c11bUL)]
    [InlineData("Client/Content/A", 0UL, 0xbb69cfe63c43eed4UL)]
    [InlineData("Client/Content/B", 0UL, 0xbb7401e63c4c984fUL)]
    [InlineData("Client/Content/Test", 42UL, 0x8bd10b4e01daacb1UL)]
    public void KnownPaths_ReturnExpectedHashes(string path, ulong seed, ulong expected)
    {
        WuwaPakPacker.Fnv64Path(path, seed).Should().Be(expected);
    }

    [Fact]
    public void CaseInsensitive()
    {
        var a = WuwaPakPacker.Fnv64Path("Client/Content/Test", 0);
        var b = WuwaPakPacker.Fnv64Path("client/content/test", 0);
        a.Should().Be(b);
    }
}
