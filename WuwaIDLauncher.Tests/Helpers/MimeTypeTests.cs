using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests;

public class MimeTypeTests
{
    [Theory]
    [InlineData("index.html", "text/html; charset=utf-8")]
    [InlineData("styles.css", "text/css; charset=utf-8")]
    [InlineData("script.js", "application/javascript; charset=utf-8")]
    [InlineData("data.json", "application/json")]
    [InlineData("image.png", "image/png")]
    [InlineData("photo.jpg", "image/jpeg")]
    [InlineData("photo.jpeg", "image/jpeg")]
    [InlineData("icon.svg", "image/svg+xml")]
    [InlineData("font.woff", "font/woff")]
    [InlineData("font.woff2", "font/woff2")]
    [InlineData("bg.webp", "image/webp")]
    [InlineData("video.mp4", "video/mp4")]
    [InlineData("audio.mp3", "audio/mpeg")]
    public void KnownExtensions_ReturnCorrectMimeType(string path, string expected)
    {
        Helpers.GetMimeType(path).Should().Be(expected);
    }

    [Theory]
    [InlineData("file.xyz")]
    [InlineData("file.dat")]
    [InlineData("file")]
    [InlineData("file.exe")]
    public void UnknownExtensions_ReturnOctetStream(string path)
    {
        Helpers.GetMimeType(path).Should().Be("application/octet-stream");
    }

    [Fact]
    public void CaseInsensitive_ExtensionHandled()
    {
        Helpers.GetMimeType("file.PNG").Should().Be("image/png");
        Helpers.GetMimeType("file.MP4").Should().Be("video/mp4");
        Helpers.GetMimeType("file.CSS").Should().Be("text/css; charset=utf-8");
    }
}
