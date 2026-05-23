using System.Text.Json;
using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests;

public class ActivePlayerServiceTests
{
    [Fact]
    public void BuildHeartbeatJson_DoesNotIncludePersonalData()
    {
        var json = ActivePlayerService.BuildHeartbeatJson("client-123", "2.2.0", "method2", "launch");
        using var doc = JsonDocument.Parse(json);
        var root = doc.RootElement;

        root.GetProperty("client_id").GetString().Should().Be("client-123");
        root.GetProperty("launcher_version").GetString().Should().Be("2.2.0");
        root.GetProperty("install_method").GetString().Should().Be("method2");
        root.GetProperty("event").GetString().Should().Be("launch");
        root.TryGetProperty("game_path", out _).Should().BeFalse();
        root.TryGetProperty("windows_user", out _).Should().BeFalse();
    }

    [Fact]
    public void NormalizeInstallMethod_OnlyAllowsKnownMethods()
    {
        ActivePlayerService.NormalizeInstallMethod("method2").Should().Be("method2");
        ActivePlayerService.NormalizeInstallMethod("anything").Should().Be("method1");
        ActivePlayerService.NormalizeInstallMethod(null).Should().Be("method1");
    }
}
