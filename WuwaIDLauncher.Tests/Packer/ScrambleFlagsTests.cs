using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests.Packer;

public class ScrambleFlagsTests
{
    private static uint Scramble(uint f) =>
        ((f & 0x3fu) << 16)
        | ((f >> 6) & 0xFFFFu)
        | ((f << 6) & (1u << 28))
        | ((f >> 1) & 0x0FC00000u)
        | (f & 0xE0000000u);

    [Fact]
    public void Zero_ReturnsZero()
    {
        Scramble(0).Should().Be(0);
    }

    [Fact]
    public void KnownValue_CorrectScramble()
    {
        var result = Scramble(1);
        result.Should().Be((1u & 0x3fu) << 16);
    }

    [Fact]
    public void Deterministic()
    {
        Scramble(0x12345678).Should().Be(Scramble(0x12345678));
    }

    [Fact]
    public void DifferentInputs_DifferentOutputs()
    {
        var a = Scramble(0x01);
        var b = Scramble(0x02);
        a.Should().NotBe(b);
    }
}
