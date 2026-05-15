using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests;

public class HashTests
{
    [Fact]
    public void CorrectHash_ReturnsTrue()
    {
        var path = Path.Combine(Path.GetTempPath(), $"hash_test_{Guid.NewGuid()}.tmp");
        File.WriteAllText(path, "hello world");
        try
        {
            using var sha = System.Security.Cryptography.SHA256.Create();
            var hash = Convert.ToHexString(sha.ComputeHash(File.OpenRead(path)));
            Helpers.VerifySha256(path, hash).Should().BeTrue();
        }
        finally { File.Delete(path); }
    }

    [Fact]
    public void WrongHash_ReturnsFalse()
    {
        var path = Path.Combine(Path.GetTempPath(), $"hash_test_{Guid.NewGuid()}.tmp");
        File.WriteAllText(path, "hello world");
        try
        {
            Helpers.VerifySha256(path, "0000000000000000000000000000000000000000000000000000000000000000")
                .Should().BeFalse();
        }
        finally { File.Delete(path); }
    }

    [Fact]
    public void FileNotFound_ReturnsFalse()
    {
        Helpers.VerifySha256("/nonexistent/file.txt", "abc123").Should().BeFalse();
    }

    [Fact]
    public void CaseInsensitive_HashComparison()
    {
        var path = Path.Combine(Path.GetTempPath(), $"hash_test_{Guid.NewGuid()}.tmp");
        File.WriteAllText(path, "test data");
        try
        {
            using var sha = System.Security.Cryptography.SHA256.Create();
            var hash = Convert.ToHexString(sha.ComputeHash(File.OpenRead(path)));
            Helpers.VerifySha256(path, hash.ToLowerInvariant()).Should().BeTrue();
            Helpers.VerifySha256(path, hash.ToUpperInvariant()).Should().BeTrue();
        }
        finally { File.Delete(path); }
    }
}
