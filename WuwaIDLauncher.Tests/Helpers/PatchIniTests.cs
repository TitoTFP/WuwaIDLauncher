using FluentAssertions;
using WuwaIDLauncher;
using Xunit;

namespace WuwaIDLauncher.Tests;

public class PatchIniTests
{
    [Fact]
    public void EmptyInput_OnlyManagedKeys()
    {
        var toSet = new Dictionary<string, List<(string key, string value)>>
        {
            ["SystemSettings"] = new() { ("r.TestKey", "1") },
        };

        var result = Helpers.PatchIniContent("", toSet);

        result.Should().Contain("[SystemSettings]");
        result.Should().Contain("r.TestKey=1");
    }

    [Fact]
    public void ExistingIni_ManagedReplaced_UnmanagedKept()
    {
        var original = "[SystemSettings]\nr.ManagedKey=old\nr.UnmanagedKey=keep\n[/Script/Engine.RendererSettings]\nr.RayTracing.LoadConfig=1";

        var toSet = new Dictionary<string, List<(string key, string value)>>
        {
            ["SystemSettings"] = new() { ("r.ManagedKey", "new") },
        };

        var result = Helpers.PatchIniContent(original, toSet);

        result.Should().Contain("r.UnmanagedKey=keep");
        result.Should().Contain("r.ManagedKey=new");
        result.Should().NotContain("r.ManagedKey=old");
    }

    [Fact]
    public void SectionMissing_Created()
    {
        var original = "[OtherSection]\nr.SomeKey=value";

        var toSet = new Dictionary<string, List<(string key, string value)>>
        {
            ["SystemSettings"] = new() { ("r.NewKey", "42") },
        };

        var result = Helpers.PatchIniContent(original, toSet);

        result.Should().Contain("[SystemSettings]");
        result.Should().Contain("r.NewKey=42");
        result.Should().Contain("[OtherSection]");
        result.Should().Contain("r.SomeKey=value");
    }

    [Fact]
    public void MultipleSections_EachHandledCorrectly()
    {
        var original = "[SystemSettings]\nr.Old1=1\n[/Script/Engine.RendererSettings]\nr.Old2=2";

        var toSet = new Dictionary<string, List<(string key, string value)>>
        {
            ["SystemSettings"] = new() { ("r.New1", "a") },
            ["/Script/Engine.RendererSettings"] = new() { ("r.New2", "b") },
        };

        var result = Helpers.PatchIniContent(original, toSet);

        result.Should().Contain("r.New1=a");
        result.Should().Contain("r.New2=b");
        result.Should().NotContain("r.Old1=1");
        result.Should().NotContain("r.Old2=2");
    }

    [Fact]
    public void AllTogglesEnabled_AllKeysSet()
    {
        var ss = new List<(string key, string value)>
        {
            ("r.VRS.EnableMaterial", "false"),
            ("r.VRS.EnableMesh", "false"),
            ("r.ParallelFrustumCull", "1"),
            ("r.ParallelOcclusionCull", "1"),
            ("a.URO.ForceAnimRate", "1"),
            ("r.Upscale.Quality", "3"),
            ("r.streaming.MeshMaxKeepMips", "15"),
            ("r.streaming.TextureMaxKeepMips", "15"),
            ("foliage.DensityScaleLOD.DrawCallOptimize", "1"),
            ("r.SceneColorFringeQuality", "0"),
            ("r.Shadow.MaxCSMResolution", "256"),
            ("r.Shadow.MaxResolution", "256"),
            ("r.Shadow.MinResolution", "256"),
            ("r.Shadow.PerObjectShadowMapResolution", "256"),
            ("r.Shadow.PerObjectResolutionMax", "256"),
            ("r.Shadow.PerObjectResolutionMin", "256"),
            ("r.Shadow.RadiusThreshold", "0.06"),
            ("r.Shadow.DistanceScale", "0.5"),
            ("r.Shadow.ForbidHISMShadowStartIndex", "0"),
            ("r.SSR.MaxRoughness", "0.1"),
            ("r.SSR.HalfResSceneColor", "1"),
            ("r.AmbientOcclusionMaxQuality", "0"),
            ("r.Kuro.KuroEnableFFTBloom", "0"),
            ("r.Kuro.KuroEnableToonFFTBloom", "0"),
            ("r.DrawKuroPPLensflare", "0"),
            ("r.EnableLensflareSceneSample", "0"),
            ("r.kuro.kuroEnableScreenLeak", "0"),
            ("r.DepthOfFieldQuality", "0"),
            ("r.KuroMaterialQualityLevel", "2"),
            ("r.MaterialQualityLevel", "2"),
            ("r.DetailMode", "0"),
            ("r.Kuro.MaterialDesktopQualityShoulderRender", "0"),
            ("r.SSS.Scale", "0"),
            ("r.SSS.Quality", "0"),
            ("r.ViewDistanceScale", "0.8"),
            ("r.ScreenSizeCullRatioFactor", "10"),
            ("r.StaticMeshLODDistanceScale", "0.7"),
            ("wp.Runtime.PlannedLoadingRangeScale", "0.4"),
            ("wp.Runtime.SoraGridBlackListHeight", "5000"),
            ("foliage.CullAll", "1"),
            ("r.Kuro.Foliage.GrassCullDistanceMax", "2000"),
            ("r.Kuro.Foliage.Grass3_0CullDistanceMax", "2000"),
            ("r.Kuro.InteractionEffect.EnableFoliageEffect", "0"),
            ("r.Kuro.InteractionEffect.UseCppWaterEffect", "0"),
            ("r.EmitterSpawnRateScale", "0.125"),
            ("fx.Niagara.QualityLevel", "0"),
            ("r.ParticleLightQuality", "0"),
            ("r.KuroVolumeCloudEnable", "0"),
            ("r.KuroVolumetricLight.DownSampleFactor", "4"),
            ("r.KuroVolumetricLight.ColorMaskDownSampleFactor", "4"),
            ("r.LightShaftDownSampleFactor", "2"),
            ("r.SSFS", "0"),
        };

        var toSet = new Dictionary<string, List<(string key, string value)>>
        {
            ["SystemSettings"] = ss,
            ["/Script/Engine.RendererSettings"] = new() { ("r.RayTracing.LoadConfig", "0") },
        };

        var result = Helpers.PatchIniContent("", toSet);

        foreach (var (key, value) in ss)
        {
            result.Should().Contain($"{key}={value}", $"key {key} should be set to {value}");
        }
        result.Should().Contain("r.RayTracing.LoadConfig=0");
    }

    [Fact]
    public void DuplicateManagedKeys_Deduplicated()
    {
        var original = "[SystemSettings]\nr.TestKey=old1\nr.TestKey=old2";

        var toSet = new Dictionary<string, List<(string key, string value)>>
        {
            ["SystemSettings"] = new() { ("r.TestKey", "new") },
        };

        var result = Helpers.PatchIniContent(original, toSet);

        var lines = result.Split('\n', StringSplitOptions.RemoveEmptyEntries);
        var testKeyCount = lines.Count(l => l.Trim().StartsWith("r.TestKey="));
        testKeyCount.Should().Be(1);
        result.Should().Contain("r.TestKey=new");
    }

    [Fact]
    public void OutputEndsWithNewline()
    {
        var toSet = new Dictionary<string, List<(string key, string value)>>
        {
            ["SystemSettings"] = new() { ("r.Key", "1") },
        };

        var result = Helpers.PatchIniContent("", toSet);
        result.Should().EndWith("\n");
    }
}
