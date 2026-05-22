using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests.Packer;

public class ScrambleFlagsTests
{
    [Theory]
    [InlineData(0x00000000u, 0x00000000u)]
    [InlineData(0x00000001u, 0x00010000u)]
    [InlineData(0x00000002u, 0x00020000u)]
    [InlineData(0x12345678u, 0x0938d159u)]
    [InlineData(0xe1234567u, 0xe0a78d15u)]
    [InlineData(0xffffffffu, 0xffffffffu)]
    public void KnownFlags_ReturnExpectedScramble(uint flags, uint expected)
    {
        WuwaPakPacker.ScrambleFlags(flags).Should().Be(expected);
    }
}
